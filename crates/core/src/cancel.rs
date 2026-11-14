//! Stopping conversions and other work under way, from another thread.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::error::{Error, Result};

/// Cancels work from another thread: conversions check it between steps,
/// and an ffmpeg process one waits for is killed. It knows one process at
/// a time, the last started, so work that runs several at once needs a
/// token for each.
///
/// Fitting a WebM to the size limit can also be asked to settle: to stop
/// searching and keep the best encode that fits so far, see
/// [`CancelToken::settle`].
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    cancelled: AtomicBool,
    /// What fitting is at, whether it was asked to settle, and the process
    /// at work, under one lock: a settle is taken, and the process killed,
    /// only for the fitting it was checked against, never for the next
    /// conversion's, which may start while it is asked.
    state: Mutex<State>,
    /// Runs as a settle has been taken and not yet acted on.
    #[cfg(test)]
    settle_hook: hook::Hook,
}

#[derive(Debug, Default)]
struct State {
    fitting: Fitting,
    /// Asked to settle, while it could: until the conversion ends, though
    /// it stops work only while the search goes on.
    settling: bool,
    child: Option<Arc<Mutex<Child>>>,
}

/// What fitting a WebM is at, for the conversion under way.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Fitting {
    /// Not fitting: nothing to settle.
    #[default]
    Idle,
    /// Searching, nothing fitting yet.
    Searching,
    /// Searching, and an encode fits, so it can settle.
    Fits,
    /// The search has ended, and what it kept is written: too late to
    /// settle, though cancelling still stops it.
    Done,
}

impl CancelToken {
    /// A token nothing has cancelled yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// What it knows of fitting and the process at work: poisoning is
    /// ignored, as nothing there is left half changed.
    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Says to stop: work asks at its next step, and the ffmpeg process
    /// running for it, if one is, is killed. Every clone of the token is
    /// cancelled, for good.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        kill(&self.state());
    }

    /// Whether [`CancelToken::cancel`] was called, on it or a clone.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Whether fitting can settle now: a WebM conversion is fitting, and
    /// one of its encodes fits under the limit. Front-ends offer
    /// [`CancelToken::settle`] while this holds, as events say too
    /// ([`Event::Progress`](crate::webm::Event::Progress) and
    /// [`Event::AttemptFinished`](crate::webm::Event::AttemptFinished)).
    pub fn can_settle(&self) -> bool {
        self.state().fitting == Fitting::Fits && !self.is_cancelled()
    }

    /// Asks fitting to settle: the encode at work is abandoned, its ffmpeg
    /// process killed, and the best encode that fits so far, by the
    /// search's own ordering, is written as the result, as if the search
    /// had ended there. The outcome says it settled. Returns false, and
    /// does nothing, where it can't: when nothing fits yet, nothing is
    /// fitting, the search has ended and its result is being written, or
    /// the work was cancelled. It holds for the conversion
    /// under way only: whether it can, and what it stops, are of the same
    /// moment, so a conversion that starts as it is asked is never stopped
    /// by it.
    pub fn settle(&self) -> bool {
        let mut state = self.state();
        if state.fitting != Fitting::Fits || self.is_cancelled() {
            return false;
        }
        #[cfg(test)]
        self.inner.settle_hook.run();
        state.settling = true;
        kill(&state);
        true
    }

    /// Whether [`CancelToken::settle`] was called and took, and the work
    /// isn't cancelled since.
    pub fn is_settling(&self) -> bool {
        self.state().settling && !self.is_cancelled()
    }

    /// Cancelled, or settling while the search goes on: the step at work
    /// is to stop.
    pub(crate) fn interrupted(&self) -> bool {
        self.is_cancelled() || self.state().stops_search()
    }

    /// An error where the step at work is to stop, cancelled or settling.
    pub(crate) fn check(&self) -> Result<()> {
        if self.interrupted() { Err(Error::Cancelled) } else { Ok(()) }
    }

    /// Waits up to `most` for the work to be cancelled or asked to settle,
    /// giving whether it was: a process interrupted as Ctrl-C reaches it
    /// may end before the one who pressed it has said what it means.
    pub(crate) fn await_interruption(&self, most: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + most;
        loop {
            if self.interrupted() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// A conversion starts fitting: nothing fits yet, and nothing was
    /// asked of it.
    pub(crate) fn begin_fitting(&self) {
        let mut state = self.state();
        state.fitting = Fitting::Searching;
        state.settling = false;
    }

    /// Lets fitting settle from now on: an encode fits. Only while a
    /// conversion is fitting.
    pub(crate) fn allow_settling(&self) {
        let mut state = self.state();
        if state.fitting == Fitting::Searching {
            state.fitting = Fitting::Fits;
        }
    }

    /// The search ends, however it does: no settle is taken from now on,
    /// and none stops what follows. Gives whether one was taken, in the
    /// search's last moment as well, for the outcome to say so.
    pub(crate) fn end_search(&self) -> bool {
        let mut state = self.state();
        state.fitting = Fitting::Done;
        state.settling
    }

    /// Forgets that fitting could settle, or was asked to: a conversion
    /// ends, and the token may serve the next.
    pub(crate) fn end_fitting(&self) {
        let mut state = self.state();
        state.fitting = Fitting::Idle;
        state.settling = false;
    }

    /// The ffmpeg process to kill when cancelled, while one runs. One
    /// started as the work was asked to stop is killed at once.
    pub(crate) fn register(&self, child: Option<Arc<Mutex<Child>>>) {
        let mut state = self.state();
        state.child = child;
        if self.is_cancelled() || state.stops_search() {
            kill(&state);
        }
    }
}

impl State {
    /// Asked to settle while the search goes on, which is to stop.
    fn stops_search(&self) -> bool {
        self.settling && self.fitting != Fitting::Done
    }
}

/// Kills the process at work, if one is.
fn kill(state: &State) {
    if let Some(child) = &state.child {
        let _ = child.lock().unwrap_or_else(PoisonError::into_inner).kill();
    }
}

#[cfg(test)]
mod hook {
    use std::sync::Mutex;

    /// Something for a test to run at a given moment.
    #[derive(Default)]
    pub(super) struct Hook(Mutex<Option<Box<dyn Fn() + Send>>>);

    impl Hook {
        // its test runs processes as only Unix has them
        #[cfg_attr(not(unix), allow(dead_code))]
        pub(super) fn set(&self, hook: impl Fn() + Send + 'static) {
            *self.0.lock().unwrap() = Some(Box::new(hook));
        }

        pub(super) fn run(&self) {
            if let Some(hook) = &*self.0.lock().unwrap() {
                hook();
            }
        }
    }

    impl std::fmt::Debug for Hook {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Hook")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles_only_once_something_fits() {
        let token = CancelToken::new();
        assert!(!token.can_settle() && !token.settle() && !token.is_settling());
        // nor where nothing is fitting
        token.allow_settling();
        assert!(!token.can_settle() && !token.settle());
        token.begin_fitting();
        assert!(!token.can_settle() && !token.settle());
        token.allow_settling();
        assert!(token.can_settle());
        assert!(token.settle() && token.is_settling());
        // the step at work stops, as when cancelled, but nothing is cancelled
        assert!(token.check().is_err() && !token.is_cancelled());
        // the next conversion starts afresh
        token.end_fitting();
        assert!(!token.can_settle() && !token.is_settling() && token.check().is_ok());
        // cancelling beats settling
        token.begin_fitting();
        token.allow_settling();
        token.settle();
        token.cancel();
        assert!(!token.is_settling() && !token.can_settle() && !token.settle());
    }

    #[test]
    fn cant_settle_once_the_search_ends() {
        let token = CancelToken::new();
        token.begin_fitting();
        token.allow_settling();
        // ended: what the search kept is written, which no settle stops
        assert!(!token.end_search());
        assert!(!token.can_settle() && !token.settle() && !token.is_settling());
        assert!(token.check().is_ok());
        // nor does an encode that fits let it again
        token.allow_settling();
        assert!(!token.can_settle());
        // and taken in its last moment, it says so, and stops nothing after
        token.begin_fitting();
        token.allow_settling();
        assert!(token.settle());
        assert!(token.end_search() && token.is_settling() && token.check().is_ok());
        // cancelling still stops it
        token.cancel();
        assert!(token.check().is_err());
    }

    /// A process that runs until killed, as ffmpeg encoding.
    #[cfg(unix)]
    fn running() -> Arc<Mutex<Child>> {
        let child = std::process::Command::new("sleep").arg("60").spawn().unwrap();
        Arc::new(Mutex::new(child))
    }

    /// Whether `child` was killed, waiting for it a while if it was.
    #[cfg(unix)]
    fn killed(child: &Mutex<Child>) -> bool {
        use std::os::unix::process::ExitStatusExt;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match child.lock().unwrap().try_wait().unwrap() {
                Some(status) => return status.signal() == Some(9),
                None if std::time::Instant::now() > deadline => return false,
                None => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
    }

    /// A settle checked against one conversion, and taken a moment later
    /// on another thread, as a signal handler's, never settles the next
    /// conversion, started in that moment, nor kills its process: what is
    /// checked and what is done are of the same conversion.
    #[cfg(unix)]
    #[test]
    fn a_settle_never_reaches_the_conversion_after_the_one_it_was_for() {
        use std::sync::mpsc;
        use std::time::Duration;

        let token = CancelToken::new();
        // the first conversion fits, and encodes again
        token.begin_fitting();
        token.allow_settling();
        let (first, second) = (running(), running());
        token.register(Some(first.clone()));
        // the settle stops once it has found that it can, and waits for
        // the next conversion to start, for as long as that could take
        let (checked, has_checked) = mpsc::channel();
        let (started, has_started) = mpsc::channel::<()>();
        let has_started = Mutex::new(has_started);
        token.inner.settle_hook.set(move || {
            checked.send(()).unwrap();
            let _ = has_started.lock().unwrap().recv_timeout(Duration::from_millis(500));
        });
        let settler = std::thread::spawn({
            let token = token.clone();
            move || token.settle()
        });
        has_checked.recv().unwrap();
        // meanwhile the first conversion ends, and the next starts, with
        // nothing of it fitting yet
        let switcher = std::thread::spawn({
            let (token, second) = (token.clone(), second.clone());
            move || {
                token.register(None);
                token.end_fitting();
                token.begin_fitting();
                token.register(Some(second));
                let _ = started.send(());
            }
        });
        // taken for the first, whose process is stopped
        assert!(settler.join().unwrap());
        switcher.join().unwrap();
        assert!(killed(&first));
        // and not for the second, which goes on
        assert!(!token.is_settling() && !token.can_settle() && token.check().is_ok());
        assert!(second.lock().unwrap().try_wait().unwrap().is_none());
        // nor can it settle, as nothing of it fits
        assert!(!token.settle());
        assert!(second.lock().unwrap().try_wait().unwrap().is_none());
        token.cancel();
        assert!(killed(&second));
    }
}

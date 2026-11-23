//! Cross-thread cancellation and early settling for conversions.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::error::{Error, Result};

/// Cancels work from another thread. Conversions check the token between steps,
/// and cancelling it kills the active ffmpeg child process. Each token tracks
/// one child process at a time (the last one started), so work that runs
/// several processes at once needs a separate token for each.
///
/// During WebM size-fitting, callers can also ask the search to settle early
/// and keep the best encode that fits so far; see [`CancelToken::settle`].
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    cancelled: AtomicBool,
    /// Fitting state, settle flag and active child process under a single lock
    /// so a settle request only applies to the conversion it checked against
    /// and never leaks into the next conversion.
    state: Mutex<State>,
    /// Test hook invoked immediately after a settle check succeeds.
    #[cfg(test)]
    settle_hook: hook::Hook,
}

#[derive(Debug, Default)]
struct State {
    fitting: Fitting,
    /// True once a settle request has been accepted; stays set until the
    /// conversion ends, though it stops work only while the search is running.
    settling: bool,
    child: Option<Arc<Mutex<Child>>>,
}

/// Lifecycle phase of WebM size-fitting for the active conversion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Fitting {
    /// No size-fitting in progress.
    #[default]
    Idle,
    /// Searching, with no attempt under the size limit yet.
    Searching,
    /// Searching, and at least one attempt fits, so settling is possible.
    Fits,
    /// Search finished and the chosen output is being written; too late to
    /// settle, though cancellation still works.
    Done,
}

impl CancelToken {
    /// Creates a fresh, uncancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Locks the internal state, ignoring mutex poisoning because no state
    /// transition leaves it inconsistent.
    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Cancels the token permanently (along with all clones) and kills any
    /// active ffmpeg child process.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        kill(&self.state());
    }

    /// Returns true if [`CancelToken::cancel`] has been called on this token or
    /// any clone.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Returns true when a WebM search can settle now (at least one encode fits
    /// under the size limit and the token is not cancelled). Front-ends also
    /// receive this in [`Event::Progress`](crate::Event::Progress) and
    /// [`Event::AttemptFinished`](crate::Event::AttemptFinished).
    pub fn can_settle(&self) -> bool {
        self.state().fitting == Fitting::Fits && !self.is_cancelled()
    }

    /// Stops an active WebM search early: the encode in progress is abandoned
    /// and its ffmpeg process killed, and the best fitting encode so far (by
    /// the search's own ordering) is written, as if the search had ended there.
    /// The outcome reports that it settled.
    ///
    /// Returns `false` and does nothing if no search is running, nothing fits
    /// yet, the search has already finished, or the token is cancelled. Because
    /// the check and process kill share one lock, a settle request can never
    /// affect a subsequent conversion.
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

    /// Returns true if [`CancelToken::settle`] succeeded for the current
    /// conversion and the token has not been cancelled since.
    pub fn is_settling(&self) -> bool {
        self.state().settling && !self.is_cancelled()
    }

    /// Returns true if the current step should stop (cancelled, or settling
    /// while the search is still active).
    pub(crate) fn interrupted(&self) -> bool {
        self.is_cancelled() || self.state().stops_search()
    }

    /// Returns [`Error::Cancelled`] if [`CancelToken::interrupted`] is true.
    pub(crate) fn check(&self) -> Result<()> {
        if self.interrupted() { Err(Error::Cancelled) } else { Ok(()) }
    }

    /// Waits up to `most` for cancellation or settling. Useful when a child
    /// process exits from Ctrl-C before the signal handler thread updates the
    /// token, so the caller can return [`Error::Cancelled`] instead of an
    /// ffmpeg error.
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

    /// Marks the start of a WebM size-fitting search.
    pub(crate) fn begin_fitting(&self) {
        let mut state = self.state();
        state.fitting = Fitting::Searching;
        state.settling = false;
    }

    /// Marks that at least one encode fits under the size limit, enabling
    /// [`CancelToken::settle`] (if the search is still active).
    pub(crate) fn allow_settling(&self) {
        let mut state = self.state();
        if state.fitting == Fitting::Searching {
            state.fitting = Fitting::Fits;
        }
    }

    /// Marks the end of the search phase so further settle requests are ignored
    /// and do not interrupt writing the output. Returns whether a settle was
    /// requested during the search.
    pub(crate) fn end_search(&self) -> bool {
        let mut state = self.state();
        state.fitting = Fitting::Done;
        state.settling
    }

    /// Resets fitting state after a conversion finishes so the token can be
    /// reused.
    pub(crate) fn end_fitting(&self) {
        let mut state = self.state();
        state.fitting = Fitting::Idle;
        state.settling = false;
    }

    /// Registers the active ffmpeg child process to kill on cancellation or
    /// settle. If the token is already interrupted, kills `child` immediately.
    pub(crate) fn register(&self, child: Option<Arc<Mutex<Child>>>) {
        let mut state = self.state();
        state.child = child;
        if self.is_cancelled() || state.stops_search() {
            kill(&state);
        }
    }
}

impl State {
    fn stops_search(&self) -> bool {
        self.settling && self.fitting != Fitting::Done
    }
}

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

    /// Verifies that a settle check against one conversion cannot settle or
    /// kill a second conversion that starts concurrently.
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

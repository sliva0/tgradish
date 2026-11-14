//! Stopping conversions and other work under way, from another thread.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    /// An encode of the fitting under way fits, so it can settle.
    fits: AtomicBool,
    /// Asked to settle, while it could.
    settling: AtomicBool,
    /// Its owner takes Ctrl-C for the processes it starts.
    takes_interrupts: AtomicBool,
    child: Mutex<Option<Arc<Mutex<Child>>>>,
}

impl CancelToken {
    /// A token nothing has cancelled yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Says that its owner takes Ctrl-C itself, deciding what it means,
    /// as settling or cancelling, and stopping ffmpeg through the token:
    /// ffmpeg processes started for the work are then kept out of the
    /// terminal's process group, on Unix, so that Ctrl-C there doesn't
    /// reach them before the owner has decided. Elsewhere it changes
    /// nothing: on Windows they have no console to get Ctrl-C from.
    pub fn take_interrupts(&self) {
        self.inner.takes_interrupts.store(true, Ordering::SeqCst);
    }

    /// Whether [`CancelToken::take_interrupts`] was called.
    #[cfg(unix)]
    pub(crate) fn takes_interrupts(&self) -> bool {
        self.inner.takes_interrupts.load(Ordering::SeqCst)
    }

    /// Says to stop: work asks at its next step, and the ffmpeg process
    /// running for it, if one is, is killed. Every clone of the token is
    /// cancelled, for good.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.kill_child();
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
        self.inner.fits.load(Ordering::SeqCst) && !self.is_cancelled()
    }

    /// Asks fitting to settle: the encode at work is abandoned, its ffmpeg
    /// process killed, and the best encode that fits so far, by the
    /// search's own ordering, is written as the result, as if the search
    /// had ended there. The outcome says it settled. Returns false, and
    /// does nothing, where it can't: when nothing fits yet, nothing is
    /// fitting, or the work was cancelled. It holds for the conversion
    /// under way only.
    pub fn settle(&self) -> bool {
        if !self.can_settle() {
            return false;
        }
        self.inner.settling.store(true, Ordering::SeqCst);
        self.kill_child();
        true
    }

    /// Whether [`CancelToken::settle`] was called and took, and the work
    /// isn't cancelled since.
    pub fn is_settling(&self) -> bool {
        self.inner.settling.load(Ordering::SeqCst) && !self.is_cancelled()
    }

    /// Cancelled, or settling: the step at work is to stop.
    pub(crate) fn interrupted(&self) -> bool {
        self.is_cancelled() || self.inner.settling.load(Ordering::SeqCst)
    }

    /// An error where the step at work is to stop, cancelled or settling.
    pub(crate) fn check(&self) -> Result<()> {
        if self.interrupted() { Err(Error::Cancelled) } else { Ok(()) }
    }

    /// Lets fitting settle from now on: an encode fits.
    pub(crate) fn allow_settling(&self) {
        self.inner.fits.store(true, Ordering::SeqCst);
    }

    /// Forgets that fitting could settle, or was asked to: a conversion
    /// ends, and the token may serve the next.
    pub(crate) fn end_fitting(&self) {
        self.inner.fits.store(false, Ordering::SeqCst);
        self.inner.settling.store(false, Ordering::SeqCst);
    }

    /// The ffmpeg process to kill when cancelled, while one runs.
    pub(crate) fn register(&self, child: Option<Arc<Mutex<Child>>>) {
        *self.inner.child.lock().unwrap() = child;
    }

    fn kill_child(&self) {
        if let Some(child) = self.inner.child.lock().unwrap().as_ref() {
            let _ = child.lock().unwrap().kill();
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
        token.allow_settling();
        assert!(token.can_settle());
        assert!(token.settle() && token.is_settling());
        // the step at work stops, as when cancelled, but nothing is cancelled
        assert!(token.check().is_err() && !token.is_cancelled());
        // the next conversion starts afresh
        token.end_fitting();
        assert!(!token.can_settle() && !token.is_settling() && token.check().is_ok());
        // cancelling beats settling
        token.allow_settling();
        token.settle();
        token.cancel();
        assert!(!token.is_settling() && !token.can_settle() && !token.settle());
    }
}

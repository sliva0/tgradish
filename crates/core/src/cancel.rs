//! Stopping conversions and other work under way, from another thread.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{Error, Result};

/// Cancels work from another thread: conversions check it between steps,
/// and an ffmpeg process one waits for is killed.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    cancelled: AtomicBool,
    child: Mutex<Option<Arc<Mutex<Child>>>>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self.inner.child.lock().unwrap().as_ref() {
            let _ = child.lock().unwrap().kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(Error::Cancelled) } else { Ok(()) }
    }

    /// The ffmpeg process to kill when cancelled, while one runs.
    pub(crate) fn register(&self, child: Option<Arc<Mutex<Child>>>) {
        *self.inner.child.lock().unwrap() = child;
    }
}

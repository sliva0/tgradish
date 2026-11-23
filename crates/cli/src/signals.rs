//! Handles SIGTERM and SIGHUP (from `kill`, a service manager, or a closing
//! terminal) as a plain cancellation rather than Ctrl-C's settle-on-best
//! behaviour. This stops and reaps ffmpeg, removes temporary files and partial
//! outputs, and then re-raises the signal so the parent sees the expected
//! termination status (for example, "Terminated" in a shell or a clean stop in
//! a service manager).
//!
//! For CLI commands, Ctrl-C (SIGINT) is handled by `ctrlc` in `main.rs`. For
//! the GUI, SIGINT is handled here just like SIGTERM and SIGHUP: it closes the
//! window and exits by signal so wrapping scripts stop on Ctrl-C.
//!
//! `ctrlc` does not report which signal arrived, so this module installs its
//! own async-signal-safe handler that writes one byte to a self-pipe to wake a
//! dedicated thread.

#![allow(unsafe_code)]

use std::io::Read;
use std::os::fd::{BorrowedFd, IntoRawFd};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, Signal};

/// First termination signal received, or 0 if none has arrived.
static RECEIVED: AtomicI32 = AtomicI32::new(0);

/// Write end of the wake pipe for the termination thread, or -1 before setup.
/// Stays open for the lifetime of the process.
static WAKE: AtomicI32 = AtomicI32::new(-1);

/// Maximum wait after a termination signal before forcing exit.
///
/// Stopping normally kills ffmpeg promptly, and the GUI gives conversions 3
/// seconds; anything longer means something is stuck and must not block exit.
const AT_MOST: Duration = Duration::from_secs(10);

/// Installs handlers for SIGTERM and SIGHUP (and SIGINT when `interrupt` is
/// true, as in the GUI).
///
/// On the first signal, stdout and stderr are redirected to `/dev/null` (on
/// SIGHUP the terminal may already be gone, and the exit signal itself reports
/// why the process ended) and `stop` runs on a background thread. The caller
/// should call [`end_by_received`] once work finishes; if that takes longer
/// than [`AT_MOST`], the background thread exits by signal anyway. Signals that
/// were already ignored at startup (such as SIGHUP under `nohup` or SIGINT in a
/// background job) remain ignored.
pub fn on_termination(
    interrupt: bool,
    stop: impl FnOnce() + Send + 'static,
) -> std::io::Result<()> {
    let (mut reader, writer) = std::io::pipe()?;
    WAKE.store(writer.into_raw_fd(), Ordering::SeqCst);
    std::thread::Builder::new().name("termination".into()).spawn(move || {
        let mut byte = [0];
        if reader.read_exact(&mut byte).is_err() {
            return;
        }
        quieten();
        stop();
        std::thread::sleep(AT_MOST);
        end_by_received();
    })?;
    let action = SigAction::new(SigHandler::Handler(handle), SaFlags::SA_RESTART, SigSet::empty());
    let interrupt = interrupt.then_some(Signal::SIGINT);
    for signal in [Signal::SIGTERM, Signal::SIGHUP].into_iter().chain(interrupt) {
        // SAFETY: `handle` only performs async-signal-safe operations.
        let before = unsafe { nix::sys::signal::sigaction(signal, &action) }?;
        if matches!(before.handler(), SigHandler::SigIgn) {
            // SAFETY: restores the previous disposition unchanged.
            unsafe { nix::sys::signal::sigaction(signal, &before) }?;
        }
    }
    Ok(())
}

/// Records the first signal and wakes the termination thread.
///
/// Async-signal-safe: touches only atomics and `write(2)`, and preserves
/// `errno`. Only the first signal wakes the thread because a second signal asks
/// for the same shutdown (a closing terminal may send SIGHUP twice, and
/// pressing Ctrl-C again while the GUI closes cuts shutdown no shorter), so at
/// most one byte is ever written and `write(2)` cannot block.
extern "C" fn handle(signal: nix::libc::c_int) {
    if RECEIVED.compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return;
    }
    let errno = nix::errno::Errno::last_raw();
    let fd = WAKE.load(Ordering::SeqCst);
    if fd >= 0 {
        // SAFETY: `fd` is the write end of the wake pipe and is never closed.
        let _ = nix::unistd::write(unsafe { BorrowedFd::borrow_raw(fd) }, &[0]);
    }
    nix::errno::Errno::set_raw(errno);
}

/// Returns the first termination signal received, if any.
pub fn received() -> Option<i32> {
    Some(RECEIVED.load(Ordering::SeqCst)).filter(|&signal| signal != 0)
}

/// Terminates the process with the received signal's default action so the
/// parent sees signal termination. Does nothing if no signal was received.
pub fn end_by_received() {
    let Some(number) = received() else { return };
    if let Ok(signal) = Signal::try_from(number) {
        // SAFETY: restores the default signal disposition to terminate the
        // process.
        let _ = unsafe { nix::sys::signal::signal(signal, SigHandler::SigDfl) };
        let _ = nix::sys::signal::pthread_sigmask(
            SigmaskHow::SIG_UNBLOCK,
            Some(&SigSet::from(signal)),
            None,
        );
        let _ = nix::sys::signal::raise(signal);
    }
    // Fallback if raising failed: match shell convention (128 + signal).
    std::process::exit(128 + number);
}

/// Redirects stdout and stderr to `/dev/null`.
fn quieten() {
    use std::os::fd::AsRawFd;
    let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") else { return };
    for fd in [nix::libc::STDOUT_FILENO, nix::libc::STDERR_FILENO] {
        // SAFETY: both descriptors stay open, redirected to /dev/null.
        unsafe { nix::libc::dup2(null.as_raw_fd(), fd) };
    }
}

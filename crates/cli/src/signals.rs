//! SIGTERM and SIGHUP, as `kill`, a service manager stopping tgradish or
//! a terminal closing send: taken as a plain cancel, not as Ctrl-C's
//! keeping the best so far, so that ffmpeg is stopped and reaped, and
//! temporary files and a result cut short go as when cancelled. Then
//! tgradish ends by the signal itself, as whoever sent it expects to see:
//! a shell says "Terminated", and a service manager counts it a clean
//! stop. Ctrl-C, SIGINT, is ctrlc's in other commands, see `main.rs`; the
//! GUI, where it has no other meaning, takes it here as the other two,
//! closing as its window closes, and ends by it, as a shell expects of a
//! program Ctrl-C ended: a script running it stops there.
//!
//! ctrlc can't say which signal it caught, so these have a handler of
//! their own which, as ctrlc's, only notes the signal and wakes a thread,
//! here by a pipe; the thread does the rest.

#![allow(unsafe_code)]

use std::io::Read;
use std::os::fd::{BorrowedFd, IntoRawFd};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, Signal};

/// The signal to end by, the first that came; 0 until one does.
static RECEIVED: AtomicI32 = AtomicI32::new(0);

/// The write end of the pipe that wakes the thread acting on a signal,
/// open for as long as the program runs; -1 until there is one.
static WAKE: AtomicI32 = AtomicI32::new(-1);

/// How long after a signal tgradish ends by it whether or not it has
/// stopped by then: stopping takes a moment, killing ffmpeg rather than
/// letting it finish, and the GUI gives its conversions 3 seconds, so only
/// something stuck takes longer, which a signal to end must not wait for.
const AT_MOST: Duration = Duration::from_secs(10);

/// Takes SIGTERM and SIGHUP from now on, and SIGINT too where `interrupt`,
/// as the GUI does, calling `stop` on a thread of
/// its own as the first comes, to stop what is under way: after which
/// whoever waits for it ends the program with [`end_by_received`], or the
/// thread does, after [`AT_MOST`]. Nothing is written to stdout or stderr
/// from then on: on SIGHUP the terminal may be gone, which writes would
/// fail at, and the signal says why it ended. A signal ignored as the
/// program started, as `nohup` ignores SIGHUP and a shell SIGINT for what
/// a script runs in the background, stays ignored.
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
        // SAFETY: `handle` does only what a signal handler may
        let before = unsafe { nix::sys::signal::sigaction(signal, &action) }?;
        if matches!(before.handler(), SigHandler::SigIgn) {
            // SAFETY: as it was
            unsafe { nix::sys::signal::sigaction(signal, &before) }?;
        }
    }
    Ok(())
}

/// Notes the signal, and wakes the thread: async-signal-safe, as it
/// touches atomics and write(2) alone, and errno as it found it. Only the
/// first signal wakes it, as a second is asking for the same, which a
/// terminal closing may send HUP twice for, and Ctrl-C pressed again
/// while the GUI closes cuts no shorter; so one byte at most is ever
/// written, and the write never waits on a full pipe.
extern "C" fn handle(signal: nix::libc::c_int) {
    if RECEIVED.compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return;
    }
    let errno = nix::errno::Errno::last_raw();
    let fd = WAKE.load(Ordering::SeqCst);
    if fd >= 0 {
        // SAFETY: the pipe's write end, which is never closed
        let _ = nix::unistd::write(unsafe { BorrowedFd::borrow_raw(fd) }, &[0]);
    }
    nix::errno::Errno::set_raw(errno);
}

/// The signal that came, if one did.
pub fn received() -> Option<i32> {
    Some(RECEIVED.load(Ordering::SeqCst)).filter(|&signal| signal != 0)
}

/// Ends the program by the signal that came, if one did, as its default
/// action would have: for its parent to see that it was ended by it.
pub fn end_by_received() {
    let Some(number) = received() else { return };
    if let Ok(signal) = Signal::try_from(number) {
        // SAFETY: the default action, which ends the program, takes the
        // place of `handle`
        let _ = unsafe { nix::sys::signal::signal(signal, SigHandler::SigDfl) };
        let _ = nix::sys::signal::pthread_sigmask(
            SigmaskHow::SIG_UNBLOCK,
            Some(&SigSet::from(signal)),
            None,
        );
        let _ = nix::sys::signal::raise(signal);
    }
    // where it couldn't be raised, as shells show a program it ended
    std::process::exit(128 + number);
}

/// Sends what is still written to stdout and stderr nowhere.
fn quieten() {
    use std::os::fd::AsRawFd;
    let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") else { return };
    for fd in [nix::libc::STDOUT_FILENO, nix::libc::STDERR_FILENO] {
        // SAFETY: both stay open, now on /dev/null, which is all that
        // changes for whoever writes to them
        unsafe { nix::libc::dup2(null.as_raw_fd(), fd) };
    }
}

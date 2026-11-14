//! Running ffmpeg and ffprobe as child processes, their output read line
//! by line as they go, so that a [`CancelToken`] can kill them.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use crate::CancelToken;
use crate::error::{Error, FileAction, Result};

/// Output of a running ffmpeg.
pub(crate) enum Output {
    /// Encoded media time, from `-progress pipe:1`.
    Time { micros: u64 },
    /// Any other line of stdout.
    Stdout(String),
    /// A line of stderr.
    Line(String),
}

const STDERR_TAIL: usize = 40;

/// How long a process ended by an interrupt waits for the token to say
/// what it meant, before it counts as a failure.
const INTERRUPT_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

/// A child process started: killed and reaped if dropped before it is
/// waited for, so that a panic or an early return while it runs never
/// leaves it running.
struct Running<'a> {
    child: Arc<Mutex<Child>>,
    cancel: &'a CancelToken,
    program: &'static str,
    waited: bool,
}

impl Running<'_> {
    /// Waits for it, its output read to the end. `stderr` is the last
    /// lines of its stderr, for the error if it failed.
    fn wait(mut self, stderr: &[String]) -> Result<()> {
        self.waited = true;
        let status = self.child.lock().unwrap_or_else(PoisonError::into_inner).wait();
        self.cancel.register(None);
        let status = status.map_err(Error::io(FileAction::Run, self.program))?;
        // Ctrl-C reaches ffmpeg as it reaches tgradish, in the terminal's
        // process group: what it means, as cancelling or settling, may be
        // decided only after ffmpeg has ended for it
        if interrupted(&status) {
            self.cancel.await_interruption(INTERRUPT_GRACE);
        }
        self.cancel.check()?;
        if !status.success() {
            return Err(Error::Ffmpeg {
                program: self.program,
                status: status.to_string(),
                stderr: stderr.join("\n"),
            });
        }
        Ok(())
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if self.waited {
            return;
        }
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
        drop(child);
        self.cancel.register(None);
    }
}

/// Whether a process ended as interrupted: by SIGINT, or as ffmpeg ends
/// once it has caught it, or another signal to stop, with 255.
fn interrupted(status: &std::process::ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // SIGINT
        if status.signal() == Some(2) {
            return true;
        }
    }
    status.code() == Some(255)
}

/// Starts `cmd` with its output piped, so that cancelling kills it.
fn spawn<'a>(
    mut cmd: Command,
    program: &'static str,
    cancel: &'a CancelToken,
) -> Result<(Running<'a>, ChildStdout, ChildStderr)> {
    cancel.check()?;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // don't flash a console window when used from a GUI
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => {
            let program = std::path::PathBuf::from(cmd.get_program());
            Error::FfmpegNotFound(crate::error::FfmpegMissing::NoProgram(program))
        }
        _ => Error::io(FileAction::Run, cmd.get_program())(err),
    })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let child = Arc::new(Mutex::new(child));
    // killed at once where cancelled since the check above
    cancel.register(Some(child.clone()));
    Ok((Running { child, cancel, program, waited: false }, stdout, stderr))
}

/// Runs `cmd` until it exits, passing its output to `on_output`. ffmpeg
/// commands should use `-progress pipe:1` to report progress. Returns the
/// last lines of stderr.
pub(crate) fn run(
    cmd: Command,
    program: &'static str,
    cancel: &CancelToken,
    on_output: &mut dyn FnMut(Output),
) -> Result<Vec<String>> {
    let (child, stdout, stderr) = spawn(cmd, program, cancel)?;
    let (tx, rx) = mpsc::channel();
    let stdout_thread = spawn_reader(stdout, tx.clone(), |line| {
        match line.strip_prefix("out_time_us=").map(|micros| micros.trim().parse()) {
            Some(Ok(micros)) => Some(Output::Time { micros }),
            // N/A before the first frame
            Some(Err(_)) => None,
            None => Some(Output::Stdout(line)),
        }
    });
    let stderr_thread = spawn_reader(stderr, tx, |line| Some(Output::Line(line)));

    let mut tail = VecDeque::with_capacity(STDERR_TAIL);
    for output in rx {
        if let Output::Line(line) = &output {
            if tail.len() == STDERR_TAIL {
                tail.pop_front();
            }
            tail.push_back(line.clone());
        }
        on_output(output);
    }
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    child.wait(tail.make_contiguous())?;
    Ok(tail.into())
}

/// Runs `cmd` until it exits and returns everything it wrote to stdout,
/// which may be binary.
pub(crate) fn run_bytes(
    cmd: Command,
    program: &'static str,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    run_reading(cmd, program, cancel, |stdout| {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    })
}

/// Runs `cmd` and returns what it writes to stdout as pieces of `len`
/// bytes, such as frames of raw video, each read into its own buffer
/// rather than all into one and copied out. A last piece cut short is
/// left out.
pub(crate) fn run_pieces(
    cmd: Command,
    program: &'static str,
    len: usize,
    cancel: &CancelToken,
) -> Result<Vec<Vec<u8>>> {
    run_reading(cmd, program, cancel, |stdout| {
        let mut pieces = Vec::new();
        // pieces of nothing would never end
        if len == 0 {
            std::io::copy(stdout, &mut std::io::sink())?;
            return Ok(pieces);
        }
        loop {
            let mut piece = vec![0; len];
            match stdout.read_exact(&mut piece) {
                Ok(()) => pieces.push(piece),
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(pieces),
                Err(err) => return Err(err),
            }
        }
    })
}

/// Runs `cmd`, reading its stdout with `read`, and its stderr for the
/// tail to fail with.
fn run_reading<T>(
    cmd: Command,
    program: &'static str,
    cancel: &CancelToken,
    read: impl FnOnce(&mut ChildStdout) -> std::io::Result<T>,
) -> Result<T> {
    let (child, mut stdout, stderr) = spawn(cmd, program, cancel)?;
    let (tx, rx) = mpsc::channel();
    let stderr_thread = spawn_reader(stderr, tx, |line| Some(Output::Line(line)));
    let read = read(&mut stdout);
    let _ = stderr_thread.join();
    let lines: Vec<String> = rx
        .into_iter()
        .filter_map(|output| match output {
            Output::Line(line) => Some(line),
            _ => None,
        })
        .collect();
    child.wait(&lines[lines.len().saturating_sub(STDERR_TAIL)..])?;
    read.map_err(Error::io(FileAction::Run, program))
}

fn spawn_reader(
    stream: impl Read + Send + 'static,
    tx: mpsc::Sender<Output>,
    parse: impl Fn(String) -> Option<Output> + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        // ffmpeg may print invalid UTF-8 (file names, metadata)
        for line in reader.split(b'\n').map_while(|line| line.ok()) {
            let line = String::from_utf8_lossy(&line).trim_end().to_string();
            if let Some(output) = parse(line)
                && tx.send(output).is_err()
            {
                break;
            }
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    /// Runs `script` with `cancel`, which `then` is done to a moment
    /// after it starts, on another thread, as a signal handler would.
    fn interrupted_by(script: &str, cancel: &CancelToken, then: fn(&CancelToken)) -> Result<()> {
        let decider = std::thread::spawn({
            let cancel = cancel.clone();
            move || {
                std::thread::sleep(Duration::from_millis(150));
                then(&cancel);
            }
        });
        let ran = run(sh(script), "ffmpeg", cancel, &mut |_| {}).map(drop);
        decider.join().unwrap();
        ran
    }

    /// As ffmpeg ends on Ctrl-C, before whoever pressed it has said what
    /// it means: by the signal, or with 255 once it has caught it.
    const INTERRUPTED: [&str; 2] = ["sleep 0.05; kill -INT $$; sleep 5", "sleep 0.05; exit 255"];

    #[test]
    fn waits_a_moment_for_what_an_interrupt_meant() {
        for script in INTERRUPTED {
            // cancelling
            let cancel = CancelToken::new();
            let ran = interrupted_by(script, &cancel, CancelToken::cancel);
            assert!(matches!(ran, Err(Error::Cancelled)), "{script}: {ran:?}");
            // settling, the attempt given up
            let cancel = CancelToken::new();
            cancel.begin_fitting();
            cancel.allow_settling();
            let ran = interrupted_by(script, &cancel, |cancel| assert!(cancel.settle()));
            assert!(matches!(ran, Err(Error::Cancelled)) && cancel.is_settling(), "{script}");
            // neither, in time: it failed
            let cancel = CancelToken::new();
            let started = Instant::now();
            let ran = run(sh(script), "ffmpeg", &cancel, &mut |_| {});
            assert!(matches!(ran, Err(Error::Ffmpeg { .. })), "{script}: {ran:?}");
            assert!(started.elapsed() >= INTERRUPT_GRACE, "{script}");
        }
        // other failures fail at once
        let started = Instant::now();
        let ran = run(sh("exit 1"), "ffmpeg", &CancelToken::new(), &mut |_| {});
        assert!(matches!(ran, Err(Error::Ffmpeg { .. })));
        assert!(started.elapsed() < INTERRUPT_GRACE);
    }

    /// Whether process `pid` is there, running or not yet reaped.
    fn exists(pid: &str) -> bool {
        Command::new("kill").args(["-0", pid]).stderr(Stdio::null()).status().unwrap().success()
    }

    #[test]
    fn leaves_nothing_running_where_reading_its_output_panics() {
        let cancel = CancelToken::new();
        let mut pid = String::new();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cmd = sh("echo $$; exec sleep 60");
            let _ = run(cmd, "ffmpeg", &cancel, &mut |output| {
                if let Output::Stdout(line) = output {
                    pid = line;
                    panic!("as a front-end's callback may");
                }
            });
        }));
        assert!(panicked.is_err() && !pid.is_empty());
        // killed, and reaped: not even a zombie is left
        assert!(!exists(&pid), "{pid} still runs");
        // and the token holds no process of it
        cancel.cancel();
    }
}

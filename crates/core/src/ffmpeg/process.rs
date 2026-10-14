//! Running ffmpeg and ffprobe as child processes, their output read line
//! by line as they go, so that a [`CancelToken`] can kill them.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};

use crate::CancelToken;
use crate::error::{Error, Result};

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

/// Starts `cmd` with its output piped, so that cancelling kills it.
fn spawn(
    mut cmd: Command,
    cancel: &CancelToken,
) -> Result<(Arc<Mutex<Child>>, ChildStdout, ChildStderr)> {
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
            Error::FfmpegNotFound(format!("{} does not exist", cmd.get_program().display()))
        }
        _ => Error::Io(err),
    })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let child = Arc::new(Mutex::new(child));
    cancel.register(Some(child.clone()));
    // cancel() may have run between the check above and register()
    if cancel.is_cancelled() {
        let _ = child.lock().unwrap().kill();
    }
    Ok((child, stdout, stderr))
}

/// Waits for a child whose output is read to the end. `stderr` is the last
/// lines of its stderr, for the error if it failed.
fn wait(
    child: &Mutex<Child>,
    program: &'static str,
    cancel: &CancelToken,
    stderr: &[String],
) -> Result<()> {
    let status = child.lock().unwrap().wait();
    cancel.register(None);
    let status = status?;
    cancel.check()?;
    if !status.success() {
        return Err(Error::Ffmpeg {
            program,
            status: status.to_string(),
            stderr: stderr.join("\n"),
        });
    }
    Ok(())
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
    let (child, stdout, stderr) = spawn(cmd, cancel)?;
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
    wait(&child, program, cancel, tail.make_contiguous())?;
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
    let (child, mut stdout, stderr) = spawn(cmd, cancel)?;
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
    wait(&child, program, cancel, &lines[lines.len().saturating_sub(STDERR_TAIL)..])?;
    Ok(read?)
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

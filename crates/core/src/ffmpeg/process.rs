use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use crate::error::{Error, Result};

/// Cancels a running conversion from another thread, killing the ffmpeg
/// process it is waiting for.
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

    fn register(&self, child: Option<Arc<Mutex<Child>>>) {
        *self.inner.child.lock().unwrap() = child;
    }
}

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

/// Runs `cmd` until it exits, passing its output to `on_output`. ffmpeg
/// commands should use `-progress pipe:1` to report progress. Returns the
/// last lines of stderr.
pub(crate) fn run(
    mut cmd: Command,
    program: &'static str,
    cancel: &CancelToken,
    on_output: &mut dyn FnMut(Output),
) -> Result<Vec<String>> {
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

    let status = child.lock().unwrap().wait();
    cancel.register(None);
    let status = status?;
    cancel.check()?;
    if !status.success() {
        return Err(Error::Ffmpeg {
            program,
            status: status.to_string(),
            stderr: Vec::from(tail).join("\n"),
        });
    }
    Ok(tail.into())
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

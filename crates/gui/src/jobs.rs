//! Conversions, run on a worker thread, reporting what they do as they go.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use tgradish_core::backend::Backend;
use tgradish_core::convert::{Request, convert};
use tgradish_core::events::{Event, Params};
use tgradish_core::ffmpeg::CancelToken;
use tgradish_core::options::Options;
use tgradish_core::tgs::{self, Preview, TgsEvent, TgsOptions, TgsRequest};
use tgradish_core::{Error, telegram};
use tgradish_tgs::check::Severity;
use tgradish_tgs::normalise::Report;
use tgradish_tgs::sticker::Step;

/// What a job converts with, fixed when it starts.
pub enum Plan {
    Webm { options: Options, backend: Backend },
    Tgs { options: TgsOptions },
}

/// One encode while fitting a WebM sticker.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub number: u32,
    pub params: Params,
    /// Size, once it is done.
    pub bytes: Option<u64>,
    pub fits: bool,
    /// How much it looks like the source, from 0 to 1, when measured.
    pub ssim: Option<f64>,
}

/// What a WebM conversion was planned to make.
#[derive(Debug, Clone, PartialEq)]
pub struct WebmPlan {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub length: f64,
    pub spoofs: bool,
}

/// What a conversion is doing or did, besides its result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    pub stage: String,
    /// How far along the stage is, when that is known.
    pub fraction: Option<f32>,
    pub webm: Option<WebmPlan>,
    pub attempts: Vec<Attempt>,
    /// What reading the pixel art found.
    pub report: Option<Box<Report>>,
    /// The lossless `.tgs` size estimate, when that was too large.
    pub lossless_bytes: Option<usize>,
    pub steps: Vec<Step>,
    pub warnings: Vec<String>,
}

/// A problem Telegram would have with a result.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// Telegram refuses it, rather than only frowning on it.
    pub refused: bool,
    pub text: String,
}

#[derive(Debug)]
pub struct Done {
    pub output: PathBuf,
    pub bytes: u64,
    pub limit: u64,
    /// Changed to fit: frames or colours given up for `.tgs`.
    pub lossy: bool,
    /// For WebM: the attempt kept and whether its duration is spoofed.
    pub kept: Option<u32>,
    pub spoofed: bool,
    /// For `.tgs`: its Lottie's size, layers and rectangles.
    pub json_bytes: Option<u64>,
    pub shapes: Option<(usize, usize)>,
    pub problems: Vec<Problem>,
    /// `.tgs` results' frames; WebM ones are decoded from the file.
    pub preview: Option<Preview>,
}

#[derive(Debug)]
pub enum Status {
    Waiting,
    Running,
    Done(Box<Done>),
    Failed(String),
    /// The output exists and isn't one of ours.
    Exists(PathBuf),
    Cancelled,
}

enum Message {
    Progress(Box<dyn FnOnce(&mut Progress) + Send>),
    Finished(Result<Done, Error>),
}

/// One conversion of an item, waiting, running or finished.
pub struct Job {
    pub output: PathBuf,
    pub status: Status,
    pub progress: Progress,
    cancel: CancelToken,
    messages: Option<Receiver<Message>>,
}

impl Job {
    pub fn waiting(output: PathBuf) -> Job {
        Job {
            output,
            status: Status::Waiting,
            progress: Progress::default(),
            cancel: CancelToken::new(),
            messages: None,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, Status::Running)
    }

    pub fn is_waiting(&self) -> bool {
        matches!(self.status, Status::Waiting)
    }

    pub fn is_finished(&self) -> bool {
        !self.is_running() && !self.is_waiting()
    }

    /// Starts converting on a worker thread, which repaints `ctx` as it goes.
    pub fn start(
        &mut self,
        plan: Plan,
        inputs: Vec<PathBuf>,
        sequence: bool,
        overwrite: bool,
        ctx: &egui::Context,
    ) {
        let (sender, receiver) = channel();
        self.messages = Some(receiver);
        self.status = Status::Running;
        self.progress = Progress { stage: "starting".into(), ..Progress::default() };
        let output = self.output.clone();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let send = |message| {
                let _ = sender.send(message);
                ctx.request_repaint();
            };
            let update =
                |change: Box<dyn FnOnce(&mut Progress) + Send>| send(Message::Progress(change));
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                run(plan, inputs, sequence, output, overwrite, &cancel, &update)
            }));
            let result = result.unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("no details");
                Err(Error::InvalidOptions(format!("tgradish crashed: {message}")))
            });
            send(Message::Finished(result));
        });
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Takes in what the worker reported; true when it just finished.
    pub fn poll(&mut self) -> bool {
        let Some(messages) = &self.messages else { return false };
        let mut finished = false;
        while let Ok(message) = messages.try_recv() {
            match message {
                Message::Progress(change) => change(&mut self.progress),
                Message::Finished(result) => {
                    finished = true;
                    self.status = match result {
                        Ok(done) => Status::Done(Box::new(done)),
                        Err(Error::Cancelled) => Status::Cancelled,
                        Err(Error::OutputExists(path)) => Status::Exists(path),
                        Err(err) => Status::Failed(err.to_string()),
                    };
                }
            }
        }
        if finished {
            self.messages = None;
        }
        finished
    }
}

type Update<'a> = &'a dyn Fn(Box<dyn FnOnce(&mut Progress) + Send>);

fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

fn run(
    plan: Plan,
    inputs: Vec<PathBuf>,
    sequence: bool,
    output: PathBuf,
    overwrite: bool,
    cancel: &CancelToken,
    update: Update,
) -> Result<Done, Error> {
    if let Some(dir) = output.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    match plan {
        Plan::Webm { options, backend } => {
            webm(options, backend, &inputs[0], output, overwrite, cancel, update)
        }
        Plan::Tgs { options } => {
            let request = TgsRequest { inputs, sequence, output, options, overwrite };
            let mut found = Vec::new();
            let mut finished = None;
            let outcome = tgs::convert(&request, cancel, &mut |event| match event {
                TgsEvent::Started { report, .. } => update(Box::new(move |progress| {
                    progress.report = Some(report);
                    progress.stage = "encoding".into();
                })),
                TgsEvent::TooLarge { bytes } => update(Box::new(move |progress| {
                    progress.lossless_bytes = Some(bytes);
                    progress.stage = format!("about {} losslessly; fitting", kib(bytes as u64));
                })),
                TgsEvent::Reduced { step } => update(Box::new(move |progress| {
                    progress.steps.push(step);
                })),
                TgsEvent::Packing => update(Box::new(|progress| {
                    progress.stage = "compressing".into();
                })),
                TgsEvent::Warning { message } => update(Box::new(move |progress| {
                    progress.warnings.push(message);
                })),
                TgsEvent::Finished { issues, json_bytes, layers, rectangles, .. } => {
                    found = issues
                        .into_iter()
                        .map(|issue| Problem {
                            refused: issue.severity == Severity::Error,
                            text: issue.message,
                        })
                        .collect();
                    finished = Some((json_bytes, layers, rectangles));
                }
                TgsEvent::Error { .. } => {}
            })?;
            Ok(Done {
                output: outcome.output,
                bytes: outcome.bytes,
                limit: tgs::MAX_BYTES,
                lossy: outcome.lossy,
                kept: None,
                spoofed: false,
                json_bytes: finished.map(|(json, _, _)| json),
                shapes: finished.map(|(_, layers, rectangles)| (layers, rectangles)),
                problems: found,
                preview: Some(outcome.preview),
            })
        }
    }
}

fn webm(
    options: Options,
    backend: Backend,
    input: &Path,
    output: PathBuf,
    overwrite: bool,
    cancel: &CancelToken,
    update: Update,
) -> Result<Done, Error> {
    let limit = options.target.unwrap_or_default().max_bytes();
    let request = Request {
        input: input.to_path_buf(),
        output: Some(output),
        options,
        overwrite,
        keep_temp: false,
    };
    let mut kept = None;
    let outcome = convert(&backend, &request, cancel, &mut |event| match event {
        Event::Started { plan } => {
            let planned = WebmPlan {
                width: plan.width,
                height: plan.height,
                fps: plan.fps,
                length: plan.length,
                spoofs: plan.spoofs(plan.length),
            };
            update(Box::new(move |progress| progress.webm = Some(planned)));
        }
        Event::AttemptStarted { attempt, params } => update(Box::new(move |progress| {
            progress.stage = format!("encoding, attempt {attempt}");
            progress.fraction = Some(0.0);
            progress.attempts.push(Attempt {
                number: attempt,
                params,
                bytes: None,
                fits: false,
                ssim: None,
            });
        })),
        Event::Progress { pass, passes, fraction, .. } => {
            let done = ((f64::from(pass - 1) + fraction) / f64::from(passes)) as f32;
            update(Box::new(move |progress| progress.fraction = Some(done)));
        }
        Event::AttemptFinished { attempt, bytes, fits, .. } => update(Box::new(move |progress| {
            if let Some(entry) = progress.attempts.iter_mut().find(|a| a.number == attempt) {
                (entry.bytes, entry.fits) = (Some(bytes), fits);
            }
            progress.fraction = None;
        })),
        Event::Scored { attempt, ssim } => update(Box::new(move |progress| {
            if let Some(entry) = progress.attempts.iter_mut().find(|a| a.number == attempt) {
                entry.ssim = Some(ssim);
            }
        })),
        Event::Warning { message } => {
            update(Box::new(move |progress| progress.warnings.push(message)))
        }
        Event::Finished { attempt, .. } => kept = Some(attempt),
        _ => {}
    })?;
    Ok(Done {
        output: outcome.output,
        bytes: outcome.bytes,
        limit,
        lossy: false,
        kept,
        spoofed: outcome.spoofed,
        json_bytes: None,
        shapes: None,
        problems: outcome.issues.iter().map(problem).collect(),
        preview: None,
    })
}

fn problem(issue: &telegram::Issue) -> Problem {
    Problem { refused: true, text: issue.to_string() }
}

/// Opens the folder holding `path`, with the file selected where the
/// system can do that.
pub fn reveal(path: &Path) {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    #[cfg(target_os = "windows")]
    let command = std::process::Command::new("explorer").arg("/select,").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let command = {
        let _ = path;
        std::process::Command::new("xdg-open").arg(folder).spawn()
    };
    let _ = (command, folder);
}

//! Conversions, run on a worker thread, reporting what they do as they go.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use tgradish_core::CancelToken;
use tgradish_core::backend::Backend;
use tgradish_core::convert::{Request, convert};
use tgradish_core::events::{Event, Params};
use tgradish_core::options::Options;
use tgradish_core::presets::Format;
use tgradish_core::preview::Preview;
use tgradish_core::still;
use tgradish_core::tgs::{self, TgsEvent, TgsOptions, TgsRequest};
use tgradish_core::{Error, telegram};
use tgradish_tgs::check::{Issue, Severity};
use tgradish_tgs::normalise::Report;
use tgradish_tgs::sticker::Step;

use crate::l10n::fl;
use crate::widgets;

/// What a job converts with, fixed when it starts.
pub enum Plan {
    Webm {
        options: Options,
        backend: Backend,
    },
    Tgs {
        options: TgsOptions,
        /// ffmpeg, which reads videos.
        backend: Option<Backend>,
    },
    /// An image sticker: WebM's options, `start` the frame's time.
    Image {
        options: Options,
        backend: Backend,
    },
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
    /// How many times fitting may encode.
    pub attempts: u32,
    /// The size limit, in bytes, which sizes are shown as shares of.
    pub limit: u64,
}

/// What a `.tgs` conversion is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TgsStage {
    Reading,
    Drawing,
    Fitting,
    Compressing,
}

/// What a conversion is doing or did, besides its result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
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
    /// For `.tgs`: what it is doing.
    pub tgs: Option<TgsStage>,
    /// For `.tgs`: the art is never changed to fit.
    pub tgs_lossless: bool,
}

impl Progress {
    /// How far fitting a `.tgs` got: the size taken off so far, against
    /// what has to go, on a log scale, as reductions save shares.
    pub fn tgs_fitting(&self) -> Option<f32> {
        let lossless = self.lossless_bytes? as f64;
        let now = self.steps.last().map_or(lossless, |step| step.bytes as f64);
        let target = tgs::MAX_BYTES as f64;
        if lossless <= target {
            return Some(1.0);
        }
        Some(((lossless / now).ln() / (lossless / target).ln()).clamp(0.0, 1.0) as f32)
    }

    /// How far the whole conversion got, roughly.
    pub fn overall(&self) -> Option<f32> {
        if let Some(stage) = self.tgs {
            return Some(match stage {
                TgsStage::Reading => 0.03,
                TgsStage::Drawing => 0.15,
                TgsStage::Fitting => 0.3 + 0.5 * self.tgs_fitting().unwrap_or(0.0),
                TgsStage::Compressing => 0.85,
            });
        }
        let most = self.webm.as_ref()?.attempts.max(1) as f32;
        let done = self.attempts.iter().filter(|attempt| attempt.bytes.is_some()).count() as f32;
        let current = self.fraction.unwrap_or(0.0);
        Some(((done + current) / most).min(1.0))
    }
}

/// A problem Telegram would have with a result.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// Telegram refuses it, rather than only frowning on it.
    pub refused: bool,
    pub text: String,
}

impl Problem {
    /// Something Telegram refuses in a result for `target`.
    pub fn refused(issue: &telegram::Issue, target: telegram::Target) -> Problem {
        Problem { refused: true, text: issue_text(issue, target) }
    }

    /// What checking a `.tgs` found, which may only be frowned on.
    pub fn of_tgs(issue: Issue) -> Problem {
        Problem { refused: issue.severity == Severity::Error, text: issue.message }
    }
}

/// Telegram's `issues` with a result for `target`, as problems.
fn refused(issues: &[telegram::Issue], target: telegram::Target) -> Vec<Problem> {
    issues.iter().map(|issue| Problem::refused(issue, target)).collect()
}

/// What Telegram would refuse, in the window's words.
fn issue_text(issue: &telegram::Issue, target: telegram::Target) -> String {
    use telegram::Issue;
    match issue {
        Issue::NotWebm { doc_type } => fl!("issue-not-webm", container = doc_type.as_str()),
        Issue::NoVideo => fl!("issue-no-video"),
        Issue::NotVp9 { codec_id } => fl!("issue-not-vp9", codec = codec_id.as_str()),
        Issue::HasAudio => fl!("issue-audio"),
        &Issue::WrongSize { width, height, .. } => {
            let side = target.box_size().0;
            if target.requires_exact_size() {
                fl!("issue-size", width = width, height = height, side = side)
            } else {
                fl!("issue-size", "sticker", width = width, height = height, side = side)
            }
        }
        &Issue::TooBig { bytes, max } => {
            fl!("issue-too-big", size = widgets::kib(bytes), limit = widgets::kib(max))
        }
        &Issue::TooLong { seconds, max } => fl!(
            "issue-too-long",
            seconds = format!("{seconds:.3} s"),
            limit = widgets::seconds(max)
        ),
        &Issue::TooManyFps { fps, max } => {
            fl!("issue-too-many-fps", fps = format!("{fps:.2}"), limit = format!("{max:.0}"))
        }
        Issue::Animated => fl!("issue-animated"),
    }
}

/// What went wrong, in the window's words where people meet it, and as
/// tgradish-core says it otherwise.
pub(crate) fn failure(err: &Error) -> String {
    let name = |path: &Path| {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned());
        name.unwrap_or_else(|| path.display().to_string())
    };
    match err {
        Error::FfmpegNotFound(details) => fl!("ffmpeg-not-found", details = details.as_str()),
        Error::Ffmpeg { program, status, stderr } => {
            let program = program.to_string();
            let failed = fl!("program-failed", program = program, status = status.as_str());
            format!("{failed}\n{stderr}")
        }
        Error::Probe { path, message } => {
            fl!("unreadable-file", name = name(path), details = message.as_str())
        }
        Error::NoVideo(path) => fl!("no-video", name = name(path)),
        Error::NoFrame { path, at } => {
            fl!("no-frame-at", name = name(path), seconds = widgets::seconds(*at))
        }
        &Error::NothingFits { smallest, limit } => {
            fl!("nothing-fits", limit = widgets::kib(limit), smallest = widgets::kib(smallest))
        }
        Error::Clipboard(details) => fl!("clipboard-failed", details = details.as_str()),
        // as a preset's settings are wrong: what is wrong, without the
        // command line's "invalid options"
        Error::InvalidOptions(details) => {
            let mut chars = details.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        _ => err.to_string(),
    }
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

impl Done {
    /// What a result made earlier is, read from the file, for a window
    /// that didn't make it; `None` when it is gone.
    pub fn of_file(path: &Path, format: Format, target: telegram::Target) -> Option<Done> {
        match format {
            Format::Webm => {
                let info = tgradish_core::webm::inspect_file(path).ok()?;
                let spoofed = info.spoofed();
                Some(Done {
                    output: path.to_path_buf(),
                    bytes: info.file_size,
                    limit: target.max_bytes(),
                    lossy: false,
                    kept: None,
                    spoofed,
                    json_bytes: None,
                    shapes: None,
                    problems: refused(&telegram::check(&info, target), target),
                    preview: None,
                })
            }
            Format::Image => {
                let info = still::inspect_file(path).ok()?;
                let issues = info.issues(target);
                Some(Done {
                    output: path.to_path_buf(),
                    bytes: info.bytes,
                    limit: telegram::MAX_STILL_BYTES,
                    lossy: false,
                    kept: None,
                    spoofed: false,
                    json_bytes: None,
                    shapes: None,
                    problems: refused(&issues, target),
                    preview: None,
                })
            }
            Format::Tgs => {
                let (stats, issues) = tgs::inspect_file(path).ok()?;
                Some(Done {
                    output: path.to_path_buf(),
                    bytes: stats.tgs_bytes.unwrap_or_default() as u64,
                    limit: tgs::MAX_BYTES,
                    lossy: false,
                    kept: None,
                    spoofed: false,
                    json_bytes: Some(stats.json_bytes as u64),
                    shapes: Some((stats.layers, stats.shapes)),
                    problems: issues.into_iter().map(Problem::of_tgs).collect(),
                    preview: None,
                })
            }
        }
    }
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
    /// The conversion panicked, as this says.
    Crashed(String),
}

/// One conversion of an item, waiting, running or finished.
pub struct Job {
    pub output: PathBuf,
    /// The output replaces a file made at another time.
    pub replaces: bool,
    pub status: Status,
    pub progress: Progress,
    cancel: CancelToken,
    messages: Option<Receiver<Message>>,
}

impl Job {
    pub fn waiting(output: PathBuf) -> Job {
        Job {
            output,
            replaces: false,
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
        self.progress = Progress::default();
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
            send(match result {
                Ok(result) => Message::Finished(result),
                Err(panic) => Message::Crashed(
                    panic
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                        .or_else(|| panic.downcast_ref::<String>().cloned())
                        .unwrap_or_default(),
                ),
            });
        });
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Asked to stop, and not yet stopped.
    pub fn is_stopping(&self) -> bool {
        self.is_running() && self.cancel.is_cancelled()
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
                        Err(err) => Status::Failed(failure(&err)),
                    };
                }
                Message::Crashed(details) => {
                    finished = true;
                    self.status = Status::Failed(fl!("crashed", details = details));
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
    // the frames of one animation, which pixel art drawing joins
    let frames = (sequence || inputs.len() > 1).then(|| inputs.clone());
    match plan {
        Plan::Webm { options, backend } => {
            let request = Request {
                input: inputs[0].clone(),
                sequence: frames,
                output: Some(output),
                options,
                overwrite,
                keep_temp: false,
            };
            webm(&request, backend, cancel, update)
        }
        Plan::Image { options, backend } => {
            let request = Request {
                input: inputs[0].clone(),
                sequence: frames,
                output: Some(output),
                options,
                overwrite,
                keep_temp: false,
            };
            let target = request.options.target.unwrap_or_default();
            let outcome = still::convert(&backend, &request, cancel)?;
            let warnings = outcome.warnings.clone();
            update(Box::new(move |progress| progress.warnings.extend(warnings)));
            let preview = still::preview(&outcome.output).ok();
            Ok(Done {
                output: outcome.output,
                bytes: outcome.bytes,
                limit: telegram::MAX_STILL_BYTES,
                lossy: matches!(outcome.encoding, still::Encoding::Lossy { .. }),
                kept: None,
                spoofed: false,
                json_bytes: None,
                shapes: None,
                problems: refused(&outcome.issues, target),
                preview,
            })
        }
        Plan::Tgs { options, backend } => {
            let lossless = options.lossless.unwrap_or(tgs::LOSSLESS);
            let request = TgsRequest { inputs, sequence, output, options, overwrite, backend };
            update(Box::new(move |progress| {
                progress.tgs = Some(TgsStage::Reading);
                progress.tgs_lossless = lossless;
            }));
            let outcome = tgs::convert(&request, cancel, &mut |event| match event {
                TgsEvent::Started { report, .. } => update(Box::new(move |progress| {
                    progress.report = Some(report);
                    progress.tgs = Some(TgsStage::Drawing);
                })),
                TgsEvent::TooLarge { bytes } => update(Box::new(move |progress| {
                    progress.lossless_bytes = Some(bytes);
                    progress.tgs = Some(TgsStage::Fitting);
                })),
                TgsEvent::Reduced { step } => update(Box::new(move |progress| {
                    progress.steps.push(step);
                    progress.tgs = Some(TgsStage::Fitting);
                })),
                TgsEvent::Packing => update(Box::new(|progress| {
                    progress.tgs = Some(TgsStage::Compressing);
                })),
                TgsEvent::Warning { message } => update(Box::new(move |progress| {
                    progress.warnings.push(message);
                })),
                TgsEvent::Finished { .. } | TgsEvent::Error { .. } => {}
            })?;
            Ok(Done {
                output: outcome.output,
                bytes: outcome.bytes,
                limit: tgs::MAX_BYTES,
                lossy: outcome.lossy,
                kept: None,
                spoofed: false,
                json_bytes: Some(outcome.json_bytes),
                shapes: Some((outcome.layers, outcome.rectangles)),
                problems: outcome.issues.into_iter().map(Problem::of_tgs).collect(),
                preview: Some(outcome.preview),
            })
        }
    }
}

fn webm(
    request: &Request,
    backend: Backend,
    cancel: &CancelToken,
    update: Update,
) -> Result<Done, Error> {
    let limit = request.options.target.unwrap_or_default().max_bytes();
    let outcome = convert(&backend, request, cancel, &mut |event| match event {
        Event::Started { plan } => {
            let planned = WebmPlan {
                width: plan.width,
                height: plan.height,
                fps: plan.fps,
                length: plan.length,
                attempts: if plan.fit == tgradish_core::options::Fit::Off {
                    1
                } else {
                    plan.attempts
                },
                limit,
            };
            update(Box::new(move |progress| progress.webm = Some(planned)));
        }
        Event::AttemptStarted { attempt, params } => update(Box::new(move |progress| {
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
        _ => {}
    })?;
    Ok(Done {
        output: outcome.output,
        bytes: outcome.bytes,
        limit,
        lossy: false,
        kept: Some(outcome.attempt),
        spoofed: outcome.spoofed,
        json_bytes: None,
        shapes: None,
        problems: refused(&outcome.issues, request.options.target.unwrap_or_default()),
        preview: None,
    })
}

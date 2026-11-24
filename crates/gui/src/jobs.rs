//! Background conversion jobs running on worker threads and streaming progress
//! updates back to the GUI.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use tgradish_core::CancelToken;
use tgradish_core::backend::Backend;
use tgradish_core::convert;
use tgradish_core::events::{Details, StartDetails};
use tgradish_core::input::Input;
use tgradish_core::options::Settings;
use tgradish_core::presets::Format;
use tgradish_core::presets::FormatOptions;
use tgradish_core::tgradish_tgs::check::Severity;
use tgradish_core::tgradish_tgs::normalise::Report;
use tgradish_core::tgradish_tgs::sticker::Step;
use tgradish_core::tgs;
use tgradish_core::webm::{Decision, Params};
use tgradish_core::webp;
use tgradish_core::{Error, Event, FfmpegMissing, Outcome, Started, telegram};

use crate::l10n::fl;
use crate::widgets;

/// What a job converts with, fixed when it starts: what to make and how,
/// and ffmpeg, which WebM and static stickers need, and `.tgs` only to
/// read videos.
pub(crate) struct Setup {
    pub(crate) settings: Settings<FormatOptions>,
    pub(crate) backend: Option<Backend>,
    /// How many threads it uses, as Settings says.
    pub(crate) threads: tgradish_core::config::Threads,
}

/// One encode while fitting a WebM sticker.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attempt {
    pub(crate) number: u32,
    pub(crate) params: Params,
    /// Size, once it is done.
    pub(crate) bytes: Option<u64>,
    pub(crate) fits: bool,
    /// How much it looks like the source, from 0 to 1, when measured.
    pub(crate) ssim: Option<f64>,
}

/// What a WebM conversion was planned to make.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WebmPlan {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: f64,
    pub(crate) length: f64,
    /// How many times fitting may encode.
    pub(crate) attempts: u32,
    /// The size limit, in bytes, which sizes are shown as shares of.
    pub(crate) limit: u64,
}

/// What a `.tgs` conversion is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TgsStage {
    Reading,
    Drawing,
    Fitting,
    Compressing,
}

/// What a conversion is doing or did, besides its result.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Progress {
    /// How far along the stage is, when that is known.
    pub(crate) fraction: Option<f32>,
    pub(crate) webm: Option<WebmPlan>,
    pub(crate) attempts: Vec<Attempt>,
    /// Why fitting a WebM stopped where it did, or tried a lower frame
    /// rate.
    pub(crate) decisions: Vec<Decision>,
    /// What reading the pixel art found.
    pub(crate) report: Option<Box<Report>>,
    /// The `.tgs` size estimate with the art unchanged, when that was too
    /// large.
    pub(crate) unchanged_bytes: Option<usize>,
    pub(crate) steps: Vec<Step>,
    pub(crate) warnings: Vec<String>,
    /// For `.tgs`: what it is doing.
    pub(crate) tgs: Option<TgsStage>,
    /// For `.tgs`: the art is never changed to fit.
    pub(crate) tgs_exact: bool,
    /// For `.tgs`: how much of making it is done, and of the stage it is
    /// at or the try again under way, from 0 to 1, as converting says;
    /// nothing while it is read.
    pub(crate) tgs_done: f32,
    pub(crate) stage_done: f32,
    /// For `.tgs`: each try that came out too large all the same, in
    /// order, the first that of the stages: the size it was compressed to,
    /// or none where it was too large to be worth compressing.
    pub(crate) missed: Vec<Option<u64>>,
    /// For `.tgs`: the estimate each try again after a miss aimed at, in
    /// bytes, the second try first.
    pub(crate) retries: Vec<u64>,
    /// For `.tgs`: how many of the steps the first try took, once it
    /// missed; tries again add theirs after them.
    pub(crate) first_steps: Option<usize>,
}

impl Progress {
    /// How far the whole conversion got, roughly.
    pub(crate) fn overall(&self) -> Option<f32> {
        if self.tgs.is_some() {
            return Some(self.tgs_done);
        }
        let most = self.webm.as_ref()?.attempts.max(1) as f32;
        let done = self.attempts.iter().filter(|attempt| attempt.bytes.is_some()).count() as f32;
        let current = self.fraction.unwrap_or(0.0);
        Some(((done + current) / most).min(1.0))
    }
}

/// A problem Telegram would have with a result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Problem {
    /// Telegram refuses it, rather than only frowning on it.
    pub(crate) refused: bool,
    pub(crate) text: String,
}

impl Problem {
    /// What Telegram would have with a result or a file, in the GUI's
    /// words where it has them: refused, or only frowned on.
    pub(crate) fn of(issue: &telegram::Issue) -> Problem {
        Problem { refused: issue.severity() == Severity::Error, text: issue_text(issue) }
    }
}

/// Telegram's `issues` with a result, as problems.
pub(crate) fn problems(issues: &[telegram::Issue]) -> Vec<Problem> {
    issues.iter().map(Problem::of).collect()
}

/// What Telegram would have with a result, in the GUI's words: of the
/// kinds every format has, its own; of what only a `.tgs` is checked for,
/// as core says it.
fn issue_text(issue: &telegram::Issue) -> String {
    use telegram::Issue;
    match issue {
        Issue::NotWebm { doc_type } => fl!("issue-not-webm", container = doc_type.as_str()),
        Issue::NoVideo => fl!("issue-no-video"),
        Issue::NotVp9 { codec_id } => fl!("issue-not-vp9", codec = codec_id.as_str()),
        Issue::HasAudio => fl!("issue-audio"),
        &Issue::WrongSize { width, height, side, exact } => {
            if exact {
                fl!("issue-size", width = width, height = height, side = side)
            } else {
                fl!("issue-size", "sticker", width = width, height = height, side = side)
            }
        }
        &Issue::TooBig { bytes, max } => {
            let (size, limit) = sizes(bytes, max);
            fl!("issue-too-big", size = size, limit = limit)
        }
        &Issue::TooLong { seconds, max } => fl!(
            "issue-too-long",
            seconds = format!("{seconds:.3} s"),
            limit = widgets::seconds(max)
        ),
        &Issue::TooManyFps { fps, max } => {
            fl!("issue-too-many-fps", fps = widgets::trimmed(fps), limit = widgets::trimmed(max))
        }
        Issue::Animated => fl!("issue-animated"),
        // what only a .tgs is checked for, and what the GUI has no words
        // of its own for yet, as core says it
        issue => issue.to_string(),
    }
}

/// `bytes` against a `limit`, in KiB, or in bytes where KiB round both
/// to the same.
fn sizes(bytes: u64, limit: u64) -> (String, String) {
    let (size, most) = (widgets::kib(bytes), widgets::kib(limit));
    if size == most { (format!("{bytes} B"), format!("{limit} B")) } else { (size, most) }
}

/// What went wrong, in the GUI's words where people meet it, and as
/// tgradish-core says it otherwise.
pub(crate) fn failure(err: &Error) -> String {
    let name = |path: &Path| {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned());
        name.unwrap_or_else(|| path.display().to_string())
    };
    match err {
        Error::FfmpegNotFound(FfmpegMissing::NotOnPath) => fl!("ffmpeg-not-on-path"),
        Error::FfmpegNotFound(missing) => fl!("ffmpeg-not-found", details = missing.to_string()),
        Error::FfmpegArgsNeedProgram => fl!("ffmpeg-args-need-program"),
        Error::Ffmpeg { program, status, stderr } => {
            let program = program.to_string();
            let failed = fl!("program-failed", program = program, status = status.as_str());
            format!("{failed}\n{stderr}")
        }
        Error::Unreadable { path, message } => {
            fl!("unreadable-file", name = name(path), details = message.as_str())
        }
        Error::NoVideo(path) => fl!("no-video", name = name(path)),
        Error::NoSuchFile(path) => fl!("no-such-file", name = name(path)),
        Error::Undrawable { .. } => fl!("tgs-unseen"),
        Error::NotWebm(path) => fl!("not-webm", name = name(path)),
        Error::NotSticker(path) => fl!("not-sticker", name = name(path)),
        Error::NoFrame { path, at } => {
            fl!("no-frame-at", name = name(path), seconds = widgets::seconds(*at))
        }
        &Error::NothingFits { smallest, limit } => {
            let (smallest, limit) = sizes(smallest, limit);
            fl!("nothing-fits", limit = limit, smallest = smallest)
        }
        Error::Clipboard(tgradish_core::ClipboardError::Empty) => fl!("nothing-to-paste", "why"),
        Error::Clipboard(details) => fl!("clipboard-failed", details = details.to_string()),
        Error::OutputIsInput(path) => fl!("output-is-input", name = name(path)),
        Error::NothingToConvert => fl!("nothing-to-convert"),
        // as a preset's settings are wrong: what is wrong, as core says it
        Error::InvalidOption { .. } | Error::Preset(_) | Error::Config { .. } => {
            let details = err.to_string();
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
pub(crate) struct Done {
    pub(crate) output: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) limit: u64,
    /// Changed to fit, as core says: frames or colours given up for
    /// `.tgs`, or a picture saved lossy where lossless didn't fit.
    pub(crate) changed: bool,
    /// For WebM: the attempt that became the result, and whether its
    /// duration is spoofed.
    pub(crate) attempt: Option<u32>,
    pub(crate) spoofed: bool,
    /// For WebM: fitting was asked to stop early, and kept the best
    /// attempt that fit so far.
    pub(crate) settled: bool,
    /// For `.tgs`: its Lottie's size, layers and rectangles.
    pub(crate) json_bytes: Option<u64>,
    pub(crate) shapes: Option<(usize, usize)>,
    pub(crate) problems: Vec<Problem>,
    /// What a `.tgs` made now shows, kept as its art until the result's
    /// frames are first drawn from it, after which they are drawn from the
    /// file again, as other results' are always read from theirs.
    pub(crate) drawing: Option<tgs::Drawing>,
}

impl Done {
    /// What converting made, as core says it.
    fn of(outcome: Outcome) -> Done {
        let mut done = Done {
            output: outcome.output,
            bytes: outcome.bytes,
            limit: outcome.limit,
            // a WebM's notes say what fitting kept of its frame rate and
            // length instead
            changed: outcome.changed && outcome.format != Format::Webm,
            attempt: None,
            spoofed: false,
            settled: false,
            json_bytes: None,
            shapes: None,
            problems: problems(&outcome.issues),
            drawing: None,
        };
        match outcome.details {
            Details::Webm { attempt, spoofed, settled, .. } => {
                (done.attempt, done.spoofed, done.settled) = (Some(attempt), spoofed, settled);
            }
            Details::Tgs { json_bytes, layers, rectangles, drawing, .. } => {
                done.json_bytes = Some(json_bytes);
                done.shapes = Some((layers as usize, rectangles as usize));
                done.drawing = Some(drawing);
            }
            _ => {}
        }
        done
    }

    /// What a result made earlier is, read from the file, for an
    /// instance that didn't make it; `None` when it is gone.
    pub(crate) fn of_file(path: &Path, format: Format, target: telegram::Target) -> Option<Done> {
        match format {
            Format::Webm => {
                let info = tgradish_core::webm::container::inspect_file(path).ok()?;
                let spoofed = info.spoofed();
                Some(Done {
                    output: path.to_path_buf(),
                    bytes: info.file_size,
                    limit: format.max_bytes(target),
                    changed: false,
                    attempt: None,
                    spoofed,
                    settled: false,
                    json_bytes: None,
                    shapes: None,
                    problems: problems(&telegram::check(&info, target)),
                    drawing: None,
                })
            }
            Format::Webp => {
                let info = webp::inspect_file(path).ok()?;
                let issues = info.issues(target);
                Some(Done {
                    output: path.to_path_buf(),
                    bytes: info.bytes,
                    limit: format.max_bytes(target),
                    changed: false,
                    attempt: None,
                    spoofed: false,
                    settled: false,
                    json_bytes: None,
                    shapes: None,
                    problems: problems(&issues),
                    drawing: None,
                })
            }
            Format::Tgs => {
                let (stats, issues) = tgs::inspect_file(path).ok()?;
                Some(Done {
                    output: path.to_path_buf(),
                    bytes: stats.tgs_bytes.unwrap_or_default() as u64,
                    limit: format.max_bytes(target),
                    changed: false,
                    attempt: None,
                    spoofed: false,
                    settled: false,
                    json_bytes: Some(stats.json_bytes as u64),
                    shapes: Some((stats.layers, stats.shapes)),
                    problems: problems(&issues),
                    drawing: None,
                })
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum Status {
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

/// One conversion of an item, waiting, running or finished. Dropped, it
/// stops converting.
pub(crate) struct Job {
    pub(crate) output: PathBuf,
    /// The output replaces a file made at another time.
    pub(crate) replaces: bool,
    pub(crate) status: Status,
    pub(crate) progress: Progress,
    /// It fits a WebM in several encodes, and can keep the best so far.
    pub(crate) settles: bool,
    cancel: CancelToken,
    messages: Option<Receiver<Message>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Job {
    pub(crate) fn waiting(output: PathBuf) -> Job {
        Job {
            output,
            replaces: false,
            status: Status::Waiting,
            progress: Progress::default(),
            settles: false,
            cancel: CancelToken::new(),
            messages: None,
            worker: None,
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        matches!(self.status, Status::Running)
    }

    pub(crate) fn is_waiting(&self) -> bool {
        matches!(self.status, Status::Waiting)
    }

    pub(crate) fn is_finished(&self) -> bool {
        !self.is_running() && !self.is_waiting()
    }

    /// Starts converting on a worker thread, which repaints `ctx` as it goes.
    pub(crate) fn start(
        &mut self,
        setup: Setup,
        input: Input,
        overwrite: bool,
        ctx: &egui::Context,
    ) {
        let (sender, receiver) = channel();
        self.messages = Some(receiver);
        self.status = Status::Running;
        self.progress = Progress::default();
        self.settles = matches!(&setup.settings.options, FormatOptions::Webm(options)
            if options.fit_or_default() != tgradish_core::options::Fit::Off);
        let output = self.output.clone();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.worker = Some(std::thread::spawn(move || {
            let send = |message| {
                let _ = sender.send(message);
                ctx.request_repaint();
            };
            let update =
                |change: Box<dyn FnOnce(&mut Progress) + Send>| send(Message::Progress(change));
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                run(setup, input, output, overwrite, &cancel, &update)
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
        }));
    }

    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Waits until `deadline` at most for the worker to end, as it does
    /// once it has cleaned up after a conversion stopped; true if it did.
    pub(crate) fn wait_until(&mut self, deadline: std::time::Instant) -> bool {
        let Some(worker) = &self.worker else { return true };
        while !worker.is_finished() {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = self.worker.take().map(std::thread::JoinHandle::join);
        true
    }

    /// Asked to stop, and not yet stopped.
    pub(crate) fn is_stopping(&self) -> bool {
        self.is_running() && self.cancel.is_cancelled()
    }

    /// Whether it can keep the best encode so far now: one fits.
    pub(crate) fn can_settle(&self) -> bool {
        self.is_running() && self.cancel.can_settle()
    }

    /// Stops fitting and keeps the best encode so far, where one fits.
    pub(crate) fn settle(&self) {
        self.cancel.settle();
    }

    /// Asked to keep the best so far, and not yet done.
    pub(crate) fn is_settling(&self) -> bool {
        self.is_running() && self.cancel.is_settling()
    }

    /// Takes in what the worker reported; true when it just finished.
    pub(crate) fn poll(&mut self) -> bool {
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
    setup: Setup,
    input: Input,
    output: PathBuf,
    overwrite: bool,
    cancel: &CancelToken,
    update: Update,
) -> Result<Done, Error> {
    if let Some(dir) = output.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|error| Error::Io {
            action: tgradish_core::FileAction::Create,
            path: dir.to_path_buf(),
            error,
        })?;
    }
    let Setup { mut settings, backend, threads } = setup;
    if let FormatOptions::Tgs(options) = &settings.options {
        let exact = options.exact_or_default();
        // a sheet of a file of frames of its own, which converting
        // refuses: read as it is, and said so, as the GUI shows it
        let reading = Settings { selection: settings.selection.clone(), options: options.clone() };
        if let Some((_, frames)) =
            crate::media::animated(backend.as_ref(), &input, &reading, cancel)
        {
            (settings.selection.sheet, settings.selection.sheet_frames) = (None, None);
            let said = fl!("sheet-refused", "converted", frames = frames);
            update(Box::new(move |progress| progress.warnings.push(said)));
        }
        update(Box::new(move |progress| {
            progress.tgs = Some(TgsStage::Reading);
            progress.tgs_exact = exact;
        }));
    }
    let (output, keep_temp) = (Some(output), false);
    let request = convert::Request { input, output, settings, overwrite, keep_temp, threads };
    let outcome =
        convert::convert(backend.as_ref(), &request, cancel, &mut |event| follow(event, update))?;
    Ok(Done::of(outcome))
}

/// Puts a `.tgs` conversion at `stage`, which it says how far it got of
/// from the start.
fn at_stage(progress: &mut Progress, stage: TgsStage) {
    if progress.tgs != Some(stage) {
        progress.stage_done = 0.0;
    }
    progress.tgs = Some(stage);
}

/// Takes in what a conversion of any format says as it goes.
fn follow(event: Event, update: Update) {
    match event {
        Event::Started(Started { details: StartDetails::Webm { plan }, limit, .. }) => {
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
        Event::Started(Started { details: StartDetails::Tgs { report }, .. }) => {
            update(Box::new(move |progress| {
                progress.report = Some(report);
                at_stage(progress, TgsStage::Drawing);
            }))
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
        Event::Decided { decision, .. } => {
            update(Box::new(move |progress| progress.decisions.push(decision)))
        }
        Event::TooLarge { bytes } => update(Box::new(move |progress| {
            progress.unchanged_bytes = Some(bytes as usize);
            // kept exactly as it is, it goes on to be compressed as it is
            if !progress.tgs_exact {
                at_stage(progress, TgsStage::Fitting);
            }
        })),
        // a try again is a bar of its own, the stages done
        Event::Reduced { step } => update(Box::new(move |progress| {
            progress.steps.push(step);
            if progress.retries.is_empty() {
                at_stage(progress, TgsStage::Fitting);
            }
        })),
        Event::Packing => update(Box::new(|progress| {
            if progress.retries.is_empty() {
                at_stage(progress, TgsStage::Compressing);
            }
        })),
        Event::Missed { bytes, aim } => update(Box::new(move |progress| {
            progress.first_steps.get_or_insert(progress.steps.len());
            progress.missed.push(bytes);
            progress.retries.push(aim);
            at_stage(progress, TgsStage::Compressing);
            progress.stage_done = 0.0;
        })),
        Event::Advanced { fraction, stage_fraction } => update(Box::new(move |progress| {
            (progress.tgs_done, progress.stage_done) = (fraction as f32, stage_fraction as f32);
        })),
        Event::Warning { message } => {
            update(Box::new(move |progress| progress.warnings.push(message.to_string())))
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(bytes: usize) -> Step {
        let reduction = tgradish_core::tgradish_tgs::reduce::Reduction::Despeckle;
        Step::new(reduction, bytes, 0.0)
    }

    /// `progress` after `event`.
    fn followed(progress: Progress, event: Event) -> Progress {
        let progress = std::cell::RefCell::new(progress);
        follow(event, &|change| change(&mut progress.borrow_mut()));
        progress.into_inner()
    }

    #[test]
    fn a_tgs_too_large_is_fitted_unless_kept_exactly() {
        let drawing = Progress { tgs: Some(TgsStage::Drawing), ..Progress::default() };
        let fitting = followed(drawing.clone(), Event::TooLarge { bytes: 200_000 });
        assert_eq!(fitting.tgs, Some(TgsStage::Fitting));
        // kept exactly, it goes from drawing to compressing as it is
        let exact = Progress { tgs_exact: true, ..drawing };
        let kept = followed(exact, Event::TooLarge { bytes: 200_000 });
        assert_eq!((kept.tgs, kept.unchanged_bytes), (Some(TgsStage::Drawing), Some(200_000)));
    }

    #[test]
    fn follows_how_far_a_tgs_got_in_all_and_in_each_stage() {
        let reading = Progress { tgs: Some(TgsStage::Reading), ..Progress::default() };
        assert_eq!(reading.overall(), Some(0.0));
        let advanced = |fraction, stage_fraction| Event::Advanced { fraction, stage_fraction };
        let drawing = Progress { tgs: Some(TgsStage::Drawing), ..reading };
        let drawn = followed(drawing, advanced(0.25, 0.5));
        assert_eq!((drawn.overall(), drawn.stage_done), (Some(0.25), 0.5));
        // a stage starts from nothing
        let packing = followed(drawn, Event::Packing);
        assert_eq!((packing.overall(), packing.stage_done), (Some(0.25), 0.0));
        let packed = followed(packing, advanced(0.75, 0.4));
        assert_eq!((packed.overall(), packed.stage_done), (Some(0.75), 0.4));
        // the result missed: a try again, its bar from the start, the
        // stages done however it fits and compresses again
        let missed = followed(packed, Event::Missed { bytes: Some(70_000), aim: 60_000 });
        assert_eq!(
            (missed.missed.clone(), missed.retries.clone()),
            (vec![Some(70_000)], vec![60_000])
        );
        assert_eq!((missed.tgs, missed.stage_done), (Some(TgsStage::Compressing), 0.0));
        let missed = followed(missed, advanced(0.8, 0.3));
        let missed = followed(missed, Event::Reduced { step: step(60_000) });
        let missed = followed(missed, Event::Packing);
        assert_eq!((missed.tgs, missed.stage_done), (Some(TgsStage::Compressing), 0.3));
        // the first try's steps, without those of the tries again
        assert_eq!((missed.steps.len(), missed.first_steps), (1, Some(0)));
        // and again, too large to be worth compressing
        let again = followed(missed, Event::Missed { bytes: None, aim: 57_000 });
        assert_eq!((again.missed.len(), again.retries.len(), again.stage_done), (2, 2, 0.0));
        let done = followed(again, advanced(1.0, 1.0));
        assert_eq!(done.overall(), Some(1.0));
    }

    #[test]
    fn says_roughly_how_far_a_conversion_got() {
        assert_eq!(Progress::default().overall(), None);
        // a WebM by encodes done of those it may take, and the one at work
        let plan =
            WebmPlan { width: 512, height: 512, fps: 30.0, length: 1.0, attempts: 4, limit: 1 };
        let params = Params { fps: 30.0, length: 1.0, rate: tgradish_core::webm::Rate::Crf(30) };
        let attempt = |bytes| Attempt { number: 1, params, bytes, fits: false, ssim: None };
        let mut progress = Progress {
            webm: Some(plan),
            attempts: vec![attempt(Some(10)), attempt(None)],
            fraction: Some(0.5),
            ..Progress::default()
        };
        assert_eq!(progress.overall(), Some(0.375));
        // more than were planned don't go past the end
        progress.attempts = vec![attempt(Some(10)); 6];
        assert_eq!(progress.overall(), Some(1.0));
    }

    fn texts(issues: &[telegram::Issue]) -> Vec<String> {
        problems(issues).into_iter().map(|problem| problem.text).collect()
    }

    #[test]
    fn words_problems_as_the_gui_does() {
        use telegram::Issue;
        // a sticker's size, and an emoji's, by what the issue says
        let sticker = Issue::WrongSize { width: 300, height: 200, side: 512, exact: false };
        let emoji = Issue::WrongSize { width: 300, height: 200, side: 100, exact: true };
        assert_eq!(
            Problem::of(&sticker),
            Problem {
                refused: true,
                text: fl!("issue-size", "sticker", width = 300, height = 200, side = 512)
            }
        );
        assert_eq!(
            Problem::of(&emoji).text,
            fl!("issue-size", width = 300, height = 200, side = 100)
        );
        // what only a .tgs is checked for, as core says it, and refused
        // only where it is an error
        let thirty = br#"{"fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[{"ty":4,"shapes":[]}]}"#;
        let (_, found) = tgradish_core::tgradish_tgs::check::check(thirty, None).unwrap();
        let frowned = Issue::Lottie(found[0].clone());
        // and a .tgs's of the kinds every format has in the GUI's words
        let narrow = br#"{"fr":60,"ip":0,"op":60,"w":512,"h":256,"layers":[{"ty":4,"shapes":[]}]}"#;
        let (stats, narrow) = tgradish_core::tgradish_tgs::check::check(narrow, None).unwrap();
        let narrow = telegram::check_lottie(narrow, &stats);
        assert_eq!(texts(&narrow), [fl!("issue-size", width = 512, height = 256, side = 512)]);
        assert_eq!(
            Problem::of(&frowned),
            Problem { refused: false, text: found[0].message.clone() }
        );
    }

    #[test]
    fn gives_sizes_in_bytes_where_kib_would_read_the_same() {
        assert_eq!(sizes(1024, 262_144), ("1.0 KiB".into(), "256.0 KiB".into()));
        assert_eq!(sizes(65_537, 65_536), ("65537 B".into(), "65536 B".into()));
    }

    #[test]
    fn words_failures_as_the_gui_does() {
        let invalid = Error::InvalidOption { option: "crf", reason: "must be at most 63".into() };
        assert_eq!(failure(&invalid), "Crf must be at most 63");
        let missing = failure(&Error::FfmpegNotFound(FfmpegMissing::NotBuiltIn));
        let details = FfmpegMissing::NotBuiltIn.to_string();
        assert_eq!(missing, fl!("ffmpeg-not-found", details = details));
        // the GUI's own words for what core names flags or keys in
        assert_eq!(
            failure(&Error::FfmpegNotFound(FfmpegMissing::NotOnPath)),
            fl!("ffmpeg-not-on-path")
        );
        assert!(!failure(&Error::FfmpegArgsNeedProgram).contains("ffmpeg-args"));
        let unreadable = Error::Unreadable {
            path: PathBuf::from("/some/where/clip.mp4"),
            message: "broken".into(),
        };
        assert_eq!(
            failure(&unreadable),
            fl!("unreadable-file", name = "clip.mp4", details = "broken")
        );
        // a file gone, or of the wrong kind, by its name in the GUI's words
        let gone = Error::NoSuchFile(PathBuf::from("/some/where/clip.mp4"));
        assert_eq!(failure(&gone), fl!("no-such-file", name = "clip.mp4"));
        let not = Error::NotSticker(PathBuf::from("/some/where/notes.txt"));
        assert_eq!(failure(&not), fl!("not-sticker", name = "notes.txt"));
        let not = Error::NotWebm(PathBuf::from("/some/where/clip.mp4"));
        assert_eq!(failure(&not), fl!("not-webm", name = "clip.mp4"));
        // what the GUI has no words of its own for, as tgradish-core says
        assert_eq!(failure(&Error::Cancelled), Error::Cancelled.to_string());
    }
}

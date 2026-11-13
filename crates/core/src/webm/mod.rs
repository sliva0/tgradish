//! WebM video stickers and emoji: converting into them, as VP9 fitted to
//! the size limit ([`fit`]), and what their container holds
//! ([`container`]).
//!
//! [`convert`] runs synchronously and reports [`Event`]s through a callback.
//! To run it in the background, call it from a thread and cancel it with a
//! [`CancelToken`]:
//!
//! ```no_run
//! # use tgradish_core::{backend::Backend, webm::{convert, Request}, CancelToken};
//! let backend = Backend::select(Default::default(), None)?;
//! let cancel = CancelToken::new();
//! let request = Request::new("pig.mp4".into());
//! let worker = std::thread::spawn({
//!     let cancel = cancel.clone();
//!     move || convert(&backend, &request, &cancel, &mut |event| println!("{event:?}"))
//! });
//! // cancel.cancel() stops it early
//! let outcome = worker.join().unwrap()?;
//! # Ok::<(), tgradish_core::Error>(())
//! ```

pub mod container;
mod events;
pub mod fit;
mod plan;

pub use events::{Event, Params, Rate};
pub use plan::{
    Plan, default_fit_range, encoded_length, estimate_bitrate, frame_count, plan, played_fps,
    read_from,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::CancelToken;
use crate::backend::{Backend, Pass};
use crate::error::{Error, FileAction, Result};
use crate::ffmpeg::Output;
use crate::options::Options;
use crate::picture::choose_auto_scaling;
use crate::telegram::{self, Issue, Target};
use container::Patch;
use fit::{Attempt, Encoder};

/// A conversion to run.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The input, or the first of a sequence's, which names the result.
    pub input: PathBuf,
    /// The frames of one animation in several files: image files, or
    /// folders of them, in name order.
    pub sequence: Option<Vec<PathBuf>>,
    /// Defaults to the input with a `.sticker.webm` or `.emoji.webm`
    /// extension.
    pub output: Option<PathBuf>,
    pub options: Options,
    /// Replace the output if it exists.
    pub overwrite: bool,
    /// Keep intermediate files, whatever comes of the conversion, and
    /// report where they are: in the outcome, or in a warning when it
    /// fails.
    pub keep_temp: bool,
}

impl Request {
    pub fn new(input: PathBuf) -> Self {
        Self {
            input,
            sequence: None,
            output: None,
            options: Options::default(),
            overwrite: false,
            keep_temp: false,
        }
    }
}

pub fn default_output(input: &Path, target: Target) -> PathBuf {
    crate::fsutil::beside(input, &format!("{}.webm", target.name()))
}

/// What converting made: all that [`Event::Finished`] says, whether or not
/// events were followed.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Outcome {
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub output: PathBuf,
    pub bytes: u64,
    /// The attempt that was kept.
    pub attempt: u32,
    pub params: Params,
    pub spoofed: bool,
    pub issues: Vec<Issue>,
    /// Directory with intermediate files, if they were kept.
    #[serde(serialize_with = "crate::protocol::serialize_optional_path")]
    pub temp_dir: Option<PathBuf>,
}

/// Runs fitting attempts through a [`Backend`]. Reuses first-pass logs:
/// libvpx-vp9 first-pass statistics do not depend on the target bitrate.
struct BackendEncoder<'a> {
    backend: &'a Backend,
    plan: &'a Plan,
    dir: &'a Path,
    cancel: &'a CancelToken,
    on_event: &'a mut dyn FnMut(Event),
    attempts: u32,
    pass_logs: HashMap<(u64, u64, bool), PathBuf>,
}

impl BackendEncoder<'_> {
    fn run_pass(
        &mut self,
        attempt: u32,
        (pass_number, passes): (u8, u8),
        params: &Params,
        pass: Pass,
        output: Option<&Path>,
    ) -> Result<()> {
        let length = params.length;
        let on_event = &mut *self.on_event;
        let progress = |fraction| Event::Progress { attempt, pass: pass_number, passes, fraction };
        on_event(progress(0.0));
        self.backend.encode(self.plan, params, pass, output, self.cancel, &mut |output| {
            match output {
                Output::Time { micros } => {
                    on_event(progress((micros as f64 / 1e6 / length).clamp(0.0, 1.0)));
                }
                Output::Line(line) => on_event(Event::Log { line }),
                // other -progress keys
                Output::Stdout(_) => {}
            }
        })?;
        on_event(progress(1.0));
        Ok(())
    }
}

impl Encoder for BackendEncoder<'_> {
    fn encode(&mut self, params: Params) -> Result<Attempt> {
        self.cancel.check()?;
        self.attempts += 1;
        let attempt = self.attempts;
        (self.on_event)(Event::AttemptStarted { attempt, params });
        let path = self.dir.join(format!("attempt-{attempt:02}.webm"));

        if params.rate == Rate::Lossless {
            self.run_pass(attempt, (1, 1), &params, Pass::Single, Some(&path))?;
        } else {
            // the first pass's statistics are of the frames encoded, so
            // they serve any attempt at the same frame rate and length;
            // constant quality and a bitrate keep logs of their own, as
            // libvpx's first pass differs between them
            let key = (
                params.fps.to_bits(),
                params.length.to_bits(),
                matches!(params.rate, Rate::Crf(_)),
            );
            let cached = self.pass_logs.get(&key).cloned();
            let passes = if cached.is_some() { 1 } else { 2 };
            let log = match cached {
                Some(log) => log,
                None => {
                    let log = self.dir.join(format!("pass-{}", self.pass_logs.len()));
                    self.run_pass(attempt, (1, passes), &params, Pass::First(&log), None)?;
                    self.pass_logs.insert(key, log.clone());
                    log
                }
            };
            self.run_pass(attempt, (passes, passes), &params, Pass::Second(&log), Some(&path))?;
        }

        let bytes = std::fs::metadata(&path).map_err(Error::io(FileAction::Read, &path))?.len();
        let fits = bytes <= self.plan.target.max_bytes();
        (self.on_event)(Event::AttemptFinished { attempt, params, bytes, fits });
        Ok(Attempt { number: attempt, params, bytes, path })
    }

    fn score(&mut self, attempt: &Attempt) -> Result<f64> {
        let on_event = &mut *self.on_event;
        let fps = attempt.params.fps;
        let ssim = self.backend.ssim(self.plan, &attempt.path, fps, self.cancel, &mut |line| {
            on_event(Event::Log { line })
        })?;
        on_event(Event::Scored { attempt: attempt.number, ssim });
        Ok(ssim)
    }
}

/// Converts a video into a Telegram sticker or emoji.
pub fn convert(
    backend: &Backend,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    // pixel art, drawn for ffmpeg
    let sequence = request.sequence.as_deref();
    let mut readable = crate::drawn::readable(&request.input, sequence, &request.options, cancel)?;
    // ffmpeg reads them from the file: not kept while it does
    drop(readable.take_drawn());
    let source = readable.probe(backend, cancel)?;
    let (mut plan, warnings) = plan(request, source)?;
    plan.input = readable.path().to_path_buf();
    choose_auto_scaling(backend, &mut plan, &request.options, cancel)?;
    if !plan.ffmpeg_args.is_empty() && !backend.supports_ffmpeg_args() {
        return Err(Error::FfmpegArgsNeedProgram);
    }
    // the input as given, not as drawn for ffmpeg
    let started = Plan { input: request.input.clone(), ..plan.clone() };
    on_event(Event::Started { plan: Box::new(started) });
    // after it: of several inputs' events, a warning is about the one that
    // started last
    for message in warnings {
        on_event(Event::Warning { message });
    }

    // kept from the start when asked, whatever comes of the encodes
    let temp = tempfile::Builder::new()
        .prefix("tgradish-")
        .disable_cleanup(request.keep_temp)
        .tempdir()
        .map_err(Error::io(FileAction::Create, std::env::temp_dir()))?;
    let made = encode_in(temp.path(), backend, request, plan, cancel, on_event);
    if made.is_err() && request.keep_temp {
        // the outcome says where they are, and there is none
        let message = format!("{KEPT_IN}{}", temp.path().display());
        on_event(Event::Warning { message });
    }
    made
}

/// Applies `changes` to the encoded `bytes`, or as many of them as there is
/// room for: what extra ffmpeg arguments make can leave too little beside
/// the header for tgradish's name, and with no duration there, for a
/// spoofed one. Those are left out, each with a warning, rather than the
/// encode lost. Returns the changes made.
fn patch_what_fits(
    bytes: &mut [u8],
    mut changes: Patch,
    on_event: &mut dyn FnMut(Event),
) -> Result<Patch> {
    loop {
        // on an error, `bytes` are as they were
        let err = match container::patch(bytes, &changes) {
            Ok(_) => return Ok(changes),
            Err(err) => err,
        };
        let left_out = if changes.muxing_app.is_some() || changes.signature.is_some() {
            (changes.muxing_app, changes.signature) = (None, None);
            "tgradish's name"
        } else if changes.duration.is_some() {
            changes.duration = None;
            "the spoofed duration"
        } else {
            return Err(err.into());
        };
        let message = format!("written without {left_out} in its metadata: {err}");
        on_event(Event::Warning { message });
    }
}

/// What a warning that a failed conversion's intermediate files are kept
/// starts with; their directory follows.
const KEPT_IN: &str = "intermediate files are kept in ";

/// Fits `plan` by encodes into `dir`, and writes the result.
fn encode_in(
    dir: &Path,
    backend: &Backend,
    request: &Request,
    plan: Plan,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let mut encoder = BackendEncoder {
        backend,
        plan: &plan,
        dir,
        cancel,
        on_event: &mut *on_event,
        attempts: 0,
        pass_logs: HashMap::new(),
    };
    let best = fit::run(&plan, &mut encoder)?;
    cancel.check()?;
    let spoofs = plan.spoofs(encoded_length(best.params.length, best.params.fps));

    let changes = Patch {
        duration: spoofs.then_some(plan.fake_duration),
        muxing_app: plan.watermark.then(|| crate::TOOL_ID.to_string()),
        signature: plan.watermark.then(crate::signature),
        // the hidden mark, which no option leaves out
        track_uid: Some(crate::mark::Mark::current().to_u64()),
        ..Default::default()
    };
    let mut bytes = std::fs::read(&best.path).map_err(Error::io(FileAction::Read, &best.path))?;
    let spoofed = patch_what_fits(&mut bytes, changes, on_event)?.duration.is_some();
    // checked again here: the output may have appeared while encoding
    crate::fsutil::write_output(&plan.output, &bytes, request.overwrite)?;

    let info = container::inspect(&bytes)?;
    let issues = telegram::check(&info, plan.target);
    let temp_dir = request.keep_temp.then(|| dir.to_path_buf());
    on_event(Event::Finished {
        output: plan.output.clone(),
        bytes: info.file_size,
        attempt: best.number,
        params: best.params,
        spoofed,
        issues: issues.clone(),
        temp_dir: temp_dir.clone(),
    });
    Ok(Outcome {
        output: plan.output,
        bytes: info.file_size,
        attempt: best.number,
        params: best.params,
        spoofed,
        issues,
        temp_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WebM header with no room beside Info, as the arguments it is
    /// given can make ffmpeg write, with a Duration or not.
    fn cramped_webm(duration: bool) -> Vec<u8> {
        use crate::webm::container::ebml::build::*;
        use crate::webm::container::ebml::ids;
        let mut info = vec![uint(ids::TIMESTAMP_SCALE, 1_000_000), el(ids::MUXING_APP, b"Lavf")];
        if duration {
            info.push(f64_el(ids::DURATION, 4000.0));
        }
        let track = el(
            ids::TRACK_ENTRY,
            &concat(&[
                uint(ids::TRACK_NUMBER, 1),
                uint(ids::TRACK_TYPE, 1),
                el(ids::TRACK_UID, &0x8000_0000_0000_0001u64.to_be_bytes()),
            ]),
        );
        let segment = concat(&[el(ids::INFO, &concat(&info)), el(ids::TRACKS, &track)]);
        concat(&[el(ids::EBML, &el(ids::DOC_TYPE, b"webm")), el(ids::SEGMENT, &segment)])
    }

    #[test]
    fn patches_what_there_is_room_for() {
        let mark = crate::mark::Mark::current().to_u64();
        let changes = Patch {
            duration: Some(0.5),
            muxing_app: Some(crate::TOOL_ID.into()),
            signature: Some(crate::signature()),
            track_uid: Some(mark),
            ..Default::default()
        };
        for duration in [true, false] {
            let mut bytes = cramped_webm(duration);
            let mut warnings = Vec::new();
            let made = patch_what_fits(&mut bytes, changes.clone(), &mut |event| {
                if let Event::Warning { message } = event {
                    warnings.push(message);
                }
            });
            let made = made.unwrap();
            let info = container::inspect(&bytes).unwrap();
            // the mark always fits; the name never does here
            assert_eq!(info.video.unwrap().uid, Some(mark));
            assert_eq!((info.muxing_app.as_deref(), made.muxing_app), (Some("Lavf"), None));
            // a duration there is written over, one that isn't needs room
            assert_eq!(made.duration.is_some(), duration);
            assert_eq!(info.header_duration, duration.then_some(0.5));
            assert_eq!(warnings.len(), if duration { 1 } else { 2 }, "{warnings:?}");
        }
        // what isn't WebM still fails
        let made = patch_what_fits(&mut [0; 16], changes, &mut |_| {});
        assert!(matches!(made, Err(Error::Webm(_))));
    }
}

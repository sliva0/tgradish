//! Shared conversion progress events ([`Event`]) and final results
//! ([`Outcome`]) across all three output formats.

use std::path::PathBuf;

use serde::Serialize;
use tgradish_tgs::normalise::Report;
use tgradish_tgs::sticker::Step;

use crate::presets::Format;
use crate::preview::Preview;
use crate::telegram::{Issue, Target};
use crate::webm::{Decision, Params, Plan};
use crate::webp::Encoding;
use crate::wire::Failure;

/// Progress event emitted during conversion and printed as NDJSON by
/// `convert --json` and `watch --json`.
///
/// Every conversion starts with [`Event::Started`] and ends with
/// [`Event::Finished`]. Between them, WebM emits encoding and fitting events,
/// `.tgs` emits fitting and progress events, and WebP only warnings (any
/// format can emit [`Event::Warning`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Event {
    /// First event of every conversion, carrying the WebM plan or `.tgs`
    /// normalisation report.
    Started(Started),
    /// WebM: an encode attempt has started.
    AttemptStarted {
        /// 1-based attempt number.
        attempt: u32,
        /// Encoder parameters for this attempt.
        params: Params,
    },
    /// WebM: progress (0.0 to 1.0) within the current encoding pass.
    Progress {
        /// 1-based attempt number.
        attempt: u32,
        /// 1-based pass number.
        pass: u8,
        /// Total passes (2, or 1 when reusing an earlier first pass).
        passes: u8,
        /// Fraction of the pass completed (0.0 to 1.0).
        fraction: f64,
        /// True when an earlier attempt already fits so fitting can settle
        /// early (see [`CancelToken::settle`](crate::CancelToken::settle)).
        can_settle: bool,
    },
    /// WebM: an encode attempt has finished.
    AttemptFinished {
        /// 1-based attempt number.
        attempt: u32,
        /// Encoder parameters used.
        params: Params,
        /// Output size in bytes.
        bytes: u64,
        /// Whether `bytes` is within the target size limit.
        fits: bool,
        /// True when this or an earlier attempt fits, so fitting can settle
        /// early.
        can_settle: bool,
    },
    /// WebM: how much an attempt looks like the input, by its SSIM score
    /// (0.0 to 1.0) against the source.
    Scored {
        /// 1-based attempt number.
        attempt: u32,
        /// SSIM score.
        ssim: f64,
    },
    /// WebM: size-fitting decided to stop or adjust parameters.
    Decided {
        /// Structured decision details.
        decision: Decision,
        /// Human-readable explanation.
        message: String,
    },
    /// WebM: a single line of ffmpeg stderr output.
    Log {
        /// Output line text.
        line: String,
    },
    /// `.tgs`: estimated unreduced size exceeds the limit; size-fitting begins
    /// unless `exact` is set.
    TooLarge {
        /// Estimated compressed size in bytes.
        bytes: u64,
    },
    /// `.tgs`: size-fitting applied a reduction step.
    Reduced {
        /// Applied step and its result.
        step: Step,
    },
    /// `.tgs`: compressing the final Lottie JSON.
    Packing,
    /// `.tgs`: output still missed the limit because fitting's estimates were
    /// off; fitting retries aiming lower, then encodes and compresses the
    /// result as a stage of its own, until it fits or nothing is left to try.
    Missed {
        /// Actual compressed size in bytes, or `None` if skipped before
        /// compression (far too large or over another Telegram limit).
        bytes: Option<u64>,
        /// New target estimate in bytes.
        aim: u64,
    },
    /// `.tgs`: estimated progress (0.0 to 1.0), emitted after reading the input
    /// and never less than before. Each stage gets a share of the total roughly
    /// proportional to the time it takes.
    Advanced {
        /// Overall progress fraction (0.0 to 1.0).
        fraction: f64,
        /// Progress fraction (0.0 to 1.0) within the current stage: drawing
        /// since `started`, fitting after `too-large` (unless `exact` is on),
        /// `packing`, or the whole retry stage (fitting, encoding and
        /// compressing) after each `missed`.
        stage_fraction: f64,
    },
    /// Non-fatal warning (adjusted or ignored options, pixel-art notes, or
    /// unreadable watch directories).
    Warning {
        /// What it warns of, serialised as its English message.
        message: Warning,
    },
    /// Output written; carries the same [`Outcome`] returned by `convert`.
    Finished(Outcome),
    /// Conversion or command failed. Emitted only by the CLI; library calls
    /// return `Err` instead.
    Error(Failure),
}

/// What an [`Event::Warning`] warns of, with what it measured, for
/// front-ends to word it their own way. `Display` (and serialising, as
/// a string) words it in English.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Warning {
    /// A video is read at `fps` frames a second, fewer than it has, as more
    /// of its `width` × `height` frames would exceed the memory limit.
    FewerFrames {
        /// The frame rate read at.
        fps: f64,
        /// Frame width in pixels.
        width: u32,
        /// Frame height in pixels.
        height: u32,
    },
    /// What reading pixel art found worth saying.
    Art(tgradish_tgs::normalise::Warning),
    /// Fitting the length can only shorten: its range is capped at
    /// `length` seconds.
    LengthRangeCapped {
        /// The length the range is capped at, in seconds.
        length: f64,
    },
    /// The length asked for is longer than the `available` seconds of the
    /// input after its start, which is all that is used.
    InputShorter {
        /// Seconds of input after the start.
        available: f64,
    },
    /// Trimmed to `max` seconds as spoofing is off, as Telegram rejects
    /// longer videos.
    Trimmed {
        /// The length trimmed to, in seconds.
        max: f64,
    },
    /// The option `option` has no effect, for the reason `why`, a clause
    /// naming options by key, as [`crate::webm::set_aside`] gives it.
    Ignored {
        /// The option's key.
        option: &'static str,
        /// Why it has no effect.
        why: String,
    },
    /// A failed conversion's intermediate files are kept in `dir`.
    KeptIn {
        /// Their directory.
        dir: PathBuf,
    },
    /// The output is written without `what` in its metadata, as there is no
    /// room for it beside extra ffmpeg arguments' output, as `error` says.
    LeftOut {
        /// What is left out.
        what: LeftOut,
        /// Why it didn't fit, in English.
        error: String,
    },
    /// Anything else, in English.
    Other(String),
}

/// What [`Warning::LeftOut`] leaves out of a WebM's metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LeftOut {
    /// tgradish's name, as the muxing app and signature.
    Name,
    /// The spoofed duration.
    SpoofedDuration,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::FewerFrames { fps, width, height } => write!(
                f,
                "read at {fps:.1} fps because more {width}×{height} frames would exceed the \
                 memory limit"
            ),
            Warning::Art(warning) => write!(f, "{warning}"),
            Warning::LengthRangeCapped { length } => {
                write!(f, "length range capped at {length:.2} s")
            }
            Warning::InputShorter { available } => {
                write!(f, "input is only {available:.2} s long after start")
            }
            Warning::Trimmed { max } => write!(
                f,
                "trimming to {max} s because spoofing is off (Telegram rejects longer videos)"
            ),
            Warning::Ignored { option, why } => write!(f, "{option} is ignored because {why}"),
            Warning::KeptIn { dir } => {
                write!(f, "intermediate files kept in {}", dir.display())
            }
            Warning::LeftOut { what, error } => {
                let what = match what {
                    LeftOut::Name => "tgradish's name",
                    LeftOut::SpoofedDuration => "the spoofed duration",
                };
                write!(f, "written without {what} in metadata: {error}")
            }
            Warning::Other(message) => f.write_str(message),
        }
    }
}

impl Serialize for Warning {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl From<String> for Warning {
    fn from(message: String) -> Warning {
        Warning::Other(message)
    }
}

impl From<&str> for Warning {
    fn from(message: &str) -> Warning {
        Warning::Other(message.to_owned())
    }
}

/// Payload for [`Event::Started`]: input, output, format, target, size limit,
/// and format-specific start details.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Started {
    /// Input path (or first image/directory of a sequence).
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub input: PathBuf,
    /// Destination output path.
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub output: PathBuf,
    /// Output format.
    pub format: Format,
    /// Telegram target (sticker or emoji).
    pub target: Target,
    /// Telegram file size limit in bytes.
    pub limit: u64,
    /// Format-specific start metadata.
    #[serde(flatten)]
    pub details: StartDetails,
}

/// Format-specific fields flattened into [`Started`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged, rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum StartDetails {
    /// WebM conversion plan before encoding starts.
    Webm {
        /// Computed WebM plan.
        plan: Box<Plan>,
    },
    /// `.tgs` normalisation report after reading the input.
    Tgs {
        /// Normalisation report.
        report: Box<Report>,
    },
    /// WebP has no extra start metadata.
    Webp {},
}

/// Final result of a successful conversion across any format.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Outcome {
    /// Input path (or first image/directory of a sequence).
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub input: PathBuf,
    /// Path where the output was written.
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub output: PathBuf,
    /// Output format.
    pub format: Format,
    /// Telegram target (sticker or emoji).
    pub target: Target,
    /// Output file size in bytes.
    pub bytes: u64,
    /// Telegram file size limit in bytes.
    pub limit: u64,
    /// True if fitting compromised visible output to meet the size limit:
    /// lowered frame rate or shortened duration for WebM; reductions, forced
    /// pixel scale or trimmed duration for `.tgs`; lossy fallback for WebP.
    pub changed: bool,
    /// Remaining Telegram validation issues; any error-severity issue means
    /// Telegram will reject the file (see [`crate::telegram::refuses`]).
    pub issues: Vec<Issue>,
    /// Format-specific result metadata.
    #[serde(flatten)]
    pub details: Details,
}

/// Format-specific fields flattened into [`Outcome`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged, rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Details {
    /// WebM result metadata.
    Webm {
        /// 1-based number of the chosen attempt.
        attempt: u32,
        /// Encoder parameters of the chosen attempt.
        params: Params,
        /// Whether the container header duration was spoofed.
        spoofed: bool,
        /// Whether size-fitting stopped early via
        /// [`CancelToken::settle`](crate::CancelToken::settle).
        settled: bool,
        /// Directory holding kept intermediate files, if `keep_temp` was set.
        #[serde(serialize_with = "crate::wire::serialize_optional_path")]
        temp_dir: Option<PathBuf>,
    },
    /// `.tgs` result metadata.
    Tgs {
        /// Uncompressed Lottie JSON size in bytes.
        json_bytes: u64,
        /// Reductions applied during fitting, in order.
        steps: Vec<Step>,
        /// Number of Lottie layers.
        layers: u64,
        /// Total rectangle shapes across all layers.
        rectangles: u64,
        /// Retained drawing data for [`Outcome::preview`]; not serialised.
        #[serde(skip)]
        drawing: crate::tgs::Drawing,
    },
    /// WebP result metadata.
    Webp {
        /// Output width in pixels.
        width: u32,
        /// Output height in pixels.
        height: u32,
        /// Chosen encoding mode and quality.
        encoding: Encoding,
    },
}

impl Outcome {
    /// Renders an in-memory preview for `.tgs` results (matching
    /// [`crate::tgs::preview_file`]). Returns `None` for WebM and WebP, which
    /// are decoded from disk via
    /// [`Backend::preview`](crate::backend::Backend::preview) and
    /// [`crate::webp::preview`].
    pub fn preview(&self) -> Option<Preview> {
        match &self.details {
            Details::Tgs { drawing, .. } => Some(drawing.draw()),
            _ => None,
        }
    }
}

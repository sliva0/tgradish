//! What a conversion of any format reports and makes: its [`Event`]s,
//! the first [`Started`] and the last [`Finished`](Event::Finished) with
//! its [`Outcome`], each the same of every format but for its details.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;
use tgradish_tgs::normalise::Report;
use tgradish_tgs::sticker::Step;

use crate::presets::Format;
use crate::preview::Preview;
use crate::protocol::Failure;
use crate::telegram::{Issue, Target};
use crate::webm::{Decision, Params, Plan};
use crate::webp::Encoding;

/// What a conversion of any format reports as it goes, for front-ends to
/// follow, which `convert --json` and `watch --json` print one a line:
/// [`Event::Started`] first, then what the format says as it works, and
/// [`Event::Finished`] once the result is written. WebM's fitting says
/// what each encode does, and `.tgs`'s what fitting takes off; a WebP is
/// made in one go, and says nothing between.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Event {
    /// The conversion is about to make its result: the first event of
    /// every conversion, with a WebM's plan or what reading a `.tgs`'s art
    /// found.
    Started(Started),
    /// WebM: an encode starts.
    AttemptStarted {
        /// Which, from 1.
        attempt: u32,
        /// With what settings.
        params: Params,
    },
    /// WebM: progress of the current pass of an attempt, from 0 to 1.
    Progress {
        /// Which attempt.
        attempt: u32,
        /// Which of its passes, from 1.
        pass: u8,
        /// How many it has: 2, or 1 where an earlier attempt's first pass
        /// serves it.
        passes: u8,
        /// How much of the pass is done.
        fraction: f64,
        /// Whether an encode fits already, so that fitting can settle on
        /// the best so far, see
        /// [`CancelToken::settle`](crate::CancelToken::settle).
        can_settle: bool,
    },
    /// WebM: an encode is done.
    AttemptFinished {
        /// Which.
        attempt: u32,
        /// With what settings.
        params: Params,
        /// Its size in bytes.
        bytes: u64,
        /// Whether that is within the target's limit.
        fits: bool,
        /// Whether this or an earlier encode fits, so that fitting can
        /// settle on the best so far.
        can_settle: bool,
    },
    /// WebM: SSIM of an attempt compared to the source, from 0 to 1.
    Scored {
        /// Which attempt.
        attempt: u32,
        /// Its SSIM.
        ssim: f64,
    },
    /// WebM: fitting decided to stop, or to try a change or not: why, for
    /// front-ends to say.
    Decided {
        /// What, and why.
        decision: Decision,
        /// The same in English.
        message: String,
    },
    /// WebM: a line of ffmpeg output.
    Log {
        /// The line.
        line: String,
    },
    /// `.tgs`: unchanged, the sticker would come to about this many bytes,
    /// more than fits; fitting starts, unless `exact` keeps the art as it
    /// is.
    TooLarge {
        /// The estimate, in bytes.
        bytes: u64,
    },
    /// `.tgs`: fitting applied a reduction.
    Reduced {
        /// Which, and what it came to.
        step: Step,
    },
    /// `.tgs`: the result is being compressed.
    Packing,
    /// Something worth saying that doesn't stop the conversion: as options
    /// that were adjusted or ignored, or what reading the art found; or
    /// about more than one, as `watch` says of folders it can't read.
    Warning {
        /// What, in English.
        message: String,
    },
    /// The result is written: what was made, as converting returns it.
    Finished(Outcome),
    /// A conversion, or the whole command, failed. Only printed by the
    /// command line; the library returns errors instead.
    Error(Failure),
}

/// A conversion of any format is about to make its result, which its
/// `started` event says, the first of each conversion: what it makes of
/// which input, where, and the limit it must fit, beside what only its
/// format says then.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Started {
    /// The input, the first picture of a sequence.
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub input: PathBuf,
    /// Where the result goes.
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub output: PathBuf,
    /// What it makes.
    pub format: Format,
    /// What it makes it as.
    pub target: Target,
    /// The most Telegram takes of it, in bytes.
    pub limit: u64,
    /// What only its format says as it starts, beside the rest.
    #[serde(flatten)]
    pub details: StartDetails,
}

/// What only one format's [`Started`] says, serialised beside the rest of
/// it, its `format` saying which.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged, rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum StartDetails {
    /// A WebM's: the conversion is planned and about to start encoding.
    Webm {
        /// What the conversion will do.
        plan: Box<Plan>,
    },
    /// A `.tgs`'s: the input is read.
    Tgs {
        /// What normalising found and did.
        report: Box<Report>,
    },
    /// A WebP's: the input was read, and the picture is about to be made;
    /// nothing more is said of it.
    Webp {},
}

/// What converting made, of any format, which its `finished` event says
/// whether or not events were followed: the sticker or emoji written, how
/// it measures up to Telegram's limit for it, and what Telegram would
/// still have a problem with, beside what only its format has.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Outcome {
    /// The input, the first picture of a sequence, as every event about
    /// one names it.
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub input: PathBuf,
    /// Where the result was written.
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub output: PathBuf,
    /// What it is.
    pub format: Format,
    /// What it was made as.
    pub target: Target,
    /// Its size in bytes.
    pub bytes: u64,
    /// The most Telegram takes of it, in bytes.
    pub limit: u64,
    /// Whether anything visible was given up to fit the limit: for WebM, a
    /// lower frame rate or a shorter length than planned, as fitting by
    /// them makes; for `.tgs`, reductions, a forced pixel scale or the end
    /// of a long input; for WebP, saving it lossy where lossless didn't
    /// fit.
    pub changed: bool,
    /// What Telegram would still have a problem with; it refuses the
    /// result for any error, as [`crate::telegram::refuses`] says.
    pub issues: Vec<Issue>,
    /// What only its format has, beside the rest.
    #[serde(flatten)]
    pub details: Details,
}

/// What only one format's [`Outcome`] has, serialised beside the rest of
/// it, its `format` saying which.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged, rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Details {
    /// A WebM's: the encode fitting kept, and how.
    Webm {
        /// The attempt that was kept.
        attempt: u32,
        /// Its settings.
        params: Params,
        /// Whether its header's duration was spoofed.
        spoofed: bool,
        /// Whether fitting was asked to stop early and kept the best
        /// encode that fit so far, see
        /// [`CancelToken::settle`](crate::CancelToken::settle).
        settled: bool,
        /// Where the intermediate files are, when they were kept.
        #[serde(serialize_with = "crate::protocol::serialize_optional_path")]
        temp_dir: Option<PathBuf>,
    },
    /// A `.tgs`'s: what its Lottie holds, and what fitting did.
    Tgs {
        /// Size of the Lottie JSON inside.
        json_bytes: u64,
        /// Reductions fitting applied, in order.
        steps: Vec<Step>,
        /// Lottie layers in it.
        layers: u64,
        /// Rectangles drawn in all of them.
        rectangles: u64,
        /// What the sticker shows, drawn for previews when asked, as
        /// [`Outcome::preview`] draws it; not serialised.
        #[serde(skip)]
        drawing: crate::tgs::Drawing,
    },
    /// A WebP's: the picture, and how it was encoded.
    Webp {
        /// Its width in pixels.
        width: u32,
        /// Its height in pixels.
        height: u32,
        /// How it was encoded to fit.
        encoding: Encoding,
    },
}

impl Outcome {
    /// What a `.tgs` shows, drawn now from what it was made of, as
    /// reading it back with [`crate::tgs::preview_file`] draws it; `None`
    /// for the other formats, which are read from their files for
    /// previews, as [`crate::webp::preview`] and
    /// [`Backend::preview`](crate::backend::Backend::preview) read them.
    pub fn preview(&self) -> Option<Preview> {
        match &self.details {
            Details::Tgs { drawing, .. } => Some(drawing.draw()),
            _ => None,
        }
    }
}

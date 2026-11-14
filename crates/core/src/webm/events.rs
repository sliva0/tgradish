//! What a WebM conversion reports as it goes, for front-ends to follow.
//! `.tgs` and WebP conversions report their own, [`crate::tgs::Event`]
//! and [`crate::webp::Event`].

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;

use super::Plan;
use crate::protocol::Failure;
use crate::telegram::Issue;

/// Rate control used by one encode. Serialised tagged by `mode`, as
/// `{"mode":"bitrate","kbps":500}`, `{"mode":"crf","crf":32}` or
/// `{"mode":"lossless"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(into = "RateMode")]
#[non_exhaustive]
pub enum Rate {
    /// Two-pass average bitrate, in kbit/s.
    Bitrate(f64),
    /// Constant quality, from 0 (best) to 63.
    Crf(u8),
    /// Lossless VP9.
    Lossless,
}

/// [`Rate`] as it is serialised: an internally tagged enum needs fields
/// to put its tag beside.
#[derive(Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "kebab-case")]
enum RateMode {
    /// Two-pass average bitrate.
    Bitrate {
        /// In kbit/s.
        kbps: f64,
    },
    /// Constant quality, from 0 (best) to 63.
    Crf {
        crf: u8,
    },
    Lossless,
}

impl From<Rate> for RateMode {
    fn from(rate: Rate) -> RateMode {
        match rate {
            Rate::Bitrate(kbps) => RateMode::Bitrate { kbps },
            Rate::Crf(crf) => RateMode::Crf { crf },
            Rate::Lossless => RateMode::Lossless,
        }
    }
}

/// Settings of one encode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Params {
    /// Frames a second.
    pub fps: f64,
    /// Length in seconds.
    pub length: f64,
    /// How the encoder spends bytes.
    pub rate: Rate,
}

/// Progress of a WebM conversion, printed by `convert --json` one per
/// line.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Event {
    /// The conversion is planned and about to start encoding: the first
    /// event of every conversion.
    Started {
        /// What the conversion will do.
        plan: Box<Plan>,
    },
    /// An encode starts.
    AttemptStarted {
        /// Which, from 1.
        attempt: u32,
        /// With what settings.
        params: Params,
    },
    /// Progress of the current pass of an attempt, from 0 to 1.
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
    },
    /// An encode is done.
    AttemptFinished {
        /// Which.
        attempt: u32,
        /// With what settings.
        params: Params,
        /// Its size in bytes.
        bytes: u64,
        /// Whether that is within the target's limit.
        fits: bool,
    },
    /// SSIM of an attempt compared to the source, from 0 to 1.
    Scored {
        /// Which attempt.
        attempt: u32,
        /// Its SSIM.
        ssim: f64,
    },
    /// Something worth saying that doesn't stop the conversion.
    Warning {
        /// What, in English.
        message: String,
    },
    /// A line of ffmpeg output.
    Log {
        /// The line.
        line: String,
    },
    /// The sticker is written.
    Finished {
        /// The input, the first picture of a sequence, as every format's
        /// result names it.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        input: PathBuf,
        /// Where.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        output: PathBuf,
        /// Its size in bytes.
        bytes: u64,
        /// The attempt that was kept.
        attempt: u32,
        /// Its settings.
        params: Params,
        /// Whether its header's duration was spoofed.
        spoofed: bool,
        /// Problems Telegram would still have with the result.
        issues: Vec<Issue>,
        /// Where the intermediate files are, when they were kept.
        #[serde(serialize_with = "crate::protocol::serialize_optional_path")]
        temp_dir: Option<PathBuf>,
    },
    /// A conversion, or the whole command, failed. Only printed by the
    /// CLI; the library returns errors instead.
    Error(Failure),
}

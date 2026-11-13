//! What a WebM conversion reports as it goes, for front-ends to follow.
//! `.tgs` and WebP conversions report their own, [`crate::tgs::TgsEvent`]
//! and [`crate::still::StillEvent`].

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;

use crate::convert::Plan;
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
    /// Constant quality.
    Crf(u8),
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
    pub fps: f64,
    /// Length in seconds.
    pub length: f64,
    pub rate: Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Event {
    /// The conversion is planned and about to start encoding: the first
    /// event of every conversion.
    Started {
        plan: Box<Plan>,
    },
    AttemptStarted {
        attempt: u32,
        params: Params,
    },
    /// Progress of the current pass of an attempt, from 0 to 1.
    Progress {
        attempt: u32,
        pass: u8,
        passes: u8,
        fraction: f64,
    },
    AttemptFinished {
        attempt: u32,
        params: Params,
        bytes: u64,
        fits: bool,
    },
    /// SSIM of an attempt compared to the source, from 0 to 1.
    Scored {
        attempt: u32,
        ssim: f64,
    },
    Warning {
        message: String,
    },
    /// A line of ffmpeg output.
    Log {
        line: String,
    },
    Finished {
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        output: PathBuf,
        bytes: u64,
        /// The attempt that was kept.
        attempt: u32,
        params: Params,
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

/// Why a conversion, or the whole command, failed: the error event of
/// every stream `convert --json`, `watch --json` and `inspect --json`
/// print, the same in each. Only printed by the CLI; the library returns
/// errors instead.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Failure {
    pub message: String,
    /// The input that failed, `None` when the whole command did.
    #[serde(serialize_with = "crate::protocol::serialize_optional_path")]
    pub input: Option<PathBuf>,
}

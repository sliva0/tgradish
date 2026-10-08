//! Progress reporting for conversions.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;

use crate::convert::Plan;
use crate::telegram::Issue;

/// Rate control used by one encode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rate {
    /// Two-pass average bitrate, in kbit/s.
    Bitrate(f64),
    /// Constant quality.
    Crf(u8),
    Lossless,
}

/// Settings of one encode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct Params {
    pub fps: f64,
    /// Length in seconds.
    pub length: f64,
    pub rate: Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// The conversion is planned and about to start encoding.
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
        output: PathBuf,
        bytes: u64,
        /// The attempt that was kept.
        attempt: u32,
        params: Params,
        spoofed: bool,
        /// Problems Telegram would still have with the result.
        issues: Vec<Issue>,
    },
}

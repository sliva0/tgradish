//! WebM encoding parameters and size-fitting decision types carried by
//! [`crate::Event`].

use schemars::JsonSchema;
use serde::Serialize;

/// Rate control mode for a single encode attempt. Serialises with a `mode` tag:
/// `{"mode":"bitrate","kbps":500}`, `{"mode":"crf","crf":32}` or
/// `{"mode":"lossless"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(into = "RateMode")]
#[non_exhaustive]
pub enum Rate {
    /// Two-pass target bitrate in kbit/s.
    Bitrate(f64),
    /// Constant quality from 0 (best) to 63 (worst).
    Crf(u8),
    /// Lossless VP9.
    Lossless,
}

/// Serialisation helper for [`Rate`]: an internally tagged serde enum cannot
/// put its tag beside a bare number.
#[derive(Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "kebab-case")]
enum RateMode {
    /// Two-pass target bitrate.
    Bitrate {
        /// Bitrate in kbit/s.
        kbps: f64,
    },
    /// Constant quality from 0 (best) to 63 (worst).
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

/// Parameters used for a single WebM encode attempt.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Params {
    /// Frame rate in frames per second.
    pub fps: f64,
    /// Duration in seconds.
    pub length: f64,
    /// Rate control mode.
    pub rate: Rate,
}

/// Reason why WebM size-fitting stopped or changed strategy. Serialises with a
/// `reason` tag; limit shares are fractions of the limit (above 1.0 when over
/// it) and SSIM is from 0 to 1 (e.g.
/// `{"reason":"in-band","attempt":2,"share":0.99,"least":0.96}`). A change not
/// tried emits nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Decision {
    /// Stopped because `attempt` fits and uses at least `least` of the size
    /// limit.
    InBand {
        /// Attempt number.
        attempt: u32,
        /// Fraction of the size limit used.
        share: f64,
        /// Minimum share that ends fitting.
        least: f64,
    },
    /// Stopped because `attempt` already looks as like the input as encodes
    /// get (SSIM >= 0.995), so more bits would only make it larger.
    AlikeEnough {
        /// Attempt number.
        attempt: u32,
        /// SSIM score (0.0 to 1.0).
        ssim: f64,
    },
    /// Stopped because `attempt` barely improved SSIM over `than` (made with
    /// fewer bits), so spending more bits is not worth another encode.
    AlikeUnchanged {
        /// Attempt with more bits.
        attempt: u32,
        /// Earlier attempt with fewer bits.
        than: u32,
        /// SSIM score of `attempt`.
        ssim: f64,
    },
    /// Stopped because the next predicted encode would gain too little size
    /// over `attempt`.
    LittleToGain {
        /// Best fitting attempt so far.
        attempt: u32,
        /// Current share of the size limit.
        share: f64,
        /// Predicted share of the next attempt.
        predicted: f64,
    },
    /// Stopped because even the minimum value in the search range exceeded the
    /// limit.
    TooLargeAtLeast {
        /// Attempt at the minimum search value.
        attempt: u32,
        /// Fraction of the size limit used (> 1.0).
        share: f64,
    },
    /// Stopped because no untried values remain in the search range.
    NothingLeft,
    /// Stopped after using all allowed attempts.
    OutOfAttempts {
        /// Configured attempt cap.
        attempts: u32,
    },
    /// Lowering frame rate to `fps` because nothing fit at `from`.
    LowersFrameRateToFit {
        /// Previous frame rate.
        from: f64,
        /// Next frame rate to try.
        fps: f64,
    },
    /// Lowering frame rate to `fps` because the best encode at `from` had low
    /// SSIM.
    LowersFrameRateForLooks {
        /// Previous frame rate.
        from: f64,
        /// Next frame rate to try.
        fps: f64,
        /// Best attempt at `from`.
        attempt: u32,
        /// SSIM score of `attempt`.
        ssim: f64,
    },
    /// Stopped because `fps` did not beat `kept` at `kept_fps` in SSIM.
    LowerLooksNoBetter {
        /// Lower frame rate just tried.
        fps: f64,
        /// Best SSIM achieved at `fps`.
        ssim: f64,
        /// Previously kept attempt number.
        kept: u32,
        /// Frame rate of `kept`.
        kept_fps: f64,
        /// SSIM score of `kept`.
        kept_ssim: f64,
    },
    /// Stopped early via [`CancelToken::settle`](crate::CancelToken::settle),
    /// keeping the best fitting `attempt` so far.
    Settled {
        /// Kept attempt number.
        attempt: u32,
    },
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let percent = |share: f64| format!("{:.1}%", share * 100.0);
        let fps = |fps: f64| {
            let text = format!("{fps:.2}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        match *self {
            Decision::InBand { attempt, share, least } => write!(
                f,
                "stopped: attempt {attempt} uses {} of the limit (target >= {})",
                percent(share),
                percent(least)
            ),
            Decision::AlikeEnough { attempt, ssim } => write!(
                f,
                "stopped: attempt {attempt} already matches the input at {} SSIM; more bits would \
                 only enlarge it",
                percent(ssim)
            ),
            Decision::AlikeUnchanged { attempt, than, ssim } => write!(
                f,
                "stopped: attempt {attempt} reaches {} SSIM, barely improving on attempt {than} \
                 with fewer bits",
                percent(ssim)
            ),
            Decision::LittleToGain { attempt, share, predicted } => write!(
                f,
                "stopped: attempt {attempt} uses {} of the limit; the next attempt would only \
                 reach about {}",
                percent(share),
                percent(predicted)
            ),
            Decision::TooLargeAtLeast { attempt, share } => write!(
                f,
                "stopped: even the lowest value in the range (attempt {attempt}) uses {} of the \
                 limit",
                percent(share)
            ),
            Decision::NothingLeft => write!(f, "stopped: no remaining values worth trying"),
            Decision::OutOfAttempts { attempts } => {
                write!(f, "stopped: reached the limit of {attempts} attempts")
            }
            Decision::LowersFrameRateToFit { from, fps: rate } => {
                write!(f, "trying {} fps: nothing fit at {} fps", fps(rate), fps(from))
            }
            Decision::LowersFrameRateForLooks { from, fps: rate, attempt, ssim } => write!(
                f,
                "trying {} fps: attempt {attempt} at {} fps only reached {} SSIM",
                fps(rate),
                fps(from),
                percent(ssim)
            ),
            Decision::LowerLooksNoBetter { fps: rate, ssim, kept, kept_fps, kept_ssim } => write!(
                f,
                "stopped: {} fps reached {} SSIM, no better than attempt {kept} at {} fps ({} \
                 SSIM)",
                fps(rate),
                percent(ssim),
                fps(kept_fps),
                percent(kept_ssim)
            ),
            Decision::Settled { attempt } => {
                write!(f, "stopped early as requested: kept attempt {attempt}, the best that fits")
            }
        }
    }
}

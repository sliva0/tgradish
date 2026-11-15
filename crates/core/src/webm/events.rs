//! What a WebM conversion's events tell of as it fits: the settings of
//! each encode, and why fitting stopped where it did or tried a change,
//! which [`crate::Event`] carries.

use schemars::JsonSchema;
use serde::Serialize;

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

/// Why fitting stopped where it did, or tried a change or left it
/// untried, said as it decides. Serialised tagged by `reason`, shares of
/// the limit and SSIM from 0 to 1, as `{"reason":"in-band","attempt":2,
/// "share":0.99,"least":0.96}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Decision {
    /// Stopped: `attempt` fits and uses `share` of the limit, at least
    /// the `least` fitting aims for.
    InBand {
        /// The encode.
        attempt: u32,
        /// Its share of the limit.
        share: f64,
        /// The least share that ends fitting.
        least: f64,
    },
    /// Stopped: `attempt` looks as like the input as encodes get, so more
    /// bits would only make it larger.
    AlikeEnough {
        /// The encode.
        attempt: u32,
        /// Its SSIM.
        ssim: f64,
    },
    /// Stopped: `attempt` looks hardly more like the input than `than`,
    /// made with fewer bits, so more still are not worth an encode.
    AlikeUnchanged {
        /// The encode with more bits.
        attempt: u32,
        /// The one with fewer.
        than: u32,
        /// The SSIM of `attempt`.
        ssim: f64,
    },
    /// Stopped: the next encode is predicted to use `predicted` of the
    /// limit, too little more than the `share` of `attempt`, the best
    /// that fits, to be worth it.
    LittleToGain {
        /// The best encode that fits.
        attempt: u32,
        /// Its share of the limit.
        share: f64,
        /// The next one's, as predicted.
        predicted: f64,
    },
    /// Stopped: nothing fits, not even the least of the range searched,
    /// encoded in `attempt`, which uses `share` of the limit.
    TooLargeAtLeast {
        /// The encode of the least.
        attempt: u32,
        /// Its share of the limit.
        share: f64,
    },
    /// Stopped: every value of the range worth trying is tried.
    NothingLeft,
    /// Stopped: all `attempts` encodes allowed are made.
    OutOfAttempts {
        /// How many.
        attempts: u32,
    },
    /// Not trying a lower frame rate: `attempt`, at `fps`, already looks
    /// `ssim` like the input.
    KeepsFrameRate {
        /// The encode.
        attempt: u32,
        /// Its frame rate.
        fps: f64,
        /// Its SSIM.
        ssim: f64,
    },
    /// Not trying a lower frame rate: at `fps`, how like the input
    /// encodes look hardly changed with their bitrate, so more bits for
    /// each frame would not help either.
    FrameRateWontHelp {
        /// The frame rate kept.
        fps: f64,
    },
    /// Trying `fps`: nothing fit at `from`.
    LowersFrameRateToFit {
        /// The frame rate given up.
        from: f64,
        /// The one tried next.
        fps: f64,
    },
    /// Trying `fps`: the best that fits at `from`, `attempt`, looks only
    /// `ssim` like the input.
    LowersFrameRateForLooks {
        /// The frame rate given up, for now.
        from: f64,
        /// The one tried next.
        fps: f64,
        /// The best encode at `from`.
        attempt: u32,
        /// Its SSIM.
        ssim: f64,
    },
    /// Stopped: at `fps`, the best looks `ssim` like the input, no more
    /// than `kept`, at `kept_fps`, does with `kept_ssim`.
    LowerLooksNoBetter {
        /// The frame rate tried.
        fps: f64,
        /// How like the input its best looks.
        ssim: f64,
        /// The encode kept.
        kept: u32,
        /// Its frame rate.
        kept_fps: f64,
        /// Its SSIM.
        kept_ssim: f64,
    },
    /// Stopped early, as asked: `attempt` is kept, the best that fits so
    /// far, see [`CancelToken::settle`](crate::CancelToken::settle).
    Settled {
        /// The encode kept.
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
                "stopped: attempt {attempt} uses {} of the limit, at least the {} aimed for",
                percent(share),
                percent(least)
            ),
            Decision::AlikeEnough { attempt, ssim } => write!(
                f,
                "stopped: attempt {attempt} already looks {} like the input, so more bits would \
                 only make it larger",
                percent(ssim)
            ),
            Decision::AlikeUnchanged { attempt, than, ssim } => write!(
                f,
                "stopped: attempt {attempt} looks {} like the input, hardly more than attempt \
                 {than} with fewer bits",
                percent(ssim)
            ),
            Decision::LittleToGain { attempt, share, predicted } => write!(
                f,
                "stopped: attempt {attempt} uses {} of the limit, and another would use about {}",
                percent(share),
                percent(predicted)
            ),
            Decision::TooLargeAtLeast { attempt, share } => write!(
                f,
                "stopped: even the least of the range, attempt {attempt}, uses {} of the limit",
                percent(share)
            ),
            Decision::NothingLeft => write!(f, "stopped: no value is left worth trying"),
            Decision::OutOfAttempts { attempts } => {
                write!(f, "stopped: all {attempts} attempts are made")
            }
            Decision::KeepsFrameRate { attempt, fps: rate, ssim } => write!(
                f,
                "not trying a lower frame rate: attempt {attempt} at {} fps already looks {} like \
                 the input",
                fps(rate),
                percent(ssim)
            ),
            Decision::FrameRateWontHelp { fps: rate } => write!(
                f,
                "not trying a lower frame rate: at {} fps quality did not depend on the bitrate, \
                 so more bits a frame would not help",
                fps(rate)
            ),
            Decision::LowersFrameRateToFit { from, fps: rate } => {
                write!(f, "trying {} fps: nothing fit at {} fps", fps(rate), fps(from))
            }
            Decision::LowersFrameRateForLooks { from, fps: rate, attempt, ssim } => write!(
                f,
                "trying {} fps: attempt {attempt} at {} fps looks only {} like the input",
                fps(rate),
                fps(from),
                percent(ssim)
            ),
            Decision::LowerLooksNoBetter { fps: rate, ssim, kept, kept_fps, kept_ssim } => write!(
                f,
                "stopped: {} fps looks {} like the input, no better than attempt {kept} at {} fps \
                 with {}",
                fps(rate),
                percent(ssim),
                fps(kept_fps),
                percent(kept_ssim)
            ),
            Decision::Settled { attempt } => {
                write!(f, "stopped early, as asked: kept attempt {attempt}, the best that fits")
            }
        }
    }
}

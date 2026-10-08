//! Conversion options, shared by presets, the CLI and any GUI.
//!
//! Every field is optional so options can be layered: built-in defaults,
//! then a preset, then command-line flags. Missing values get defaults when a
//! conversion is planned, some of them based on the input video.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::telegram::Target;

/// How the video is scaled into the target box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Resize {
    /// Keep the aspect ratio, touch the box with the longer side. Default
    /// for stickers.
    Contain,
    /// Like contain, then fill the rest of the box with transparency.
    /// Default for emoji, which must be exactly square.
    Pad,
    /// Keep the aspect ratio, fill the box and cut off what sticks out.
    Crop,
    /// Ignore the aspect ratio and stretch to the box.
    Stretch,
}

/// What to tune so the file ends up just under the size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// Try a few frame rates, fit the bitrate for each and keep the one that
    /// looks most like the source (by SSIM). Slowest, best looking.
    Auto,
    /// Fit the bitrate at the configured frame rate.
    Bitrate,
    /// Fit the constant quality value (CRF).
    Crf,
    /// Fit the frame rate at constant quality.
    Fps,
    /// Fit the length at constant quality.
    Length,
    /// Encode once with the given settings, even if the result is too big.
    Off,
}

/// Encoder speed. Faster settings produce worse looking stickers at the same
/// size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Speed {
    Fast,
    Balanced,
    Best,
}

impl Speed {
    /// libvpx-vp9 `-cpu-used` value for the "good" deadline.
    pub fn cpu_used(self) -> u8 {
        match self {
            Speed::Fast => 4,
            Speed::Balanced => 2,
            Speed::Best => 0,
        }
    }
}

/// When to spoof the duration header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Spoof {
    /// Only when the video is longer than Telegram allows.
    Auto,
    Always,
    /// Never; videos longer than Telegram allows are cut to fit.
    Never,
}

/// A `[min, max]` range for the value tuned by [`Fit`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl std::str::FromStr for Range {
    type Err = String;

    /// Parses `MIN..MAX`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (min, max) = s.split_once("..").ok_or("expected MIN..MAX, for example 100..400")?;
        let parse = |v: &str| v.trim().parse::<f64>().map_err(|err| format!("{v:?}: {err}"));
        let range = Range { min: parse(min)?, max: parse(max)? };
        if !(range.min.is_finite() && range.max.is_finite() && range.min <= range.max) {
            return Err(format!("{s:?} is not a valid range"));
        }
        Ok(range)
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.min, self.max)
    }
}

/// Settings for one conversion. `None` means "use the default".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// What to make. Default: sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// How to scale the video into the target size. Default: contain for
    /// stickers, pad for emoji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// What to tune to get close to the 256 KB limit. Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// Maximum number of encodes while fitting. Default: 8.
    #[schemars(range(min = 1, max = 50))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// Range searched while fitting, in the unit of the fitted value
    /// (kbit/s, CRF, fps or seconds). Default depends on `fit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit_range: Option<Range>,
    /// Seconds to skip at the start of the input. Default: 0.
    #[schemars(range(min = 0.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Length of the result in seconds. Default: the rest of the input, or
    /// at most 3 seconds when not spoofing.
    #[schemars(extend("exclusiveMinimum" = 0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Frame rate of the result. Default: the input frame rate, at most 30.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 30))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Target bitrate in kbit/s. Used when not fitting bitrate. Default:
    /// estimated from the size limit.
    #[schemars(range(min = 1.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<f64>,
    /// Constant quality from 0 (best) to 63 (worst). Used when fitting
    /// frame rate or length, or with `fit = "off"`. Default: 32.
    #[schemars(range(min = 0, max = 63))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crf: Option<u8>,
    /// Encoder speed. Default: balanced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    /// Lossless encoding. Only useful for tiny or static videos. Default:
    /// false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lossless: Option<bool>,
    /// When to spoof the duration header so Telegram accepts videos longer
    /// than 3 seconds. Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoof: Option<Spoof>,
    /// Duration written into the header when spoofing, in seconds. Default:
    /// 0.42069.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 3))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fake_duration: Option<f64>,
    /// Title stored in the file metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Mark the file as made by tgradish: writing and muxing app metadata
    /// and a signature hidden in padding. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
    /// libvpx-vp9 encoder options by name, for tuning beyond what the other
    /// options cover, for example `tune-content = "screen"` for flat
    /// graphics, `aq-mode = "2"`, `sharpness = "4"`, `arnr-strength = "3"`,
    /// `g = "60"` (keyframe interval) or `qmax = "50"`. `ffmpeg -h
    /// encoder=libvpx-vp9` lists them all. Unknown names are an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_options: Option<BTreeMap<String, String>>,
    /// Raw ffmpeg arguments, added before the output file. Only with
    /// ffmpeg as a separate program (the system one or `--ffmpeg PATH`),
    /// since the built-in ffmpeg has no command line; prefer
    /// `encoder-options` for encoder settings, which work with both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_args: Option<Vec<String>>,
}

impl Options {
    /// Returns `self` with every value set in `overlay` replacing its own.
    pub fn merged(mut self, overlay: &Options) -> Options {
        macro_rules! merge {
            ($($field:ident),* $(,)?) => {
                // destructuring makes this fail to compile when a field is added
                let Options { $($field: _),* } = overlay;
                $(if overlay.$field.is_some() {
                    self.$field = overlay.$field.clone();
                })*
            };
        }
        merge!(
            target,
            resize,
            fit,
            attempts,
            fit_range,
            start,
            length,
            fps,
            bitrate,
            crf,
            speed,
            lossless,
            spoof,
            fake_duration,
            title,
            watermark,
            encoder_options,
            extra_args,
        );
        self
    }
}

pub const DEFAULT_ATTEMPTS: u32 = 8;
pub const MAX_ATTEMPTS: u32 = 50;
pub const DEFAULT_CRF: u8 = 32;
pub const DEFAULT_FAKE_DURATION: f64 = 0.42069;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_set_values_only() {
        let base = Options { fit: Some(Fit::Auto), crf: Some(20), ..Default::default() };
        let overlay = Options { crf: Some(40), title: Some("x".into()), ..Default::default() };
        let merged = base.merged(&overlay);
        assert_eq!(merged.fit, Some(Fit::Auto));
        assert_eq!(merged.crf, Some(40));
        assert_eq!(merged.title.as_deref(), Some("x"));
    }

    #[test]
    fn parses_ranges() {
        assert_eq!("100..400".parse(), Ok(Range { min: 100.0, max: 400.0 }));
        assert_eq!("0.5 .. 2".parse(), Ok(Range { min: 0.5, max: 2.0 }));
        assert!("400..100".parse::<Range>().is_err());
        assert!("100".parse::<Range>().is_err());
        assert!("a..b".parse::<Range>().is_err());
    }

    #[test]
    fn reads_kebab_case_toml() {
        let options: Options = toml::from_str(
            "target = 'emoji'\nfit = 'bitrate'\nfit-range = { min = 50, max = 300 }\n\
             fake-duration = 1.5\nencoder-options = { tune-content = 'screen' }",
        )
        .unwrap();
        assert_eq!(options.target, Some(Target::Emoji));
        assert_eq!(options.fit_range, Some(Range { min: 50.0, max: 300.0 }));
        assert_eq!(options.fake_duration, Some(1.5));
        assert!(toml::from_str::<Options>("unknown = 1").is_err());
    }
}

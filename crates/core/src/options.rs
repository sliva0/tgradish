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

/// How the picture's pixels become the result's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Scaling {
    /// Sharp for pixel art made at least twice as large, smooth otherwise.
    Auto,
    /// Neighbouring pixels blend, in linear light so edges don't darken.
    Smooth,
    /// Each pixel becomes a block; only block edges that fall between
    /// output pixels blend. Keeps pixel art crisp at any size.
    Sharp,
    /// Each pixel becomes the same whole number of output pixels, and
    /// transparent margins fill the rest of the box.
    PixelPerfect,
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

/// The part of the input to use, in its pixels as shown (after rotation),
/// from the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    #[schemars(range(min = 1))]
    pub width: u32,
    #[schemars(range(min = 1))]
    pub height: u32,
}

impl Crop {
    /// Whether the crop lies within a `width` by `height` picture.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && u64::from(self.x) + u64::from(self.width) <= u64::from(width)
            && u64::from(self.y) + u64::from(self.height) <= u64::from(height)
    }

    /// An error message unless the crop lies within a `width` by `height`
    /// input.
    pub fn check(&self, width: u32, height: u32) -> Result<(), String> {
        if self.fits(width, height) {
            Ok(())
        } else {
            Err(format!("crop {self} is not within the {width}x{height} input"))
        }
    }
}

impl std::str::FromStr for Crop {
    type Err = String;

    /// Parses `WIDTHxHEIGHT+X+Y`, or `WIDTHxHEIGHT` for the top left.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("{s:?}: expected WIDTHxHEIGHT+X+Y, for example 640x360+100+50");
        let (size, offset) = s.split_once('+').unwrap_or((s, "0+0"));
        let (width, height) = size.split_once('x').ok_or_else(invalid)?;
        let (x, y) = offset.split_once('+').ok_or_else(invalid)?;
        let number = |v: &str| v.trim().parse::<u32>().map_err(|_| invalid());
        let crop =
            Crop { x: number(x)?, y: number(y)?, width: number(width)?, height: number(height)? };
        if crop.width == 0 || crop.height == 0 {
            return Err(format!("{s:?}: the crop must be at least 1x1"));
        }
        Ok(crop)
    }
}

impl std::fmt::Display for Crop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}+{}+{}", self.width, self.height, self.x, self.y)
    }
}

/// A scale that lines input pixels up with the result's: a whole number of
/// result pixels for each input pixel (`2`), or the other way round
/// (`1/2`). Written as `N`, `1/N` or a decimal like `0.5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExactScale {
    /// Result pixels along an input pixel; 1 when shrinking.
    pub up: u32,
    /// Input pixels along a result pixel; 1 when growing.
    pub down: u32,
}

impl ExactScale {
    pub const ONE: ExactScale = ExactScale { up: 1, down: 1 };

    /// `size` input pixels at this scale, if they make whole result pixels.
    pub fn apply(self, size: u32) -> Option<u32> {
        size.is_multiple_of(self.down).then(|| size / self.down * self.up)
    }

    /// The input pixels that make `size` result pixels, if whole.
    pub fn input_for(self, size: u32) -> Option<u32> {
        size.is_multiple_of(self.up).then(|| size / self.up * self.down)
    }

    pub fn factor(self) -> f64 {
        f64::from(self.up) / f64::from(self.down)
    }
}

impl std::str::FromStr for ExactScale {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("{s:?}: expected a whole number like 2, or 1/N like 1/2");
        let text = s.trim().trim_end_matches(['x', '×']);
        let whole = |v: &str| v.trim().parse::<u32>().ok().filter(|&n| n > 0);
        if let Some((one, n)) = text.split_once('/') {
            return match (whole(one), whole(n)) {
                (Some(1), Some(n)) => Ok(ExactScale { up: 1, down: n }),
                _ => Err(invalid()),
            };
        }
        if let Some(n) = whole(text) {
            return Ok(ExactScale { up: n, down: 1 });
        }
        // a decimal: whole, or one over a whole number
        let value: f64 = text.parse().map_err(|_| invalid())?;
        if !(value > 0.0 && value.is_finite()) {
            return Err(invalid());
        }
        let down = (1.0 / value).round();
        if value < 1.0 && (1.0 / down - value).abs() < 1e-3 {
            return Ok(ExactScale { up: 1, down: down as u32 });
        }
        Err(invalid())
    }
}

impl std::fmt::Display for ExactScale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.down {
            1 => write!(f, "{}", self.up),
            down => write!(f, "{}/{down}", self.up),
        }
    }
}

impl TryFrom<String> for ExactScale {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ExactScale> for String {
    fn from(value: ExactScale) -> String {
        value.to_string()
    }
}

impl JsonSchema for ExactScale {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ExactScale".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^([1-9][0-9]*|1/[1-9][0-9]*)$",
            "description": "N result pixels for each input pixel, or 1/N"
        })
    }
}

/// Settings for one conversion. `None` means "use the default".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// What to make. Default: sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// The part of the picture to use, in input pixels. Default: all of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// How to scale the video into the target size. Default: contain for
    /// stickers, pad for emoji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// How pixels are scaled: smooth, or kept sharp for pixel art. Default:
    /// auto, sharp for pixel art made at least twice as large.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    /// Scale by exactly this much, so input and result pixels line up: the
    /// crop must make the result's size (512 pixels on a sticker's longer
    /// side, 100 x 100 for emoji). Overrides `resize` and `scaling`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_scale: Option<ExactScale>,
    /// What to tune to get close to the size limit: 256 KB for stickers, 64
    /// KB for emoji. Default: auto.
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
    /// Frame rate of the result, at most 60. Default: the input frame rate,
    /// at most 30.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 60))]
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
    /// and a signature in padding. A mark hidden in the video stays either
    /// way (see `tgradish_core::mark`). Default: true.
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
            crop,
            resize,
            scaling,
            exact_scale,
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
    fn parses_crops() {
        let crop = Crop { x: 100, y: 50, width: 640, height: 360 };
        assert_eq!("640x360+100+50".parse(), Ok(crop));
        assert_eq!(crop.to_string(), "640x360+100+50");
        assert_eq!("64x32".parse(), Ok(Crop { x: 0, y: 0, width: 64, height: 32 }));
        assert!("0x32".parse::<Crop>().is_err());
        assert!("64x32+1".parse::<Crop>().is_err());
        assert!(crop.fits(740, 410) && !crop.fits(739, 410));
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

//! Conversion options, shared by presets, the command line and the window.
//!
//! Every field is optional so options can be layered: built-in defaults,
//! then a preset, then command-line flags. Missing values get defaults when a
//! conversion is planned, some of them based on the input video.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::telegram::Target;

/// How the video is scaled into the target box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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
    /// Neighbouring pixels blend.
    Smooth,
    /// Each pixel becomes a block, and blocks blend only at their edges.
    /// Keeps pixel art crisp at any size.
    Sharp,
    /// Each pixel becomes the same whole number of output pixels, and
    /// transparent margins fill the rest of the box.
    PixelPerfect,
}

/// What to tune so the file ends up just under the file size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// Try a few frame rates, fit the bitrate for each and keep the one that
    /// looks most like the input (by SSIM). Slowest, best looking.
    Auto,
    /// Fit the bitrate at the configured frame rate.
    Bitrate,
    /// Fit the constant quality value (CRF).
    Crf,
    /// Fit the frame rate at constant quality.
    Fps,
    /// Fit the length at constant quality.
    Length,
    /// Encode once with the given settings, even if the result is too large.
    Off,
}

/// What a fit range counts, which depends on the way of fitting it is
/// for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FitUnit {
    /// Bitrates, in kbit/s.
    Kbps,
    /// libvpx's constant qualities.
    Crf,
    /// Frame rates.
    Fps,
    /// Lengths, in seconds.
    Seconds,
}

impl Fit {
    /// What a range searched while fitting this way counts. Fitting
    /// nothing counts bitrates, as the way it encodes is by one.
    pub fn unit(self) -> FitUnit {
        match self {
            Fit::Auto | Fit::Bitrate | Fit::Off => FitUnit::Kbps,
            Fit::Crf => FitUnit::Crf,
            Fit::Fps => FitUnit::Fps,
            Fit::Length => FitUnit::Seconds,
        }
    }

    /// What the ends of a range searched while fitting this way can be:
    /// bitrates of at least 1 kbit/s, libvpx's CRFs, frame rates up to
    /// Telegram's, and lengths above 0, which planning also keeps within
    /// the result's.
    pub fn range_limits(self) -> std::ops::RangeInclusive<f64> {
        match self.unit() {
            FitUnit::Kbps => 1.0..=f64::INFINITY,
            FitUnit::Crf => f64::from(*CRFS.start())..=f64::from(*CRFS.end()),
            FitUnit::Fps => 1.0..=crate::telegram::MAX_FPS,
            FitUnit::Seconds => f64::MIN_POSITIVE..=f64::INFINITY,
        }
    }
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

/// When to spoof the duration in the metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Spoof {
    /// Only when the video is longer than Telegram allows.
    Auto,
    Always,
    /// Never; videos longer than Telegram allows are cut to fit.
    Never,
}

/// A `[min, max]` range for the value tuned by [`Fit`], in its
/// [`FitUnit`].
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
    /// All of a picture of `size`.
    pub fn whole((width, height): (u32, u32)) -> Crop {
        Crop { x: 0, y: 0, width, height }
    }

    /// Whether the crop lies within a `width` by `height` picture.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && u64::from(self.x) + u64::from(self.width) <= u64::from(width)
            && u64::from(self.y) + u64::from(self.height) <= u64::from(height)
    }

    /// Fails unless the crop lies within a `width` by `height` input.
    pub fn check(&self, width: u32, height: u32) -> crate::Result<()> {
        if self.fits(width, height) {
            Ok(())
        } else {
            let reason = format!("{self} is not within the {width}x{height} input");
            Err(crate::Error::invalid("crop", reason))
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

/// How a sprite sheet is laid out: `columns` by `rows` equal cells, its
/// frames row by row. Written `COLUMNSxROWS`, as `4x2`, the `x` either
/// case and spaces around the numbers allowed; reading it, as from a
/// file, refuses anything else, a side of 0 included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sheet {
    pub columns: u32,
    pub rows: u32,
}

impl std::str::FromStr for Sheet {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("{s:?} is not COLUMNSxROWS, for example 4x2");
        let (columns, rows) = s.split_once(['x', 'X']).ok_or_else(invalid)?;
        let number = |n: &str| n.trim().parse::<u32>().ok().filter(|&n| n > 0).ok_or_else(invalid);
        Ok(Sheet { columns: number(columns)?, rows: number(rows)? })
    }
}

impl std::fmt::Display for Sheet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.columns, self.rows)
    }
}

impl TryFrom<String> for Sheet {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Sheet> for String {
    fn from(value: Sheet) -> String {
        value.to_string()
    }
}

impl JsonSchema for Sheet {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Sheet".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^\s*[0-9]*[1-9][0-9]*\s*[xX]\s*[0-9]*[1-9][0-9]*\s*$",
            "description": "COLUMNSxROWS equal cells, frames row by row"
        })
    }
}

/// A scale that lines input pixels up with the result's: a whole number of
/// result pixels for each input pixel (`2`), or the other way round
/// (`1/2`). Written as `N`, `1/N` or a decimal like `0.5`; reading it, as
/// from a preset, refuses anything else, zero included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExactScale {
    /// Result pixels along an input pixel; 1 when shrinking. Never 0.
    pub up: u32,
    /// Input pixels along a result pixel; 1 when growing. Never 0.
    pub down: u32,
}

impl ExactScale {
    /// The largest scale either way: a pixel to a whole sticker side.
    pub const MAX: u32 = 512;

    /// Whether it scales by anything: a scale made in code with a side of
    /// 0 makes no pixels, and planning refuses it.
    pub fn is_valid(self) -> bool {
        self.up > 0 && self.down > 0
    }

    /// `size` input pixels at this scale, if they make whole result pixels.
    pub fn apply(self, size: u32) -> Option<u32> {
        if !self.is_valid() || !size.is_multiple_of(self.down) {
            return None;
        }
        (size / self.down).checked_mul(self.up)
    }

    /// The most input pixels that make at most `size` result pixels: none
    /// at a scale that isn't valid.
    pub fn most_input(self, size: u32) -> u32 {
        size.checked_div(self.up).map_or(0, |whole| whole.saturating_mul(self.down))
    }
}

impl std::str::FromStr for ExactScale {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || {
            format!(
                "{s:?}: expected a whole number like 2, or 1/N like 1/2, up to {}",
                ExactScale::MAX
            )
        };
        let text = s.trim().trim_end_matches(['x', '×']);
        let whole =
            |v: &str| v.trim().parse::<u32>().ok().filter(|&n| (1..=ExactScale::MAX).contains(&n));
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
        if value < 1.0 && down <= f64::from(ExactScale::MAX) && (1.0 / down - value).abs() < 1e-3 {
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
    /// Aseprite tag to use. Default: all frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Aseprite layers to show, by name, hidden ones included. Default:
    /// those the file shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_layers: Option<Vec<String>>,
    /// Aseprite layers to hide, by name; a hidden group hides the layers in
    /// it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_layers: Option<Vec<String>>,
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
    /// Scale by exactly this much, so input and result pixels line up. The
    /// crop must fit the result at this scale; transparent margins fill the
    /// rest. Overrides `scaling`, and `resize` must be contain or pad.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_scale: Option<ExactScale>,
    /// What to tune to get close to the file size limit: 256 KiB for
    /// stickers, 64 KiB for emoji. Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// Maximum number of encodes while fitting. Default: 8.
    #[schemars(range(min = 1, max = 50))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// Range searched while fitting, in the unit of the fitted value
    /// (kbit/s, CRF, fps or seconds), so it belongs to the `fit` it is
    /// written for: layered over options that fit in another unit without
    /// a range of its own, the range below is dropped. Default depends on
    /// `fit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit_range: Option<Range>,
    /// Seconds to skip at the start of the input. Default: 0.
    #[schemars(range(min = 0.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Seconds of the input to use, from `start`. Default: the rest of the
    /// input, or as much as plays for 3 seconds when not spoofing.
    #[schemars(extend("exclusiveMinimum" = 0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// How many times as fast as the input the result plays, 0.1 to 10: 2
    /// plays it in half the time. Default: 1.
    #[schemars(range(min = 0.1, max = 10.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<f64>,
    /// Frame rate of the result, at most 60: more than the input's at the
    /// tempo only repeats its frames. For a sequence of images, how fast
    /// its frames play, which the result's follows. Default: the input's
    /// at the tempo, from 1 to 60; 12 for a sequence.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 60))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Target bitrate in kbit/s. Used when not fitting bitrate. Default:
    /// estimated from the file size limit.
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
    /// Lossless encoding. Only small for tiny or still pictures. Default:
    /// false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lossless: Option<bool>,
    /// When to spoof the duration in the metadata so Telegram accepts videos
    /// longer than 3 seconds. Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoof: Option<Spoof>,
    /// Duration encoded into the metadata when spoofing, in seconds. Default:
    /// a short one Telegram accepts.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 3))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fake_duration: Option<f64>,
    /// Title stored in the file metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Mark the file as made by tgradish: writing and muxing app metadata
    /// and a signature in padding. Default: true.
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
    pub ffmpeg_args: Option<Vec<String>>,
}

/// Gives a struct of options, every field an `Option`, `overlaid`: the
/// options with every value another sets in place of their own, which is
/// how presets, items and flags layer. Every field is named, and the
/// struct destructured by them, so adding one without naming it here fails
/// to compile.
macro_rules! layered {
    ($options:ident { $($field:ident),* $(,)? }) => {
        impl $options {
            /// These options with every value `overlay` sets in place of
            /// their own.
            fn overlaid(mut self, overlay: &$options) -> $options {
                let $options { $($field: _),* } = overlay;
                $(if overlay.$field.is_some() {
                    self.$field = overlay.$field.clone();
                })*
                self
            }
        }
    };
}
pub(crate) use layered;

layered!(Options {
    target,
    tag,
    show_layers,
    hide_layers,
    crop,
    resize,
    scaling,
    exact_scale,
    fit,
    attempts,
    fit_range,
    start,
    length,
    tempo,
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
    ffmpeg_args,
});

/// The values of options left unset, which converting takes: each says
/// its default once, for core and front-ends alike.
impl Options {
    /// The way of fitting: auto.
    pub fn fit_or_default(&self) -> Fit {
        self.fit.unwrap_or(Fit::Auto)
    }

    /// The encoder's speed: balanced.
    pub fn speed_or_default(&self) -> Speed {
        self.speed.unwrap_or(Speed::Balanced)
    }

    /// How pixels are scaled: auto, by the input.
    pub fn scaling_or_default(&self) -> Scaling {
        self.scaling.unwrap_or(Scaling::Auto)
    }

    /// When the duration is spoofed: auto, when the result is longer than
    /// Telegram plays.
    pub fn spoof_or_default(&self) -> Spoof {
        self.spoof.unwrap_or(Spoof::Auto)
    }

    /// How many times as fast as the input it plays: as fast.
    pub fn tempo_or_default(&self) -> f64 {
        self.tempo.unwrap_or(1.0)
    }

    /// Whether to encode losslessly: no.
    pub fn lossless_or_default(&self) -> bool {
        self.lossless.unwrap_or(false)
    }

    /// Whether how pixels are scaled is left to the input: auto, with no
    /// exact scale, which places pixels itself.
    pub fn auto_scaling(&self) -> bool {
        self.scaling_or_default() == Scaling::Auto && self.exact_scale.is_none()
    }

    /// Returns `self` with every value set in `overlay` replacing its own.
    /// A range to fit within belongs to the way of fitting it was written
    /// for: when `overlay` fits in another unit, as a bitrate where `self`
    /// fits a quality, and sets no range of its own, `self`'s is dropped
    /// rather than read in the wrong unit.
    pub fn merged(self, overlay: &Options) -> Options {
        let unit = self.fit_or_default().unit();
        let mut merged = self.overlaid(overlay);
        if overlay.fit.is_some_and(|fit| fit.unit() != unit) && overlay.fit_range.is_none() {
            merged.fit_range = None;
        }
        merged
    }
}

/// `options` without the values that have no say in a conversion that
/// gets `effective`, all it is given, by how fitting uses them: the tries
/// and range of fitting that is off; a quality, or lossless encoding,
/// where a bitrate or the quality itself is fitted; a bitrate where a
/// quality is kept, or where fitting finds its own from the start; a
/// quality beside lossless encoding, or beside a bitrate that encoding
/// once goes by; and the duration spoofing writes
/// where it never spoofs. Planning warns of a quality or bitrate given
/// where it has no say, and refuses lossless encoding there; front-ends
/// set such values aside.
pub fn in_effect(mut options: Options, effective: &Options) -> Options {
    let fit = effective.fit_or_default();
    let constant_quality = matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    let lossless = constant_quality && effective.lossless_or_default();
    if fit == Fit::Off {
        (options.attempts, options.fit_range) = (None, None);
    }
    if !constant_quality {
        (options.crf, options.lossless) = (None, None);
    }
    // fitting a bitrate starts from the one given; the others don't
    // encode by one, but off does
    if !matches!(fit, Fit::Bitrate | Fit::Off) || lossless {
        options.bitrate = None;
    }
    if lossless || (fit == Fit::Off && effective.bitrate.is_some()) {
        options.crf = None;
    }
    if effective.spoof == Some(Spoof::Never) {
        options.fake_duration = None;
    }
    options
}

/// The keys of the values `options` sets, as files, JSON and flags name
/// them, in the order of its fields.
pub fn set_keys(options: &impl Serialize) -> Vec<String> {
    match serde_json::to_value(options) {
        Ok(serde_json::Value::Object(fields)) => fields.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Command line flags that set the values `options` sets: the flag of each
/// field has its name, as `tgradish convert` has them. A flag and its
/// value are one word, `--name=value`: in a word of its own, a value that
/// starts with a dash, as a title or a layer's name can, would be taken
/// for another flag.
pub fn flags(options: &impl Serialize) -> Vec<String> {
    let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(options) else {
        return Vec::new();
    };
    // to nine decimals, far finer than any cut between frames, so the
    // noise of sums and differences of times doesn't print as
    // 0.07999999999999996
    let number = |value: &serde_json::Value| match value.as_f64() {
        Some(n) if n.abs() < 1e15 => {
            let n = (n * 1e9).round() / 1e9;
            if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
        }
        Some(n) => format!("{n}"),
        None => value.to_string(),
    };
    let mut flags = Vec::new();
    for (name, value) in fields {
        let flag = format!("--{name}");
        let set = |value: &dyn std::fmt::Display| format!("{flag}={value}");
        use serde_json::Value;
        match (name.as_str(), value) {
            ("crop", value) => {
                if let Ok(crop) = serde_json::from_value::<Crop>(value) {
                    flags.push(set(&crop));
                }
            }
            ("fit-range", value) => {
                if let Ok(range) = serde_json::from_value::<Range>(value) {
                    flags.push(set(&range));
                }
            }
            // none, over a preset's
            ("encoder-options", Value::Object(pairs)) if pairs.is_empty() => flags.push(set(&"")),
            ("encoder-options", Value::Object(pairs)) => {
                for (key, value) in pairs {
                    let value = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
                    flags.push(set(&format_args!("{key}={value}")));
                }
            }
            // one each, as names can have commas
            ("show-layers" | "hide-layers", Value::Array(names)) => {
                flags.extend(names.iter().filter_map(Value::as_str).map(|name| set(&name)));
            }
            ("ffmpeg-args", Value::Array(words)) => {
                let words: Vec<&str> = words.iter().filter_map(Value::as_str).collect();
                if let Ok(joined) = shlex::try_join(words) {
                    flags.push(set(&joined));
                }
            }
            (_, Value::Array(items)) => {
                let items: Vec<String> = items
                    .iter()
                    .map(|item| item.as_str().map_or_else(|| item.to_string(), str::to_owned))
                    .collect();
                flags.push(set(&items.join(",")));
            }
            (_, Value::Bool(true)) => flags.push(flag),
            (_, Value::Bool(false)) => flags.push(set(&false)),
            (_, Value::String(text)) => flags.push(set(&text)),
            (_, value @ Value::Number(_)) => flags.push(set(&number(&value))),
            _ => {}
        }
    }
    flags
}

/// The highest bitrate worth asking for, in kbit/s, far past what any
/// sticker fits: the top of the range fitting searches by default.
pub const HIGHEST_BITRATE: f64 = 50_000.0;

/// Encodes fitting may make: enough to land within a few percent of the
/// limit for most inputs, at seconds an encode.
pub const DEFAULT_ATTEMPTS: u32 = 8;
/// The most it may make, against a mistyped number.
pub const MAX_ATTEMPTS: u32 = 50;
/// The constant quality where nothing is fitted to it: good for libvpx-vp9
/// at sticker sizes.
pub const DEFAULT_CRF: u8 = 32;
/// libvpx's constant qualities, the best first.
pub const CRFS: std::ops::RangeInclusive<u8> = 0..=63;
/// The duration spoofing writes by default: well under Telegram's 3
/// seconds, and odd enough that a spoofed header is easy to spot.
pub const DEFAULT_FAKE_DURATION: f64 = 0.42069;
/// The slowest and fastest tempos, for WebM and .tgs alike.
pub const TEMPOS: std::ops::RangeInclusive<f64> = tgradish_tgs::normalise::TEMPOS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_print_numbers_plainly() {
        let options = Options {
            // as a cut of 0.12 s from 0.04 s comes out
            length: Some(0.12 - 0.04),
            start: Some(0.1 + 1e-6),
            crf: Some(30),
            ..Default::default()
        };
        let flags = flags(&options);
        for flag in ["--length=0.08", "--start=0.100001", "--crf=30"] {
            assert!(flags.iter().any(|written| written == flag), "{flag}: {flags:?}");
        }
    }

    #[test]
    fn flags_carry_their_values() {
        // a value in a word of its own could be taken for a flag
        let options = Options {
            title: Some("-_-".into()),
            show_layers: Some(vec!["--help".into(), "a, b".into()]),
            lossless: Some(true),
            watermark: Some(false),
            encoder_options: Some([("aq-mode".to_owned(), "2".to_owned())].into()),
            ..Default::default()
        };
        assert_eq!(
            flags(&options),
            [
                "--show-layers=--help",
                "--show-layers=a, b",
                "--lossless",
                "--title=-_-",
                "--watermark=false",
                "--encoder-options=aq-mode=2",
            ]
        );
    }

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
    fn sets_aside_what_has_no_say() {
        let all = Options {
            attempts: Some(3),
            fit_range: Some(Range { min: 1.0, max: 2.0 }),
            crf: Some(30),
            bitrate: Some(500.0),
            lossless: Some(false),
            fake_duration: Some(1.0),
            ..Options::default()
        };
        // what is kept
        let with = |fit, more: Options| {
            let effective = Options { fit: Some(fit), ..more.merged(&all) };
            set_keys(&in_effect(all.clone(), &effective))
        };
        let none = Options::default();
        assert_eq!(with(Fit::Auto, none.clone()), ["attempts", "fit-range", "fake-duration"]);
        assert_eq!(
            with(Fit::Bitrate, none.clone()),
            ["attempts", "fit-range", "bitrate", "fake-duration"]
        );
        assert_eq!(
            with(Fit::Fps, none.clone()),
            ["attempts", "fit-range", "crf", "lossless", "fake-duration"]
        );
        // off encodes once, by the bitrate given rather than the quality
        assert_eq!(with(Fit::Off, none), ["bitrate", "lossless", "fake-duration"]);
        let crf = Options { bitrate: None, ..all.clone() };
        let effective = Options { fit: Some(Fit::Off), ..crf.clone() };
        assert_eq!(set_keys(&in_effect(crf, &effective)), ["crf", "lossless", "fake-duration"]);
        let lossless =
            Options { lossless: Some(true), spoof: Some(Spoof::Never), ..Options::default() };
        let effective = Options { fit: Some(Fit::Off), ..all.clone().merged(&lossless) };
        assert_eq!(set_keys(&in_effect(all.clone(), &effective)), ["lossless"]);
    }

    #[test]
    fn says_its_defaults_once() {
        let unset = Options::default();
        assert_eq!(
            (unset.fit_or_default(), unset.speed_or_default(), unset.scaling_or_default()),
            (Fit::Auto, Speed::Balanced, Scaling::Auto)
        );
        assert_eq!((unset.spoof_or_default(), unset.lossless_or_default()), (Spoof::Auto, false));
        let set = Options { fit: Some(Fit::Crf), lossless: Some(true), ..unset };
        assert_eq!((set.fit_or_default(), set.lossless_or_default()), (Fit::Crf, true));
    }

    #[test]
    fn drops_a_range_of_another_unit() {
        let qualities = Some(Range { min: 20.0, max: 40.0 });
        let base = Options { fit: Some(Fit::Crf), fit_range: qualities, ..Default::default() };
        let fit = |fit| Options { fit: Some(fit), ..Default::default() };
        // fitting a bitrate: a range of qualities is none of bitrates
        assert_eq!(base.clone().merged(&fit(Fit::Bitrate)).fit_range, None);
        // the same unit, or a range of its own, or no other fit: kept
        assert_eq!(base.clone().merged(&fit(Fit::Crf)).fit_range, qualities);
        let own = Some(Range { min: 100.0, max: 400.0 });
        let bitrates = Options { fit_range: own, ..fit(Fit::Bitrate) };
        assert_eq!(base.clone().merged(&bitrates).fit_range, own);
        let crf = Options { crf: Some(30), ..Default::default() };
        assert_eq!(base.merged(&crf).fit_range, qualities);
        // a range with no fit is one of bitrates, as fitting is by default
        let auto = Options { fit_range: own, ..Default::default() };
        assert_eq!(auto.clone().merged(&fit(Fit::Off)).fit_range, own);
        assert_eq!(auto.merged(&fit(Fit::Fps)).fit_range, None);
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
    fn reads_no_scale_of_zero() {
        for text in ["0", "1/0", "0/1", "0/0", "0.0", "0x", "-1", "1/-2", "513", "1/513"] {
            assert!(text.parse::<ExactScale>().is_err(), "{text}");
            let preset = format!("exact-scale = '{text}'");
            assert!(toml::from_str::<Options>(&preset).is_err(), "{text}");
        }
        // as code can still make one
        for zero in [ExactScale { up: 0, down: 1 }, ExactScale { up: 1, down: 0 }] {
            assert!(!zero.is_valid());
            assert_eq!((zero.apply(4), zero.most_input(512)), (None, 0), "{zero:?}");
        }
        let half: ExactScale = "1/2".parse().unwrap();
        assert!(half.is_valid() && half.apply(4) == Some(2) && half.most_input(512) == 1024);
    }

    #[test]
    fn parses_sheets() {
        let sheet = Sheet { columns: 4, rows: 2 };
        for text in ["4x2", "4X2", " 4 x 2 ", "04x2"] {
            assert_eq!(text.parse(), Ok(sheet), "{text}");
        }
        assert_eq!(sheet.to_string(), "4x2");
        for text in ["4 by 2", "0x2", "4x0", "4x", "x2", "4x2x1", "-1x2"] {
            assert!(text.parse::<Sheet>().is_err(), "{text}");
        }
        // as a file has it, refused there too
        let read = |text: &str| serde_json::from_value::<Sheet>(serde_json::json!(text));
        assert_eq!(read("2X3").unwrap(), Sheet { columns: 2, rows: 3 });
        assert!(read("foo").is_err());
        assert_eq!(serde_json::to_value(sheet).unwrap(), "4x2");
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

//! The values conversion options take, shared by every format's options:
//! [`webm::Options`](crate::webm::Options), which image stickers take too,
//! and [`tgs::Options`](crate::tgs::TgsOptions); and how options are
//! layered and written as flags.
//!
//! Every field of options is optional so they can be layered: built-in
//! defaults, then a preset, then command-line flags. Missing values get
//! defaults when a conversion is planned, some of them based on the input
//! video.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::webm::CRFS;

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

/// The slowest and fastest tempos, for WebM and .tgs alike.
pub const TEMPOS: std::ops::RangeInclusive<f64> = tgradish_tgs::normalise::TEMPOS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::Target;
    use crate::webm::Options;

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

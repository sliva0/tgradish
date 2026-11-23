//! Shared option value types, option layering, validation, and CLI flag
//! formatting used by [`webm::Options`](crate::webm::Options),
//! [`webp::Options`](crate::webp::Options) and
//! [`tgs::Options`](crate::tgs::Options).
//!
//! Every option field is `Option<T>` so options can be layered in order:
//! built-in defaults, then a preset, then command-line flags. Unset fields
//! receive defaults during planning, some derived from the input video.

use serde::{Deserialize, Serialize};

use crate::presets::{Format, FormatOptions};
use crate::telegram::Target;
use crate::webm::CRFS;
use crate::{tgs, webm, webp};

/// How the input is fitted into the target dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Resize {
    /// Keep aspect ratio and scale so the longer side matches the target.
    /// Default for stickers.
    Contain,
    /// Scale like `contain`, then fill remaining space with transparency.
    /// Default for emoji (which must be exactly square) and TGS.
    Pad,
    /// Keep aspect ratio, fill the target box and crop the excess.
    Crop,
    /// Ignore aspect ratio and stretch to fill the target box.
    Stretch,
}

/// How pixels are scaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Scaling {
    /// Use sharp scaling for pixel art upscaled 2x or more; smooth otherwise.
    Auto,
    /// Smooth interpolation between neighbouring pixels.
    Smooth,
    /// Nearest-neighbour block scaling with anti-aliased edges. Keeps pixel
    /// art crisp at any size.
    Sharp,
    /// Scale by whole-number multiples with transparent margins filling the
    /// remaining space.
    // read as `pixel-perfect` too, as it was once named
    #[serde(alias = "pixel-perfect")]
    Integer,
}

/// Which parameter to adjust to fit within Telegram's file size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Fit {
    /// Fit bitrate first; if nothing fits, or the best that fits looks poor in
    /// a way more bits per frame could mend, try lower frame rates and keep
    /// the encode most like the input (by SSIM). Slowest, best looking.
    Auto,
    /// Fit bitrate at the configured frame rate.
    Bitrate,
    /// Fit constant quality (CRF).
    Crf,
    /// Fit frame rate at constant quality.
    Fps,
    /// Fit duration at constant quality.
    Length,
    /// Encode once with the given settings, even if the file exceeds the limit.
    Off,
}

/// Unit of a [`Range`] searched by [`Fit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FitUnit {
    /// Bitrate in kbit/s.
    Kbps,
    /// libvpx constant quality (CRF).
    Crf,
    /// Frames per second.
    Fps,
    /// Duration in seconds.
    Seconds,
}

impl Fit {
    /// Returns the unit of a fit range for this strategy (`Fit::Off` uses
    /// bitrates because single-attempt encoding targets a bitrate by default).
    pub fn unit(self) -> FitUnit {
        match self {
            Fit::Auto | Fit::Bitrate | Fit::Off => FitUnit::Kbps,
            Fit::Crf => FitUnit::Crf,
            Fit::Fps => FitUnit::Fps,
            Fit::Length => FitUnit::Seconds,
        }
    }

    /// Valid bounds for a fit range under this strategy (bitrate >= 1 kbit/s,
    /// libvpx CRF range, 1..=60 fps, or duration > 0 s, which planning also
    /// clamps to the output duration).
    pub fn range_limits(self) -> std::ops::RangeInclusive<f64> {
        match self.unit() {
            FitUnit::Kbps => 1.0..=f64::INFINITY,
            FitUnit::Crf => f64::from(*CRFS.start())..=f64::from(*CRFS.end()),
            FitUnit::Fps => 1.0..=crate::telegram::MAX_FPS,
            FitUnit::Seconds => f64::MIN_POSITIVE..=f64::INFINITY,
        }
    }
}

/// How hard the encoder works. Lower effort is faster, but produces lower
/// quality at the same size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Effort {
    /// `-cpu-used 4`.
    Fast,
    /// `-cpu-used 2`.
    Balanced,
    /// `-cpu-used 0` (slowest, highest quality).
    Best,
}

impl Effort {
    /// libvpx-vp9 `-cpu-used` value for the "good" deadline.
    pub fn cpu_used(self) -> u8 {
        match self {
            Effort::Fast => 4,
            Effort::Balanced => 2,
            Effort::Best => 0,
        }
    }
}

/// When to spoof container duration metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Spoof {
    /// Spoof only when the video exceeds Telegram's 3-second limit.
    Auto,
    /// Always spoof, even for videos under 3 seconds.
    Always,
    /// Never spoof; trim videos longer than 3 seconds instead.
    Never,
}

/// A `[min, max]` search range for the value tuned by [`Fit`], in its
/// [`FitUnit`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Range {
    /// Lower bound.
    pub min: f64,
    /// Upper bound (must be at least `min`).
    pub max: f64,
}

impl std::str::FromStr for Range {
    type Err = String;

    /// Parses `MIN..MAX`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (min, max) = s.split_once("..").ok_or("expected MIN..MAX (for example, 100..400)")?;
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

/// Region of the input to keep, in rotated source pixels from the top-left
/// corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    /// Left edge offset in pixels.
    pub x: u32,
    /// Top edge offset in pixels.
    pub y: u32,
    /// Width in pixels (at least 1).
    pub width: u32,
    /// Height in pixels (at least 1).
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
            let reason = format!("{self} is outside the {width}×{height} input");
            Err(crate::Error::invalid("crop", reason))
        }
    }
}

impl std::str::FromStr for Crop {
    type Err = String;

    /// Parses `WIDTHxHEIGHT+X+Y`, or `WIDTHxHEIGHT` for the top left.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("{s:?}: expected WIDTHxHEIGHT+X+Y (for example, 640x360+100+50)");
        let (size, offset) = s.split_once('+').unwrap_or((s, "0+0"));
        let (width, height) = size.split_once('x').ok_or_else(invalid)?;
        let (x, y) = offset.split_once('+').ok_or_else(invalid)?;
        let number = |v: &str| v.trim().parse::<u32>().map_err(|_| invalid());
        let crop =
            Crop { x: number(x)?, y: number(y)?, width: number(width)?, height: number(height)? };
        if crop.width == 0 || crop.height == 0 {
            return Err(format!("{s:?}: crop must be at least 1x1"));
        }
        Ok(crop)
    }
}

impl std::fmt::Display for Crop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}+{}+{}", self.width, self.height, self.x, self.y)
    }
}

/// Sprite sheet grid of `columns` by `rows` equal cells, read row by row.
/// Formatted as `COLUMNSxROWS` (e.g. `4x2`, case-insensitive `x`, optional
/// spaces around numbers); parsing rejects anything else, including a side
/// of 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sheet {
    /// Number of columns.
    pub columns: u32,
    /// Number of rows.
    pub rows: u32,
}

impl std::str::FromStr for Sheet {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("{s:?}: expected COLUMNSxROWS (for example, 4x2)");
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

/// Exact integer scale factor aligning input and output pixels: `N` output
/// pixels per input pixel (`2`) or `1/N` (`1/2`, or a decimal like `0.5`).
/// Parsing rejects anything else (including 0), except an empty string, which
/// parses as [`IntegerScale::NONE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IntegerScale {
    /// Output pixels per input pixel (1 when downscaling; 0 only in
    /// [`IntegerScale::NONE`]).
    pub up: u32,
    /// Input pixels per output pixel (1 when upscaling; 0 only in
    /// [`IntegerScale::NONE`]).
    pub down: u32,
}

impl IntegerScale {
    /// Sentinel value that clears an inherited `integer-scale`: options layered
    /// over a preset or config cannot otherwise clear a scale below because
    /// `None` keeps it. Written as an empty string (`--integer-scale=`,
    /// `integer-scale = ""`, `"integer-scale": ""`). `merged` strips it so
    /// presets, the CLI, and the GUI never pass it on, and `integer_scale()`
    /// ignores it if unmerged.
    pub const NONE: IntegerScale = IntegerScale { up: 0, down: 0 };

    /// Maximum supported factor in either direction (1 pixel to a 512 px
    /// sticker).
    pub const MAX: u32 = 512;

    /// Whether both `up` and `down` are non-zero: planning refuses a scale
    /// that is not, and [`IntegerScale::NONE`] is not one.
    pub fn is_valid(self) -> bool {
        self.up > 0 && self.down > 0
    }

    /// Scales `size` by this factor if it produces an exact integer result.
    pub fn apply(self, size: u32) -> Option<u32> {
        if !self.is_valid() || !size.is_multiple_of(self.down) {
            return None;
        }
        (size / self.down).checked_mul(self.up)
    }

    /// Returns the largest input dimension that scales to at most `size` output
    /// pixels (0 if the scale is invalid).
    pub fn most_input(self, size: u32) -> u32 {
        size.checked_div(self.up).map_or(0, |whole| whole.saturating_mul(self.down))
    }
}

impl std::str::FromStr for IntegerScale {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || {
            format!(
                "{s:?}: expected a whole number (such as 2) or 1/N (such as 1/2), up to {}",
                IntegerScale::MAX
            )
        };
        if s.trim().is_empty() {
            return Ok(IntegerScale::NONE);
        }
        let text = s.trim().trim_end_matches(['x', '×']);
        let whole = |v: &str| {
            v.trim().parse::<u32>().ok().filter(|&n| (1..=IntegerScale::MAX).contains(&n))
        };
        if let Some((one, n)) = text.split_once('/') {
            return match (whole(one), whole(n)) {
                (Some(1), Some(n)) => Ok(IntegerScale { up: 1, down: n }),
                _ => Err(invalid()),
            };
        }
        if let Some(n) = whole(text) {
            return Ok(IntegerScale { up: n, down: 1 });
        }
        // a decimal: whole, or one over a whole number
        let value: f64 = text.parse().map_err(|_| invalid())?;
        if !(value > 0.0 && value.is_finite()) {
            return Err(invalid());
        }
        let down = (1.0 / value).round();
        if value < 1.0 && down <= f64::from(IntegerScale::MAX) && (1.0 / down - value).abs() < 1e-3
        {
            return Ok(IntegerScale { up: 1, down: down as u32 });
        }
        Err(invalid())
    }
}

impl std::fmt::Display for IntegerScale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == IntegerScale::NONE {
            return Ok(());
        }
        match self.down {
            1 => write!(f, "{}", self.up),
            down => write!(f, "{}/{down}", self.up),
        }
    }
}

impl TryFrom<String> for IntegerScale {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<IntegerScale> for String {
    fn from(value: IntegerScale) -> String {
        value.to_string()
    }
}

/// Generates `overlaid(&self, overlay)`, `keys()` and `sets()` for a struct
/// whose fields are all `Option<T>`. Destructuring `overlay` guarantees every
/// new field must be added here; list them in field order, which `keys()`
/// keeps.
macro_rules! layered {
    ($options:ident { $($field:ident),* $(,)? }) => {
        impl $options {
            /// Every option's key, as configs, JSON and flags name it, in
            /// field order.
            pub fn keys() -> impl Iterator<Item = String> {
                // as serde's kebab-case renaming names each field
                [$(stringify!($field)),*].into_iter().map(|field| field.replace('_', "-"))
            }

            /// Whether the option `key`, as [`Self::keys`] names it, is
            /// set.
            pub fn sets(&self, key: &str) -> bool {
                let field = key.replace('-', "_");
                $(if field == stringify!($field) {
                    return self.$field.is_some();
                })*
                false
            }

            /// Copies every `Some` field from `overlay` onto `self`.
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

/// What a conversion takes of one input and what it makes of it: the
/// target, the part of the input, what of an Aseprite file or a sprite sheet
/// is read, and the title. Each file has its own, so presets set none of
/// these. `None` uses the default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Selection {
    /// Target type: sticker or emoji. Default: sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Aseprite tag to read. Default: all frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Aseprite layers to show by name (including hidden layers). Default:
    /// layers visible in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_layers: Option<Vec<String>>,
    /// Aseprite layers to hide by name; hiding a group hides its children.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_layers: Option<Vec<String>>,
    /// Region of the input to keep, in source pixels. Default: full frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// Start offset in seconds; for WebP, the time of the frame to make the
    /// picture of. Default: 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Maximum duration in seconds to read from `start` (not for WebP).
    /// Default: full remaining duration (for WebM, or 3 seconds of output
    /// when spoofing is off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Title stored in the output: WebM and WebP metadata, or a `.tgs`'s
    /// Lottie name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Treat the input as a sprite sheet of `COLUMNSxROWS` equal cells, read
    /// row by row, for example `4x2` (`.tgs` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<Sheet>,
    /// Number of frames to read from a sprite sheet (`.tgs` only). Default:
    /// all cells except trailing empty ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet_frames: Option<u32>,
}

layered!(Selection {
    target,
    tag,
    show_layers,
    hide_layers,
    crop,
    start,
    length,
    title,
    sheet,
    sheet_frames,
});

impl Selection {
    /// Overlays all set fields from `overlay` onto `self`.
    pub fn merged(self, overlay: &Selection) -> Selection {
        self.overlaid(overlay)
    }

    /// `self` without what `format` doesn't take: a sprite sheet is read
    /// only for `.tgs`, and a picture has no length.
    pub fn for_format(mut self, format: Format) -> Selection {
        if format != Format::Tgs {
            (self.sheet, self.sheet_frames) = (None, None);
        }
        if format == Format::Webp {
            self.length = None;
        }
        self
    }

    /// The keys of those [`Selection::for_format`] keeps for `format`, in
    /// field order.
    pub fn keys_for(format: Format) -> impl Iterator<Item = String> {
        Selection::keys().filter(move |key| match key.as_str() {
            "sheet" | "sheet-frames" => format == Format::Tgs,
            "length" => format != Format::Webp,
            _ => true,
        })
    }

    /// `options` with this selection's values written over the fields of
    /// the same names they still have.
    // until the options lose those fields, which then are this alone's
    #[doc(hidden)]
    pub fn into_options(self, options: FormatOptions) -> FormatOptions {
        let Selection { target, tag, show_layers, hide_layers, crop, start, length, title, .. } =
            self;
        // as overlaid, which merging isn't: that clears a scale of none
        match options {
            FormatOptions::Webm(options) => FormatOptions::Webm(webm::Options {
                target: target.or(options.target),
                tag: tag.or(options.tag),
                show_layers: show_layers.or(options.show_layers),
                hide_layers: hide_layers.or(options.hide_layers),
                crop: crop.or(options.crop),
                start: start.or(options.start),
                length: length.or(options.length),
                title: title.or(options.title),
                ..options
            }),
            FormatOptions::Webp(options) => FormatOptions::Webp(webp::Options {
                target: target.or(options.target),
                tag: tag.or(options.tag),
                show_layers: show_layers.or(options.show_layers),
                hide_layers: hide_layers.or(options.hide_layers),
                crop: crop.or(options.crop),
                start: start.or(options.start),
                title: title.or(options.title),
                ..options
            }),
            // which holds none of them
            FormatOptions::Tgs(options) => FormatOptions::Tgs(options),
        }
    }
}

/// Everything a conversion of one file gets: what is the file's own, and the
/// options of a format, which presets hold too. Written and read as one flat
/// object, as `--options-json`, `--json` and the GUI's session have it:
/// `{"crop": …, "crf": 30}`.
// serde calls flatten with deny_unknown_fields unsupported, though it
// refuses what neither part takes; tests pin that, and the old names too
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings<O> {
    /// What is the file's own.
    #[serde(flatten)]
    pub selection: Selection,
    /// The format's options.
    #[serde(flatten)]
    pub options: O,
}

impl<O> Settings<O> {
    /// The same selection, with `options` made of these ones by `f`.
    pub fn map<P>(self, f: impl FnOnce(O) -> P) -> Settings<P> {
        Settings { selection: self.selection, options: f(self.options) }
    }
}

impl Settings<FormatOptions> {
    /// The format the options are for.
    pub fn format(&self) -> Format {
        self.options.format()
    }

    /// The target chosen, or the default, [`Target::Sticker`].
    pub fn target(&self) -> Target {
        self.selection.target.unwrap_or_default()
    }

    /// Reads `value`, a JSON object as `--options-json` takes it, as
    /// `format`'s settings: its options and what of the file's own
    /// `format` takes.
    pub fn from_json(format: Format, value: serde_json::Value) -> serde_json::Result<Self> {
        Ok(match format {
            Format::Webm => {
                serde_json::from_value::<Settings<webm::Options>>(value)?.map(FormatOptions::Webm)
            }
            Format::Webp => {
                serde_json::from_value::<Settings<webp::Options>>(value)?.map(FormatOptions::Webp)
            }
            Format::Tgs => {
                serde_json::from_value::<Settings<tgs::Options>>(value)?.map(FormatOptions::Tgs)
            }
        })
    }
}

/// Legacy option keys kept as aliases in configs, JSON and CLI flags:
/// `(old, current)`.
pub const RENAMED: [(&str, &str); 2] = [("speed", "effort"), ("exact-scale", "integer-scale")];

/// Maps a legacy option key to its current key (see [`RENAMED`]), or
/// returns `key`.
pub fn current_key(key: &str) -> &str {
    RENAMED.iter().find(|(old, _)| *old == key).map_or(key, |(_, new)| new)
}

/// Returns the keys of all fields set in `options`, in struct field order.
pub fn set_keys(options: &impl Serialize) -> Vec<String> {
    match serde_json::to_value(options) {
        Ok(serde_json::Value::Object(fields)) => fields.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Formats set fields of `options` as `tgradish convert` CLI flags.
/// Values use `--name=value` so values starting with `-` (such as titles or
/// layer names) are never mistaken for flags. Booleans use `--name` and
/// `--no-name`.
pub fn flags(options: &impl Serialize) -> Vec<String> {
    let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(options) else {
        return Vec::new();
    };
    // Round to 9 decimal places (far finer than any cut between frames) so
    // float arithmetic noise does not print as 0.07999999999999996.
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
            (_, Value::Bool(false)) => flags.push(format!("--no-{name}")),
            (_, Value::String(text)) => flags.push(set(&text)),
            (_, value @ Value::Number(_)) => flags.push(set(&number(&value))),
            _ => {}
        }
    }
    flags
}

/// Allowed playback speed multiplier range for both WebM and `.tgs`.
pub const TEMPOS: std::ops::RangeInclusive<f64> = tgradish_tgs::normalise::TEMPOS;

/// Validates a numeric option value independent of any input file. Used by
/// front-ends to check values as they are typed, returning the same error
/// wording as conversions (or `None` if valid or not a number checked by
/// [`check`]).
pub fn wrong_value(key: &str, value: f64) -> Option<String> {
    let max_seconds = crate::telegram::MAX_SECONDS;
    let max_fps = crate::telegram::MAX_FPS;
    let ok = match key {
        "start" => value >= 0.0 && value.is_finite(),
        "length" => value > 0.0 && value.is_finite(),
        "tempo" => TEMPOS.contains(&value),
        "fps" => value > 0.0 && value <= max_fps,
        "bitrate" => value >= 1.0 && value.is_finite(),
        "crf" => value >= f64::from(*CRFS.start()) && value <= f64::from(*CRFS.end()),
        "attempts" => value >= 1.0 && value <= f64::from(crate::webm::MAX_ATTEMPTS),
        "fake-duration" => value > 0.0 && value <= max_seconds,
        _ => true,
    };
    if ok {
        return None;
    }
    Some(match key {
        "start" => "must be 0 seconds or greater".into(),
        "length" => "must be greater than 0 seconds".into(),
        "tempo" => format!("must be from {} to {}", TEMPOS.start(), TEMPOS.end()),
        "fps" => format!("must be greater than 0 and at most {max_fps}"),
        "bitrate" => "must be at least 1 kbit/s".into(),
        "crf" => format!("must be from {} to {}", CRFS.start(), CRFS.end()),
        "attempts" => format!("must be from 1 to {}", crate::webm::MAX_ATTEMPTS),
        _ => format!("must be greater than 0 and at most {max_seconds} seconds"),
    })
}

/// Options structs validated by [`check`]. Inspects fields directly so
/// non-finite floats (which JSON would drop as `null`) are caught.
pub trait Checked {
    /// Set numeric options recognised by [`wrong_value`], paired with their
    /// keys.
    fn numbers(&self) -> Vec<(&'static str, f64)>;
    /// Fit range if set.
    fn fit_range(&self) -> Option<Range> {
        None
    }
}

impl Checked for crate::webm::Options {
    fn numbers(&self) -> Vec<(&'static str, f64)> {
        let numbers = [
            ("start", self.start),
            ("length", self.length),
            ("tempo", self.tempo),
            ("fps", self.fps),
            ("bitrate", self.bitrate),
            ("crf", self.crf.map(f64::from)),
            ("attempts", self.attempts.map(f64::from)),
            ("fake-duration", self.fake_duration),
        ];
        numbers.into_iter().filter_map(|(key, value)| Some((key, value?))).collect()
    }

    fn fit_range(&self) -> Option<Range> {
        self.fit_range
    }
}

impl Checked for crate::webp::Options {
    fn numbers(&self) -> Vec<(&'static str, f64)> {
        self.start.map(|start| ("start", start)).into_iter().collect()
    }
}

impl Checked for crate::tgs::Options {
    fn numbers(&self) -> Vec<(&'static str, f64)> {
        let numbers = [("tempo", self.tempo), ("fps", self.fps)];
        numbers.into_iter().filter_map(|(key, value)| Some((key, value?))).collect()
    }
}

impl Checked for Selection {
    fn numbers(&self) -> Vec<(&'static str, f64)> {
        let numbers = [("start", self.start), ("length", self.length)];
        numbers.into_iter().filter_map(|(key, value)| Some((key, value?))).collect()
    }
}

impl Checked for FormatOptions {
    fn numbers(&self) -> Vec<(&'static str, f64)> {
        match self {
            FormatOptions::Webm(options) => options.numbers(),
            FormatOptions::Tgs(options) => options.numbers(),
            FormatOptions::Webp(options) => options.numbers(),
        }
    }

    fn fit_range(&self) -> Option<Range> {
        match self {
            FormatOptions::Webm(options) => options.fit_range(),
            FormatOptions::Tgs(options) => options.fit_range(),
            FormatOptions::Webp(options) => options.fit_range(),
        }
    }
}

/// The file's own first, as its times come first among the options.
impl<O: Checked> Checked for Settings<O> {
    fn numbers(&self) -> Vec<(&'static str, f64)> {
        let mut numbers = self.selection.numbers();
        numbers.extend(self.options.numbers());
        numbers
    }

    fn fit_range(&self) -> Option<Range> {
        self.options.fit_range()
    }
}

/// Validates all input-independent values in `options` (numeric bounds,
/// finiteness, and `fit-range` ordering). Input-dependent checks happen during
/// conversion.
pub fn check(options: &impl Checked) -> crate::Result<()> {
    match first_wrong(options) {
        Some((option, reason)) => Err(crate::Error::invalid(option, reason)),
        None => Ok(()),
    }
}

/// The first value [`check`] finds wrong in `options`, by its key, and why.
pub(crate) fn first_wrong(options: &impl Checked) -> Option<(&'static str, String)> {
    for (key, value) in options.numbers() {
        if let Some(reason) = wrong_value(key, value) {
            return Some((key, reason));
        }
    }
    let range = options.fit_range()?;
    if !(range.min.is_finite() && range.max.is_finite()) {
        return Some(("fit-range", format!("{range} must have finite ends")));
    }
    if range.min > range.max {
        return Some(("fit-range", format!("{range} must have MIN <= MAX")));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::Target;
    use crate::webm::Options;

    #[test]
    fn checks_values_whatever_the_input() {
        let invalid = |checked: crate::Result<()>| match checked {
            Err(crate::Error::InvalidOption { option, reason }) => format!("{option} {reason}"),
            other => panic!("{other:?}"),
        };
        let webm = Options { tempo: Some(50.0), ..Default::default() };
        assert_eq!(invalid(check(&webm)), "tempo must be from 0.1 to 10");
        let tgs = crate::tgs::Options { fps: Some(0.0), ..Default::default() };
        assert_eq!(invalid(check(&tgs)), "fps must be greater than 0 and at most 60");
        let range = Options { fit_range: Some(Range { min: 4.0, max: 1.0 }), ..Default::default() };
        assert_eq!(invalid(check(&range)), "fit-range 4..1 must have MIN <= MAX");
        let length = Options { length: Some(0.0), ..Default::default() };
        assert_eq!(invalid(check(&length)), "length must be greater than 0 seconds");
        let fine =
            Options { start: Some(0.0), crf: Some(63), attempts: Some(1), ..Default::default() };
        assert!(check(&fine).is_ok());
        assert_eq!(wrong_value("start", f64::NAN).unwrap(), "must be 0 seconds or greater");
        assert!(wrong_value("title", -1.0).is_none());
    }

    /// The option `check` finds wrong in `options`.
    fn wrong_option(options: &impl Checked) -> &'static str {
        match check(options) {
            Err(crate::Error::InvalidOption { option, .. }) => option,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn checks_numbers_that_arent_finite() {
        type Set<T> = fn(&mut T, f64);
        let webm: [(&str, Set<Options>); 8] = [
            ("start", |options, n| options.start = Some(n)),
            ("length", |options, n| options.length = Some(n)),
            ("tempo", |options, n| options.tempo = Some(n)),
            ("fps", |options, n| options.fps = Some(n)),
            ("bitrate", |options, n| options.bitrate = Some(n)),
            ("fake-duration", |options, n| options.fake_duration = Some(n)),
            ("fit-range", |options, n| options.fit_range = Some(Range { min: n, max: 400.0 })),
            ("fit-range", |options, n| options.fit_range = Some(Range { min: 100.0, max: n })),
        ];
        let tgs: [(&str, Set<Settings<crate::tgs::Options>>); 4] = [
            ("start", |settings, n| settings.selection.start = Some(n)),
            ("length", |settings, n| settings.selection.length = Some(n)),
            ("tempo", |settings, n| settings.options.tempo = Some(n)),
            ("fps", |settings, n| settings.options.fps = Some(n)),
        ];
        for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for (key, set) in webm {
                let mut options = Options::default();
                set(&mut options, n);
                assert_eq!(wrong_option(&options), key, "{key} {n}");
            }
            for (key, set) in tgs {
                let mut options = Settings::default();
                set(&mut options, n);
                assert_eq!(wrong_option(&options), key, "{key} {n}");
            }
        }
        let range =
            Options { fit_range: Some(Range { min: 1.0, max: f64::NAN }), ..Default::default() };
        let Err(crate::Error::InvalidOption { reason, .. }) = check(&range) else { panic!() };
        assert_eq!(reason, "1..NaN must have finite ends");
    }

    #[test]
    fn checks_numbers_a_preset_sets_that_arent_finite() {
        use crate::presets::Presets;
        let dir = tempfile::tempdir().unwrap();
        let mut presets = Vec::new();
        // each a preset can set: a start and a length are one file's
        for value in ["nan", "+nan", "-nan", "inf", "+inf", "-inf"] {
            for key in ["tempo", "fps", "bitrate", "fake-duration"] {
                presets.push((key, "webm", format!("{key} = {value}")));
            }
            for key in ["tempo", "fps"] {
                presets.push((key, "tgs", format!("{key} = {value}")));
            }
            for range in [
                format!("{{ min = {value}, max = 400.0 }}"),
                format!("{{ min = 1.0, max = {value} }}"),
            ] {
                presets.push(("fit-range", "webm", format!("fit-range = {range}")));
            }
        }
        for (number, (_, table, line)) in presets.iter().enumerate() {
            let text = format!("[{table}]\n{line}\n");
            std::fs::write(dir.path().join(format!("p{number}.toml")), text).unwrap();
        }
        let loaded = Presets::load(Some(dir.path())).unwrap();
        for (number, (key, table, line)) in presets.iter().enumerate() {
            let name = format!("p{number}");
            let option = match *table {
                "webm" => wrong_option(&loaded.webm(&name).unwrap()),
                _ => wrong_option(&loaded.tgs(&name).unwrap()),
            };
            assert_eq!(option, *key, "{line}");
        }
    }

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
                "--no-watermark",
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
            assert!(text.parse::<IntegerScale>().is_err(), "{text}");
            let preset = format!("integer-scale = '{text}'");
            assert!(toml::from_str::<Options>(&preset).is_err(), "{text}");
        }
        // as code can still make one, and none is no scale either
        let zeros = [IntegerScale { up: 0, down: 1 }, IntegerScale { up: 1, down: 0 }];
        for zero in zeros.into_iter().chain([IntegerScale::NONE]) {
            assert!(!zero.is_valid());
            assert_eq!((zero.apply(4), zero.most_input(512)), (None, 0), "{zero:?}");
        }
        let half: IntegerScale = "1/2".parse().unwrap();
        assert!(half.is_valid() && half.apply(4) == Some(2) && half.most_input(512) == 1024);
        // nothing is none, which planning never sees
        assert_eq!("".parse(), Ok(IntegerScale::NONE));
        assert!(!IntegerScale::NONE.is_valid() && IntegerScale::NONE.to_string().is_empty());
    }

    /// `--integer-scale=` clears a preset's scale, as flags write it.
    #[test]
    fn clears_an_integer_scale() {
        let preset = Options { integer_scale: "2".parse().ok(), ..Default::default() };
        let none = Options { integer_scale: Some(IntegerScale::NONE), ..Default::default() };
        assert_eq!(preset.clone().merged(&none).integer_scale, None);
        assert_eq!(Options::default().merged(&none).integer_scale, None);
        assert_eq!(none.clone().merged(&Options::default()).integer_scale, None);
        assert_eq!(flags(&none), ["--integer-scale="]);
        let read: Options = toml::from_str("integer-scale = ''").unwrap();
        assert_eq!(read, none);
        // and in JSON, as --options-json takes it and --json prints it
        let json = serde_json::json!({ "integer-scale": "" });
        assert_eq!(serde_json::from_value::<Options>(json.clone()).unwrap(), none);
        assert_eq!(serde_json::to_value(&none).unwrap(), json);
        // what isn't merged scales as none does, not as a scale
        assert_eq!(none.integer_scale(), None);
        assert!(none.auto_scaling());
        let sizes = |options: &Options| {
            crate::picture::picture_sizes(crate::presets::Format::Webm, options, (8, 8)).unwrap()
        };
        assert_eq!(sizes(&none), sizes(&Options::default()));
    }

    /// What was once named otherwise reads by its old name too, in a
    /// preset's TOML and in JSON, as `--options-json` and the GUI's
    /// session have it, but is written by its name now.
    #[test]
    fn reads_options_by_their_old_names() {
        let toml: Options =
            toml::from_str("speed = 'best'\nscaling = 'pixel-perfect'\nexact-scale = '2'").unwrap();
        assert_eq!(toml.effort, Some(Effort::Best));
        assert_eq!(toml.scaling, Some(Scaling::Integer));
        assert_eq!(toml.integer_scale, Some(IntegerScale { up: 2, down: 1 }));
        let json =
            serde_json::json!({ "speed": "fast", "scaling": "pixel-perfect", "exact-scale": "" });
        let json: Options = serde_json::from_value(json).unwrap();
        assert_eq!(json.effort, Some(Effort::Fast));
        assert_eq!(json.scaling, Some(Scaling::Integer));
        assert_eq!(json.integer_scale, Some(IntegerScale::NONE));
        assert_eq!(serde_json::to_value(Scaling::Integer).unwrap(), "integer");
        let tgs: crate::tgs::Options = toml::from_str("speed = 'fast'").unwrap();
        assert_eq!(tgs.effort, Some(Effort::Fast));
        assert_eq!(set_keys(&json), ["scaling", "integer-scale", "effort"]);
        assert_eq!(flags(&tgs), ["--effort=fast"]);
        for (old, new) in RENAMED {
            assert_eq!((current_key(old), current_key(new)), (new, new));
        }
        assert_eq!(current_key("crf"), "crf");
        // and what a format uses
        use crate::presets::Format;
        assert!(Format::Webm.uses("speed") && Format::Tgs.uses("speed"));
        assert!(!Format::Webp.uses("speed"));
        assert!(Format::Webp.uses("exact-scale") && !Format::Tgs.uses("exact-scale"));
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

    /// WebM's options, every one set.
    fn every_webm_option() -> Options {
        Options {
            target: Some(Target::Emoji),
            tag: Some("idle".into()),
            show_layers: Some(vec!["Hat".into()]),
            hide_layers: Some(vec!["Sketch".into(), "Guides".into()]),
            crop: Some(Crop { x: 1, y: 2, width: 30, height: 40 }),
            resize: Some(Resize::Pad),
            scaling: Some(Scaling::Integer),
            integer_scale: Some(IntegerScale { up: 1, down: 2 }),
            fit: Some(Fit::Crf),
            attempts: Some(5),
            fit_range: Some(Range { min: 4.0, max: 40.5 }),
            start: Some(1.25),
            length: Some(2.0),
            tempo: Some(1.5),
            fps: Some(24.0),
            bitrate: Some(600.0),
            crf: Some(30),
            effort: Some(Effort::Best),
            lossless: Some(false),
            spoof: Some(Spoof::Never),
            fake_duration: Some(0.5),
            title: Some("pig".into()),
            watermark: Some(false),
            encoder_options: Some([("tune-content".into(), "screen".into())].into()),
            ffmpeg_args: Some(vec!["-x".into(), "a b".into()]),
        }
    }

    /// `.tgs`'s options, every one set.
    fn every_tgs_option() -> crate::tgs::Options {
        use tgradish_tgs::reduce::{Compromise, Kind};
        crate::tgs::Options {
            tempo: Some(0.5),
            effort: Some(Effort::Fast),
            exact: Some(false),
            reductions: Some(vec![Kind::MergeColours, Kind::DropFrames]),
            compromise: Some(Compromise::Detail),
            resize: Some(Resize::Stretch),
            keep_canvas: Some(true),
            pixel_scale: Some(2),
            fps: Some(12.5),
            watermark: Some(true),
        }
    }

    /// Every option as JSON and TOML spell and order it: as `--json`,
    /// `preset show` and the GUI's session write it, and as presets and
    /// `--options-json` read it. A change here is one to those files.
    #[test]
    fn writes_every_option_as_files_hold_it() {
        let webm = every_webm_option();
        let json = concat!(
            r#"{"target":"emoji","tag":"idle","show-layers":["Hat"],"#,
            r#""hide-layers":["Sketch","Guides"],"#,
            r#""crop":{"x":1,"y":2,"width":30,"height":40},"#,
            r#""resize":"pad","scaling":"integer","integer-scale":"1/2","fit":"crf","#,
            r#""attempts":5,"fit-range":{"min":4.0,"max":40.5},"start":1.25,"#,
            r#""length":2.0,"tempo":1.5,"fps":24.0,"bitrate":600.0,"crf":30,"#,
            r#""effort":"best","lossless":false,"spoof":"never","fake-duration":0.5,"#,
            r#""title":"pig","watermark":false,"#,
            r#""encoder-options":{"tune-content":"screen"},"ffmpeg-args":["-x","a b"]}"#,
        );
        assert_eq!(serde_json::to_string(&webm).unwrap(), json);
        assert_eq!(serde_json::from_str::<Options>(json).unwrap(), webm);
        // tables last, as TOML puts them
        let toml = "\
target = \"emoji\"
tag = \"idle\"
show-layers = [\"Hat\"]
hide-layers = [\"Sketch\", \"Guides\"]
resize = \"pad\"
scaling = \"integer\"
integer-scale = \"1/2\"
fit = \"crf\"
attempts = 5
start = 1.25
length = 2.0
tempo = 1.5
fps = 24.0
bitrate = 600.0
crf = 30
effort = \"best\"
lossless = false
spoof = \"never\"
fake-duration = 0.5
title = \"pig\"
watermark = false
ffmpeg-args = [\"-x\", \"a b\"]

[crop]
x = 1
y = 2
width = 30
height = 40

[fit-range]
min = 4.0
max = 40.5

[encoder-options]
tune-content = \"screen\"
";
        assert_eq!(toml::to_string(&webm).unwrap(), toml);
        assert_eq!(toml::from_str::<Options>(toml).unwrap(), webm);

        // as a preset's table holds them, without one file's own
        let tgs = every_tgs_option();
        let json = concat!(
            r#"{"tempo":0.5,"effort":"fast","exact":false,"#,
            r#""reductions":["merge-colours","drop-frames"],"compromise":"detail","#,
            r#""resize":"stretch","keep-canvas":true,"pixel-scale":2,"fps":12.5,"#,
            r#""watermark":true}"#,
        );
        assert_eq!(serde_json::to_string(&tgs).unwrap(), json);
        assert_eq!(serde_json::from_str::<crate::tgs::Options>(json).unwrap(), tgs);
        let toml = "\
tempo = 0.5
effort = \"fast\"
exact = false
reductions = [\"merge-colours\", \"drop-frames\"]
compromise = \"detail\"
resize = \"stretch\"
keep-canvas = true
pixel-scale = 2
fps = 12.5
watermark = true
";
        assert_eq!(toml::to_string(&tgs).unwrap(), toml);
        assert_eq!(toml::from_str::<crate::tgs::Options>(toml).unwrap(), tgs);
    }

    /// Everything a `.tgs` gets as JSON spells and orders it, flat, the
    /// file's own first: as `--options-json` and the GUI's session have
    /// it. A change here is one to those files.
    #[test]
    fn writes_every_tgs_setting_as_files_hold_it() {
        let tgs = Settings { selection: every_selection(), options: every_tgs_option() };
        let json = concat!(
            r#"{"target":"emoji","tag":"idle","show-layers":["Hat"],"#,
            r#""hide-layers":["Sketch","Guides"],"#,
            r#""crop":{"x":1,"y":2,"width":30,"height":40},"start":1.25,"length":2.0,"#,
            r#""title":"pig","sheet":"4x2","sheet-frames":7,"#,
            r#""tempo":0.5,"effort":"fast","exact":false,"#,
            r#""reductions":["merge-colours","drop-frames"],"compromise":"detail","#,
            r#""resize":"stretch","keep-canvas":true,"pixel-scale":2,"fps":12.5,"#,
            r#""watermark":true}"#,
        );
        assert_eq!(serde_json::to_string(&tgs).unwrap(), json);
        assert_eq!(serde_json::from_str::<Settings<crate::tgs::Options>>(json).unwrap(), tgs);
        let any = Settings::from_json(Format::Tgs, serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(any, tgs.clone().map(FormatOptions::Tgs));
        assert_eq!(serde_json::to_string(&any).unwrap(), json);
    }

    /// Flattening the two parts into one object is what serde calls
    /// unsupported beside `deny_unknown_fields`, so this pins what it
    /// does: a key neither part takes is refused, and the old names read.
    #[test]
    fn reads_settings_as_one_object_and_nothing_else() {
        use serde_json::json;
        let read = |value: serde_json::Value| {
            serde_json::from_value::<Settings<crate::tgs::Options>>(value)
        };
        let settings = read(json!({ "speed": "fast", "sheet": "2x1", "crop": null })).unwrap();
        assert_eq!(settings.options.effort, Some(Effort::Fast));
        assert_eq!(settings.selection.sheet, Some(Sheet { columns: 2, rows: 1 }));
        for unknown in [json!({ "made-up": 1 }), json!({ "sheet": "2x1", "crf": 30 })] {
            let err = read(unknown.clone()).unwrap_err().to_string();
            assert!(err.starts_with("unknown field"), "{unknown}: {err}");
        }
        // a value wrong for its key is refused too
        assert!(read(json!({ "sheet": "two" })).is_err());
        assert!(read(json!({ "fps": "fast" })).is_err());
        // and each key serde reads is one the format uses
        for key in Selection::keys_for(Format::Tgs).chain(crate::tgs::Options::keys()) {
            assert!(read(json!({ &key: null })).is_ok(), "{key}");
            assert!(Format::Tgs.uses(&key), "{key}");
        }
    }

    /// One file's options, every one set.
    fn every_selection() -> Selection {
        Selection {
            target: Some(Target::Emoji),
            tag: Some("idle".into()),
            show_layers: Some(vec!["Hat".into()]),
            hide_layers: Some(vec!["Sketch".into(), "Guides".into()]),
            crop: Some(Crop { x: 1, y: 2, width: 30, height: 40 }),
            start: Some(1.25),
            length: Some(2.0),
            title: Some("pig".into()),
            sheet: Some(Sheet { columns: 4, rows: 2 }),
            sheet_frames: Some(7),
        }
    }

    #[test]
    fn takes_of_one_files_options_what_each_format_reads() {
        let every = every_selection();
        for format in [Format::Webm, Format::Webp, Format::Tgs] {
            let kept = set_keys(&every.clone().for_format(format));
            assert_eq!(kept, Selection::keys_for(format).collect::<Vec<_>>(), "{format:?}");
        }
        let picture: Vec<String> = Selection::keys_for(Format::Webp).collect();
        let read = ["target", "tag", "show-layers", "hide-layers", "crop", "start", "title"];
        assert_eq!(picture, read);
        // each over the last, as options merge
        let later = Selection { start: Some(3.0), ..Selection::default() };
        assert_eq!(every.clone().merged(&later), Selection { start: Some(3.0), ..every.clone() });
        assert!(every.sets("sheet-frames") && !Selection::default().sets("crop"));
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

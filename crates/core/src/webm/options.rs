//! Conversion options for WebM. Static WebP takes those a picture has, as
//! [`crate::webp::Options`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::options::{Crop, Effort, Fit, IntegerScale, Range, Resize, Scaling, Spoof, layered};
use crate::telegram::Target;

/// Settings for one WebM conversion. `None` uses the default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// Target type: sticker or emoji. Default: sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Aseprite tag to export. Default: all frames.
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
    /// How to fit the video into the target dimensions. Default: contain for
    /// stickers, pad for emoji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// Scaling algorithm: smooth, sharp or integer. Default: auto (sharp for
    /// pixel art upscaled 2x or more).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    /// Scale by an exact integer N or 1/N so source and output pixels align.
    /// The crop, scaled, must fit inside the target size; transparent margins
    /// fill the remaining space. Overrides `scaling`, and requires `resize` to
    /// be `contain` or `pad`.
    // read as `exact-scale` too, as it was once named
    #[serde(alias = "exact-scale", skip_serializing_if = "Option::is_none")]
    pub integer_scale: Option<IntegerScale>,
    /// Parameter to adjust to get close to the file size limit (256 KiB for
    /// stickers, 64 KiB for emoji). Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// Maximum encoding attempts during size fitting. Default: 8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// Search range for size fitting, in the units of `fit` (kbit/s, CRF, fps
    /// or seconds). When merging options that change `fit` to a different unit
    /// without specifying a new `fit-range`, the inherited range is cleared.
    /// Default depends on `fit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit_range: Option<Range>,
    /// Start offset in seconds. Default: 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Maximum duration in seconds to read from `start`. Default: full
    /// remaining duration (or 3 seconds of output when spoofing is off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Playback speed multiplier (0.1 to 10; 2 plays twice as fast). Default:
    /// 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<f64>,
    /// Output frame rate (max 60; higher than the input rate at the chosen
    /// tempo only duplicates frames). For image sequences, also sets the input
    /// frame rate. Default: input rate adjusted for tempo (clamped to 1..60),
    /// or 12 for sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Target video bitrate in kbit/s when not fitting bitrate. Default:
    /// estimated from the file size limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<f64>,
    /// Constant quality level, 0 (best) to 63 (worst), used when fitting fps
    /// or length or with `fit = "off"`. Default: 32.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crf: Option<u8>,
    /// Encoder effort: fast, balanced or best (slowest). Default: balanced.
    // read as `speed` too, as it was once named
    #[serde(alias = "speed", skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Enable lossless VP9 encoding (only practical for tiny or static
    /// inputs). Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lossless: Option<bool>,
    /// When to spoof container duration so Telegram accepts videos longer than
    /// 3 seconds. Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoof: Option<Spoof>,
    /// Container duration in seconds written when spoofing. Default: a short
    /// duration accepted by Telegram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fake_duration: Option<f64>,
    /// Title metadata stored in the output file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Write tgradish metadata into the output (writing/muxing app tags and
    /// padding signature). Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
    /// Additional libvpx-vp9 encoder options by name (for example,
    /// `tune-content = "screen"` for flat graphics, `aq-mode = "2"`,
    /// `sharpness = "4"`, `arnr-strength = "3"`, `g = "60"` (keyframe interval)
    /// or `qmax = "50"`). Run `ffmpeg -h encoder=libvpx-vp9` for the full list.
    /// Unknown names fail encoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_options: Option<BTreeMap<String, String>>,
    /// Extra raw ffmpeg arguments, added before the output file. Requires an
    /// external ffmpeg executable (`--ffmpeg-from system` or `--ffmpeg PATH`),
    /// since the built-in ffmpeg has no command line; prefer `encoder-options`
    /// for encoder settings, which work with both backends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ffmpeg_args: Option<Vec<String>>,
}

layered!(Options {
    target,
    tag,
    show_layers,
    hide_layers,
    crop,
    resize,
    scaling,
    integer_scale,
    fit,
    attempts,
    fit_range,
    start,
    length,
    tempo,
    fps,
    bitrate,
    crf,
    effort,
    lossless,
    spoof,
    fake_duration,
    title,
    watermark,
    encoder_options,
    ffmpeg_args,
});

/// Single source of truth for default option values, shared by core and
/// front-ends.
impl Options {
    /// Configured fit strategy, or the default (`Fit::Auto`).
    pub fn fit_or_default(&self) -> Fit {
        self.fit.unwrap_or(Fit::Auto)
    }

    /// Configured encoder effort, or the default (`Effort::Balanced`).
    pub fn effort_or_default(&self) -> Effort {
        self.effort.unwrap_or(Effort::Balanced)
    }

    /// Configured scaling mode, or the default (`Scaling::Auto`).
    pub fn scaling_or_default(&self) -> Scaling {
        self.scaling.unwrap_or(Scaling::Auto)
    }

    /// Configured duration spoofing policy, or the default (`Spoof::Auto`:
    /// spoof only results longer than 3 s).
    pub fn spoof_or_default(&self) -> Spoof {
        self.spoof.unwrap_or(Spoof::Auto)
    }

    /// Configured playback tempo, or the default (`1.0`).
    pub fn tempo_or_default(&self) -> f64 {
        self.tempo.unwrap_or(1.0)
    }

    /// Configured lossless setting, or `false` by default.
    pub fn lossless_or_default(&self) -> bool {
        self.lossless.unwrap_or(false)
    }

    /// Returns true if scaling is automatic and no explicit `integer_scale` is
    /// set.
    pub fn auto_scaling(&self) -> bool {
        self.scaling_or_default() == Scaling::Auto && self.integer_scale().is_none()
    }

    /// Returns the configured [`IntegerScale`], treating
    /// [`IntegerScale::NONE`] as `None`.
    pub fn integer_scale(&self) -> Option<IntegerScale> {
        self.integer_scale.filter(|&scale| scale != IntegerScale::NONE)
    }

    /// Overlays set fields from `overlay` onto `self`. Clears `integer_scale`
    /// when `overlay` sets [`IntegerScale::NONE`], and drops an inherited
    /// `fit_range` if `overlay` changes `fit` to a strategy with a different
    /// unit without providing a new range.
    pub fn merged(self, overlay: &Options) -> Options {
        let unit = self.fit_or_default().unit();
        let mut merged = self.overlaid(overlay);
        if overlay.fit.is_some_and(|fit| fit.unit() != unit) && overlay.fit_range.is_none() {
            merged.fit_range = None;
        }
        if merged.integer_scale == Some(IntegerScale::NONE) {
            merged.integer_scale = None;
        }
        merged
    }
}

/// Returns `options` with any fields that have no effect under `effective`
/// cleared, as [`set_aside`] lists them, so planning neither warns about them
/// nor refuses lossless where it has no effect.
pub fn in_effect(options: Options, effective: &Options) -> Options {
    let aside = set_aside(&options, effective);
    without(options, aside.iter().map(|(key, _)| *key))
}

/// `options` without the values of `keys`, each one [`set_aside`] gives.
pub(crate) fn without<'k>(
    mut options: Options,
    keys: impl IntoIterator<Item = &'k str>,
) -> Options {
    for key in keys {
        match key {
            "attempts" => options.attempts = None,
            "fit-range" => options.fit_range = None,
            "crf" => options.crf = None,
            "lossless" => options.lossless = None,
            "bitrate" => options.bitrate = None,
            "fake-duration" => options.fake_duration = None,
            _ => unreachable!("set_aside gives no {key}"),
        }
    }
    options
}

/// Returns `(key, reason)` for each value set in `options` that has no effect
/// under `effective`; `reason` is a clause naming options by key, e.g.
/// `"fit is off"`. Covers `attempts` and `fit-range` when `fit` is `off`;
/// `crf` or `lossless` where bitrate or CRF is fitted; `bitrate` where a
/// quality is kept or fitting chooses its own; `crf` beside `lossless` or a
/// set `bitrate`; and `fake-duration` when `spoof` is `never`.
pub fn set_aside(options: &Options, effective: &Options) -> Vec<(&'static str, String)> {
    let fit = effective.fit_or_default();
    let fit_is = || {
        let name = serde_json::to_value(fit).ok().and_then(|v| v.as_str().map(str::to_owned));
        format!("fit is {}", name.unwrap_or_default())
    };
    let constant_quality = matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    let lossless = constant_quality && effective.lossless_or_default();
    let mut aside = Vec::new();
    if fit == Fit::Off {
        aside.extend(options.attempts.map(|_| ("attempts", fit_is())));
        aside.extend(options.fit_range.map(|_| ("fit-range", fit_is())));
    }
    let crf = if !constant_quality {
        Some(fit_is())
    } else if lossless {
        Some("lossless is on".to_owned())
    } else if fit == Fit::Off && effective.bitrate.is_some() {
        Some("bitrate is set".to_owned())
    } else {
        None
    };
    if let (Some(_), Some(why)) = (options.crf, &crf) {
        aside.push(("crf", why.clone()));
    }
    if let (Some(_), false) = (options.lossless, constant_quality) {
        aside.push(("lossless", fit_is()));
    }
    // fitting a bitrate starts from the one given; the others don't
    // encode by one, but off does
    let bitrate = if !matches!(fit, Fit::Bitrate | Fit::Off) {
        Some(fit_is())
    } else if lossless {
        Some("lossless is on".to_owned())
    } else {
        None
    };
    if let (Some(_), Some(why)) = (options.bitrate, bitrate) {
        aside.push(("bitrate", why));
    }
    if options.fake_duration.is_some() && effective.spoof == Some(Spoof::Never) {
        aside.push(("fake-duration", "spoof is never".to_owned()));
    }
    aside
}

/// Default upper bitrate bound (in kbit/s) for size-fitting searches, far above
/// what any sticker can fit.
pub const HIGHEST_BITRATE: f64 = 50_000.0;

/// Default maximum size-fitting attempts: enough to land within a few percent
/// of the limit for most inputs at a few seconds per encode.
pub const DEFAULT_ATTEMPTS: u32 = 8;
/// Upper limit on `attempts`, to catch mistyped numbers.
pub const MAX_ATTEMPTS: u32 = 50;
/// Default libvpx-vp9 CRF when not fitting CRF (good quality at sticker
/// dimensions).
pub const DEFAULT_CRF: u8 = 32;
/// Valid libvpx-vp9 CRF range (0 = best, 63 = worst).
pub const CRFS: std::ops::RangeInclusive<u8> = 0..=63;
/// Default spoofed container duration in seconds (well under 3 s and easy to
/// recognise in metadata).
pub const DEFAULT_FAKE_DURATION: f64 = 0.42069;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::set_keys;

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
    fn says_why_each_value_is_set_aside() {
        let why = |options: Options, key: &str| {
            let aside = set_aside(&options, &options);
            aside.into_iter().find(|(set, _)| *set == key).map(|(_, why)| why)
        };
        let crf = Options { crf: Some(20), ..Options::default() };
        let fit = |fit| Options { fit: Some(fit), ..crf.clone() };
        assert_eq!(why(fit(Fit::Bitrate), "crf").as_deref(), Some("fit is bitrate"));
        assert_eq!(why(fit(Fit::Fps), "crf"), None);
        let lossless = Options { lossless: Some(true), ..fit(Fit::Fps) };
        assert_eq!(why(lossless, "crf").as_deref(), Some("lossless is on"));
        let bitrate = Options { bitrate: Some(300.0), ..fit(Fit::Off) };
        assert_eq!(why(bitrate.clone(), "crf").as_deref(), Some("bitrate is set"));
        assert_eq!(why(bitrate, "bitrate"), None);
        let bitrate = Options { bitrate: Some(300.0), lossless: Some(true), ..fit(Fit::Off) };
        assert_eq!(why(bitrate, "bitrate").as_deref(), Some("lossless is on"));
        let bitrate = Options { bitrate: Some(300.0), ..fit(Fit::Length) };
        assert_eq!(why(bitrate, "bitrate").as_deref(), Some("fit is length"));
    }

    #[test]
    fn says_its_defaults_once() {
        let unset = Options::default();
        assert_eq!(
            (unset.fit_or_default(), unset.effort_or_default(), unset.scaling_or_default()),
            (Fit::Auto, Effort::Balanced, Scaling::Auto)
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
}

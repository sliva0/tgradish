//! WebM's options, which image stickers take the picture options of.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::options::{Crop, ExactScale, Fit, Range, Resize, Scaling, Speed, Spoof, layered};
use crate::telegram::Target;

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
    /// tempo only repeats its frames. For a sequence of images it is also
    /// the rate its pictures are read at, how fast they play, so the
    /// result's is the same. Default: the input's at the tempo, from 1 to
    /// 60; 12 for a sequence.
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
}

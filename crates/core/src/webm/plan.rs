//! Planning a WebM conversion: every setting worked out from the options
//! and the input, and the frame counts and times they come to.

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;

use super::Request;
use crate::error::{Error, Result};
use crate::ffmpeg::Probe;
use crate::options::{self, Crop, Fit, FitUnit, Range, Resize, Scaling, Speed, Spoof};
use crate::picture::{default_resize, picture_sizes};
use crate::telegram::{self, Target};

/// Every setting of a conversion, after defaults and input properties are
/// taken into account.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Plan {
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub input: PathBuf,
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub output: PathBuf,
    pub source: Probe,
    pub target: Target,
    /// The part of the source used, in its display pixels.
    pub crop: Option<Crop>,
    pub resize: Resize,
    /// How pixels are scaled; never [`Scaling::Auto`].
    pub scaling: Scaling,
    /// Size of the encoded video.
    pub width: u32,
    pub height: u32,
    /// Size the source is scaled to before padding or cropping to the
    /// encoded size.
    pub scaled_width: u32,
    pub scaled_height: u32,
    pub fit: Fit,
    pub attempts: u32,
    pub fit_range: Range,
    /// Whether `fit = auto` may lower the frame rate. False when the frame
    /// rate was set explicitly.
    pub auto_fps: bool,
    /// Where the part used starts in the input, in seconds.
    pub start: f64,
    /// How long the result plays, in seconds.
    pub length: f64,
    /// How many times as fast as the input the result plays: it uses
    /// `length * tempo` seconds of the input.
    pub tempo: f64,
    pub fps: f64,
    /// Bitrate in kbit/s to use or to start fitting from.
    pub bitrate: f64,
    pub crf: u8,
    /// With `fit = off`, encode at constant quality instead of bitrate.
    /// True when only `crf` was set.
    pub constant_quality: bool,
    pub lossless: bool,
    pub speed: Speed,
    /// Encode with an alpha channel.
    pub alpha: bool,
    pub spoof: Spoof,
    pub fake_duration: f64,
    pub title: Option<String>,
    pub watermark: bool,
    pub encoder_options: BTreeMap<String, String>,
    /// Raw ffmpeg arguments, only for ffmpeg as a separate program.
    pub ffmpeg_args: Vec<String>,
}

impl Plan {
    /// The size of the part of the source used.
    pub fn used(&self) -> (u32, u32) {
        self.crop.map_or((self.source.width, self.source.height), |crop| (crop.width, crop.height))
    }

    /// How many times larger the picture gets, at least.
    pub fn enlarges(&self) -> f64 {
        let used = self.used();
        let along = |scaled: u32, used: u32| f64::from(scaled) / f64::from(used.max(1));
        along(self.scaled_width, used.0).min(along(self.scaled_height, used.1))
    }

    /// Whether a result of `length` seconds gets its duration spoofed.
    pub fn spoofs(&self, length: f64) -> bool {
        match self.spoof {
            Spoof::Auto => telegram::too_long(length),
            Spoof::Always => true,
            Spoof::Never => false,
        }
    }
}

/// Number of frames encoded for `length` seconds at `fps`. Rounds down so
/// the result never lasts longer than asked, which matters at the 3 second
/// limit, but always at least one frame; the `1e-6` takes float error for
/// what it is, so 3 s at 30 fps is 90 frames rather than 89.
pub fn frame_count(length: f64, fps: f64) -> u64 {
    ((length * fps + 1e-6).floor() as u64).max(1)
}

/// Where to start reading for the frame that starts at `start`, after one
/// that starts at `previous`. Reading keeps frames from the first that
/// starts at or after its start, and containers, ffprobe and decoders round
/// times differently (WebM to whole milliseconds): so a millisecond before
/// the frame's start, or a quarter of the gap to `previous` where that is
/// less, which never brings the previous frame back.
pub fn read_from(start: f64, previous: f64) -> f64 {
    start - ((start - previous) / 4.0).min(1e-3)
}

/// What fitting searches where no range is asked for, for a result of
/// `fps` frames a second, `length` seconds long: bitrates from what still
/// makes a picture to far past any sticker, libvpx's CRF short of its
/// near-lossless end, which no sticker fits, frame rates up to the
/// result's, and lengths from a tenth of a second, about the shortest
/// that still moves.
pub fn default_fit_range(fit: Fit, fps: f64, length: f64) -> Range {
    match fit {
        Fit::Auto | Fit::Bitrate | Fit::Off => Range { min: 8.0, max: super::HIGHEST_BITRATE },
        Fit::Crf => Range { min: 4.0, max: f64::from(*super::CRFS.end()) },
        Fit::Fps => Range { min: 1.0, max: fps.floor().max(1.0) },
        Fit::Length => Range { min: length.min(0.1), max: length },
    }
}

/// The frame rate a result has unless `fps` says otherwise, from an input
/// of `input_fps` played at `tempo`: the input's at the tempo, from 1,
/// so a slow tempo doesn't make a single frame outlast the result, to
/// Telegram's most.
pub fn played_fps(input_fps: f64, tempo: f64) -> f64 {
    (input_fps * tempo).clamp(1.0, telegram::MAX_FPS)
}

/// Duration of [`frame_count`] frames, as written into the file header.
pub fn encoded_length(length: f64, fps: f64) -> f64 {
    frame_count(length, fps) as f64 / fps
}

/// Whether `name` can be an ffmpeg option name. Keeps the process backend's
/// `-NAME:v` argument from turning into something else.
fn is_option_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Checks a fit range given against what `fit` tunes, the one place that
/// does: finite and in order, within [`Fit::range_limits`], and holding a
/// whole number where only those are tried. A range of lengths past the
/// result's `length` is cut to it, which `warnings` says.
fn check_fit_range(
    range: Range,
    fit: Fit,
    length: f64,
    warnings: &mut Vec<String>,
) -> Result<Range> {
    if !(range.min.is_finite() && range.max.is_finite()) {
        return Err(Error::invalid("fit-range", format!("{range} must be finite")));
    }
    if range.min > range.max {
        return Err(Error::invalid("fit-range", format!("{range} must have MIN <= MAX")));
    }
    // length fitting may only shorten: `length` already respects the input
    // and the spoofing policy
    let range = match fit {
        Fit::Length if range.max > length => {
            warnings.push(format!("length range capped at {length:.2} s"));
            Range { min: range.min.min(length), max: length }
        }
        _ => range,
    };
    let what = match fit.unit() {
        FitUnit::Kbps => "bitrate",
        FitUnit::Crf => "crf",
        FitUnit::Fps => "fps",
        FitUnit::Seconds => "length",
    };
    let limits = fit.range_limits();
    let (low, high) = match fit {
        Fit::Length => (*limits.start(), length),
        _ => (*limits.start(), *limits.end()),
    };
    if range.min < low || range.max > high {
        let reason = match (low, high.is_finite()) {
            (f64::MIN_POSITIVE, _) => format!("for {what} must be above 0 and at most {high}"),
            (_, false) => format!("for {what} must be at least {low}"),
            _ => format!("for {what} must be within {low}..{high}"),
        };
        return Err(Error::invalid("fit-range", reason));
    }
    if matches!(fit, Fit::Crf | Fit::Fps) && range.min.ceil() > range.max.floor() {
        let reason = format!("{range} for {what} contains no whole number");
        return Err(Error::invalid("fit-range", reason));
    }
    Ok(range)
}

/// Bitrate in kbit/s that should land a bit under `limit` bytes.
pub fn estimate_bitrate(length: f64, limit: u64) -> f64 {
    // leaves room for container overhead and encoder overshoot
    limit as f64 * 8.0 / length / 1000.0 * 0.93
}

/// Resolves options against the probed input. Returns the plan and warnings
/// about options that were adjusted or ignored.
pub fn plan(request: &Request, source: Probe) -> Result<(Plan, Vec<String>)> {
    let o = &request.options;
    let mut warnings = Vec::new();

    let target = o.target.unwrap_or_default();
    let resize = o.resize.unwrap_or(default_resize(target));
    if target.requires_exact_size() && resize == Resize::Contain {
        let (width, height) = target.box_size();
        let name = target.name();
        let reason = format!(
            "contain can't make {name}, exactly {width}x{height}: use pad, crop or stretch"
        );
        return Err(Error::invalid("resize", reason));
    }

    let output = request.output();

    let spoof = o.spoof_or_default();
    let fake_duration = o.fake_duration.unwrap_or(super::DEFAULT_FAKE_DURATION);
    if !(fake_duration > 0.0 && fake_duration <= telegram::MAX_SECONDS) {
        let reason = format!("must be more than 0 and at most {} seconds", telegram::MAX_SECONDS);
        return Err(Error::invalid("fake-duration", reason));
    }

    let start = o.start.unwrap_or(0.0);
    if !(start >= 0.0 && start.is_finite()) {
        return Err(Error::invalid("start", "must be 0 or more seconds"));
    }
    let available = match source.duration {
        Some(duration) if start >= duration => {
            let reason = format!("is past the end of the input ({duration:.2} s)");
            return Err(Error::invalid("start", reason));
        }
        Some(duration) => duration - start,
        None => f64::INFINITY,
    };
    let tempo = o.tempo_or_default();
    if !options::TEMPOS.contains(&tempo) {
        let (slowest, fastest) = (options::TEMPOS.start(), options::TEMPOS.end());
        return Err(Error::invalid("tempo", format!("must be from {slowest} to {fastest}")));
    }
    let used = match o.length {
        Some(length) if !(length > 0.0 && length.is_finite()) => {
            return Err(Error::invalid("length", "must be more than 0 seconds"));
        }
        // a hundredth of a second over is rounding, not worth a warning
        Some(length) if length > available + 0.01 => {
            warnings.push(format!("the input is only {available:.2} s long after start"));
            available
        }
        Some(length) => length,
        None if available.is_finite() => available,
        None => telegram::MAX_SECONDS * tempo,
    };
    // how long the result plays
    let mut length = used / tempo;
    if spoof == Spoof::Never && length > telegram::MAX_SECONDS {
        warnings.push(format!(
            "cutting to {} s because spoofing is off; Telegram refuses longer videos",
            telegram::MAX_SECONDS
        ));
        length = telegram::MAX_SECONDS;
    }
    // by default the input's frames a second at the tempo, the average of
    // a video whose rate varies; at least one a second, so a slow tempo
    // doesn't make a single frame outlast the result
    let fps = match o.fps {
        Some(fps) if !(fps > 0.0 && fps <= telegram::MAX_FPS) => {
            let reason = format!("must be more than 0 and at most {}", telegram::MAX_FPS);
            return Err(Error::invalid("fps", reason));
        }
        Some(fps) => fps,
        None if source.still_image => played_fps(source.fps_or_default(), 1.0),
        None => played_fps(source.fps_or_default(), tempo),
    };

    let crf = o.crf.unwrap_or(super::DEFAULT_CRF);
    if !super::CRFS.contains(&crf) {
        let (low, high) = (super::CRFS.start(), super::CRFS.end());
        return Err(Error::invalid("crf", format!("must be between {low} and {high}")));
    }
    let bitrate = match o.bitrate {
        Some(bitrate) if !(bitrate >= 1.0 && bitrate.is_finite()) => {
            return Err(Error::invalid("bitrate", "must be at least 1 kbit/s"));
        }
        Some(bitrate) => bitrate,
        None => estimate_bitrate(length, target.max_bytes()),
    };

    let fit = o.fit_or_default();
    let attempts = o.attempts.unwrap_or(super::DEFAULT_ATTEMPTS);
    if !(1..=super::MAX_ATTEMPTS).contains(&attempts) {
        let reason = format!("must be between 1 and {}", super::MAX_ATTEMPTS);
        return Err(Error::invalid("attempts", reason));
    }
    let lossless = o.lossless_or_default();
    if lossless && !matches!(fit, Fit::Fps | Fit::Length | Fit::Off) {
        return Err(Error::invalid("lossless", "only works with fit fps, length or off"));
    }
    let in_effect = super::in_effect(o.clone(), o);
    let fitting = serde_json::to_value(fit).ok().and_then(|v| v.as_str().map(str::to_owned));
    let fitting = fitting.unwrap_or_default();
    for (key, asked, kept) in [
        ("crf", o.crf.is_some(), in_effect.crf.is_some()),
        ("bitrate", o.bitrate.is_some(), in_effect.bitrate.is_some()),
    ] {
        if asked && !kept {
            warnings.push(format!("{key} is ignored with fit {fitting}"));
        }
    }

    let fit_range = match o.fit_range {
        Some(range) => check_fit_range(range, fit, length, &mut warnings)?,
        None => default_fit_range(fit, fps, length),
    };

    let encoder_options = o.encoder_options.clone().unwrap_or_default();
    if let Some(name) = encoder_options.keys().find(|name| !is_option_name(name)) {
        let reason =
            format!("has {name:?}, which isn't an option's name: use letters, digits, - and _");
        return Err(Error::invalid("encoder-options", reason));
    }

    if let Some(crop) = o.crop {
        crop.check(source.width, source.height)?;
    }
    let used = o.crop.map_or((source.width, source.height), |crop| (crop.width, crop.height));
    let sizes = picture_sizes(o, used)?;

    let plan = Plan {
        input: request.input.first().to_path_buf(),
        output,
        target,
        crop: o.crop,
        resize,
        scaling: sizes.scaling,
        width: sizes.width,
        height: sizes.height,
        scaled_width: sizes.scaled.0,
        scaled_height: sizes.scaled.1,
        fit,
        attempts,
        fit_range,
        auto_fps: o.fps.is_none(),
        start,
        length,
        tempo,
        fps,
        bitrate,
        crf,
        constant_quality: o.crf.is_some() && o.bitrate.is_none(),
        lossless,
        speed: o.speed_or_default(),
        alpha: source.alpha || sizes.pads(),
        spoof,
        fake_duration,
        title: o.title.clone(),
        watermark: o.watermark.unwrap_or(true),
        encoder_options,
        ffmpeg_args: o.ffmpeg_args.clone().unwrap_or_default(),
        source,
    };
    Ok((plan, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::options::ExactScale;
    use crate::picture::{exact_sizes, sizes};
    use crate::webm::Options;

    fn source(duration: f64) -> Probe {
        Probe {
            format: "mov,mp4".into(),
            codec: "h264".into(),
            width: 640,
            height: 480,
            fps: Some(30.0),
            base_fps: Some(30.0),
            duration: Some(duration),
            start: 0.0,
            alpha: false,
            still_image: false,
            orientation: Default::default(),
            decoder: None,
        }
    }

    fn plan_with(options: Options) -> Result<(Plan, Vec<String>)> {
        let request = Request { options, ..Request::new(Input::File("in.mp4".into())) };
        plan(&request, source(4.84))
    }

    #[test]
    fn validates_fit_ranges() {
        let range = |min, max| Some(Range { min, max });
        for (fit, fit_range) in [
            (Fit::Bitrate, range(400.0, 100.0)),
            (Fit::Bitrate, range(f64::NAN, 100.0)),
            (Fit::Bitrate, range(0.0, 100.0)),
            (Fit::Crf, range(10.0, 70.0)),
            (Fit::Fps, range(1.5, 1.7)),
            (Fit::Fps, range(10.0, 61.0)),
        ] {
            let options = Options { fit: Some(fit), fit_range, ..Default::default() };
            assert!(plan_with(options).is_err(), "{fit:?} {fit_range:?} was accepted");
        }
    }

    #[test]
    fn validates_ranges_before_capping_and_other_limits() {
        let reversed = Options {
            fit: Some(Fit::Length),
            length: Some(1.0),
            fit_range: Some(Range { min: 8.0, max: 4.0 }),
            ..Default::default()
        };
        assert!(plan_with(reversed).is_err());
        assert!(plan_with(Options { attempts: Some(51), ..Default::default() }).is_err());
        assert!(plan_with(Options { attempts: Some(0), ..Default::default() }).is_err());
        assert!(plan_with(Options { bitrate: Some(0.5), ..Default::default() }).is_err());
    }

    #[test]
    fn rejects_encoder_option_names_that_are_not_names() {
        for name in ["", "-f", "f webm", "x;y", "a=b"] {
            let encoder_options = Some([(name.to_string(), "1".to_string())].into());
            let options = Options { encoder_options, ..Default::default() };
            assert!(plan_with(options).is_err(), "{name:?} was accepted");
        }
        let encoder_options = Some([("tune-content".to_string(), "screen".into())].into());
        assert!(plan_with(Options { encoder_options, ..Default::default() }).is_ok());
    }

    #[test]
    fn length_fit_stays_within_planned_length() {
        let options = Options {
            fit: Some(Fit::Length),
            length: Some(2.0),
            fit_range: Some(Range { min: 1.0, max: 5.0 }),
            ..Default::default()
        };
        let (plan, warnings) = plan_with(options).unwrap();
        assert_eq!(plan.fit_range, Range { min: 1.0, max: 2.0 });
        assert_eq!(warnings.len(), 1);

        let tiny = Options { fit: Some(Fit::Length), length: Some(0.04), ..Default::default() };
        let (plan, _) = plan_with(tiny).unwrap();
        assert!(plan.fit_range.min <= plan.fit_range.max);
    }

    #[test]
    fn frame_counts_never_exceed_length() {
        assert_eq!(frame_count(3.0, 29.97), 89);
        assert!(encoded_length(3.0, 29.97) <= 3.0);
        assert_eq!(frame_count(4.84, 25.0), 121);
        assert_eq!(frame_count(0.001, 25.0), 1);
    }

    #[test]
    fn sizes_a_crop_like_a_whole_input() {
        let crop = Some(Crop { x: 100, y: 40, width: 300, height: 100 });
        let (plan, _) = plan_with(Options { crop, ..Default::default() }).unwrap();
        assert_eq!((plan.width, plan.height), (512, 170));
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p");
        assert!(
            filter.contains("crop=w=iw*300/640:h=ih*100/480:x=iw*100/640:y=ih*40/480:exact=1,"),
            "{filter}"
        );
        assert!(filter.contains(",scale=512:170:flags=lanczos,"), "{filter}");
        let outside = Some(Crop { x: 400, y: 0, width: 300, height: 100 });
        assert!(plan_with(Options { crop: outside, ..Default::default() }).is_err());
    }

    #[test]
    fn scales_in_linear_light() {
        let (plan, _) = plan_with(Options::default()).unwrap();
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p");
        // no alpha to premultiply in an opaque source
        let linear = "format=gbrp16le,lutrgb=r='if(lte(val/maxval\\,0.04045)";
        assert!(filter.contains(linear) && !filter.contains("premultiply"), "{filter}");
        let lanczos = filter.find("scale=512:384:flags=lanczos").unwrap();
        assert!(filter[lanczos..].contains("lutrgb=r='if(lte(val/maxval\\,0.0031308)"));
    }

    #[test]
    fn keeps_pixels_whole() {
        let used = (50, 30);
        let sizes = |resize, scaling| sizes(Target::Sticker, resize, scaling, used);
        // auto is smooth until the picture turns out to be pixel art
        let auto = sizes(Resize::Contain, Scaling::Auto);
        assert_eq!((auto.scaled, auto.width, auto.height), ((512, 308), 512, 308));
        assert_eq!(auto.scaling, Scaling::Smooth);
        assert!(auto.enlarges(used) > 10.0);
        // ten times, with transparent margins up to the sticker's size
        let whole = sizes(Resize::Contain, Scaling::PixelPerfect);
        assert_eq!((whole.scaled, whole.width, whole.height), ((500, 300), 512, 308));
        assert!(whole.pads());
        let filled = sizes(Resize::Crop, Scaling::PixelPerfect);
        assert_eq!((filled.scaled, filled.width, filled.height), ((900, 540), 512, 512));
        // nothing to make whole when the picture shrinks
        let large = crate::picture::sizes(
            Target::Sticker,
            Resize::Contain,
            Scaling::PixelPerfect,
            (900, 600),
        );
        assert_eq!(large.scaling, Scaling::Sharp);

        let crop = Some(Crop { x: 0, y: 0, width: 50, height: 30 });
        let options = Options { crop, scaling: Some(Scaling::Sharp), ..Default::default() };
        let (plan, _) = plan_with(options).unwrap();
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p");
        // 11 times as blocks, then down to 512 x 308 blending only edges
        assert!(filter.contains("scale=550:330:flags=neighbor,format=gbrp16le"), "{filter}");
        assert!(filter.contains("scale=512:308:flags=area"), "{filter}");
        let options = Options { crop, scaling: Some(Scaling::PixelPerfect), ..Default::default() };
        let (plan, _) = plan_with(options).unwrap();
        assert!(plan.alpha);
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuva420p");
        assert!(
            filter.contains(
                "scale=500:300:flags=neighbor,format=rgba,pad=512:308:trunc((ow-iw)/4)*2:"
            ),
            "{filter}"
        );
    }

    #[test]
    fn scales_exactly() {
        let half: ExactScale = "1/2".parse().unwrap();
        let twice: ExactScale = "2".parse().unwrap();
        assert_eq!("0.5".parse::<ExactScale>(), Ok(half));
        assert_eq!("2x".parse::<ExactScale>(), Ok(twice));
        assert!("1.5".parse::<ExactScale>().is_err() && "2/3".parse::<ExactScale>().is_err());
        assert!("4294967295".parse::<ExactScale>().is_err());
        let huge = ExactScale { up: u32::MAX, down: 1 };
        let exact = |target, used, scale| exact_sizes(target, Resize::Contain, used, scale);
        assert!(exact(Target::Sticker, (2, 2), huge).is_err());
        // made in code, with nothing to scale by
        for zero in [ExactScale { up: 0, down: 1 }, ExactScale { up: 1, down: 0 }] {
            let wrong = exact(Target::Sticker, (4, 4), zero).unwrap_err();
            assert!(wrong.to_string().contains("side of 0"), "{wrong}");
            assert!(matches!(wrong, Error::InvalidOption { option: "exact-scale", .. }));
        }
        assert_eq!((half.to_string(), twice.to_string()), ("1/2".into(), "2".into()));

        // 1024 x 576 halved makes a 512 x 288 sticker, pixels in 2x2 blocks
        let sizes = exact(Target::Sticker, (1024, 576), half).unwrap();
        assert_eq!((sizes.scaled, sizes.width, sizes.height), ((512, 288), 512, 288));
        assert_eq!(sizes.scaling, Scaling::Sharp);
        let grown = exact(Target::Sticker, (256, 100), twice).unwrap();
        assert_eq!((grown.width, grown.height, grown.scaling), (512, 200, Scaling::PixelPerfect));
        // scales that don't make 512 leave transparent margins
        let thrice: ExactScale = "3".parse().unwrap();
        let margins = exact(Target::Sticker, (170, 101), thrice).unwrap();
        assert_eq!((margins.scaled, margins.width, margins.height), ((510, 303), 512, 304));
        assert!(margins.pads());
        let padded = exact_sizes(Target::Sticker, Resize::Pad, (170, 101), thrice).unwrap();
        assert_eq!((padded.width, padded.height), (512, 512));
        let emoji = exact(Target::Emoji, (33, 20), thrice).unwrap();
        assert_eq!((emoji.scaled, emoji.width, emoji.height), ((99, 60), 100, 100));
        // never larger than the box, and only whole blocks when shrinking
        let wrong = exact(Target::Sticker, (300, 100), twice).unwrap_err();
        assert!(wrong.to_string().contains("at most 256 pixels"), "{wrong}");
        assert!(exact(Target::Emoji, (51, 40), twice).is_err());
        assert!(exact(Target::Sticker, (1023, 576), half).is_err());
        assert!(exact_sizes(Target::Sticker, Resize::Crop, (256, 100), twice).is_err());

        let crop = Some(Crop { x: 0, y: 0, width: 256, height: 144 });
        let options = Options { crop, exact_scale: Some(twice), ..Default::default() };
        let (plan, _) = plan_with(options).unwrap();
        assert_eq!((plan.width, plan.height, plan.scaling), (512, 288, Scaling::PixelPerfect));
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p");
        assert!(filter.contains("scale=512:288:flags=neighbor") && !filter.contains("pad="));
    }

    #[test]
    fn spoofs_by_result_length() {
        let (plan, _) = plan_with(Options::default()).unwrap();
        assert_eq!(plan.spoof, Spoof::Auto);
        assert!(plan.spoofs(4.84) && !plan.spoofs(3.0));
    }

    #[test]
    fn plays_at_a_tempo() {
        // the input's frames, all of them up to 60 a second
        let (plan, _) = plan_with(Options::default()).unwrap();
        assert_eq!((plan.length, plan.tempo, plan.fps), (4.84, 1.0, 30.0));
        assert!(!crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p").contains("setpts"));
        let twice = Options { tempo: Some(2.0), ..Options::default() };
        let (plan, _) = plan_with(twice).unwrap();
        assert_eq!((plan.length, plan.fps), (2.42, 60.0));
        let filter = crate::ffmpeg::video_filter(&plan, 60.0, 2.42, "yuv420p");
        assert!(filter.starts_with("settb=AVTB,setpts=(PTS-STARTPTS)/2,fps=60,"), "{filter}");
        // reads the input's seconds that play for the result's, and a frame
        let args = crate::ffmpeg::input_args(&plan, 1.0, 60.0);
        let read = args.iter().position(|arg| arg == "-t").unwrap() + 1;
        assert_eq!(args[read].to_str(), Some("2.033333"));

        let slow = Options { tempo: Some(0.5), ..Options::default() };
        let (plan, _) = plan_with(slow).unwrap();
        assert_eq!((plan.length, plan.fps), (9.68, 15.0));
        // at least a frame a second, and what is asked for
        let slowest = Options { tempo: Some(0.1), spoof: Some(Spoof::Never), ..Options::default() };
        let request = Request { options: slowest, ..Request::new(Input::File("in.mp4".into())) };
        let (plan, _) = super::plan(&request, Probe { fps: Some(1.0), ..source(4.84) }).unwrap();
        assert_eq!((plan.fps, plan.length), (1.0, 3.0));
        let asked = Options { tempo: Some(0.1), fps: Some(30.0), ..Options::default() };
        assert_eq!(plan_with(asked).unwrap().0.fps, 30.0);
        // without spoofing, as much as plays for 3 seconds
        let honest = Options { tempo: Some(0.5), spoof: Some(Spoof::Never), ..Options::default() };
        assert_eq!(plan_with(honest).unwrap().0.length, 3.0);
        for tempo in [0.0, 0.05, 11.0, f64::NAN] {
            assert!(plan_with(Options { tempo: Some(tempo), ..Options::default() }).is_err());
        }
    }
}

//! Resolves WebM conversion options and probed media metadata into a concrete
//! [`Plan`], plus frame-counting and frame-boundary trimming helpers.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use super::Request;
use crate::config::Threads;
use crate::error::{Error, Result};
use crate::ffmpeg::Probe;
use crate::options::{self, Crop, Effort, Fit, FitUnit, Range, Resize, Scaling, Selection, Spoof};
use crate::picture::{default_resize, picture_sizes};
use crate::presets::Format;
use crate::telegram::{self, Target};

/// Fully resolved settings for a WebM (or WebP) conversion.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Plan {
    /// Input path passed to ffmpeg.
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub input: PathBuf,
    /// Destination output path.
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub output: PathBuf,
    /// Probed source media properties.
    pub source: Probe,
    /// Telegram target (sticker or emoji).
    pub target: Target,
    /// Crop region in rotated display pixels.
    pub crop: Option<Crop>,
    /// Resize mode.
    pub resize: Resize,
    /// Resolved scaling mode (never [`Scaling::Auto`]).
    pub scaling: Scaling,
    /// Encoded output width in pixels.
    pub width: u32,
    /// Encoded output height in pixels.
    pub height: u32,
    /// Intermediate scaled width before cropping or padding.
    pub scaled_width: u32,
    /// Intermediate scaled height before cropping or padding.
    pub scaled_height: u32,
    /// Size-fitting strategy.
    pub fit: Fit,
    /// Maximum size-fitting attempts.
    pub attempts: u32,
    /// Search range in the unit of `fit`.
    pub fit_range: Range,
    /// True if `fit = auto` may try lower frame rates (false when `fps` was
    /// set explicitly).
    pub auto_fps: bool,
    /// Start offset in seconds.
    pub start: f64,
    /// Output duration in seconds.
    pub length: f64,
    /// Playback speed multiplier (consumes `length * tempo` seconds of input).
    pub tempo: f64,
    /// Output frame rate (or starting rate for fitting).
    pub fps: f64,
    /// Target or initial bitrate in kbit/s.
    pub bitrate: f64,
    /// Constant quality level (0..=63).
    pub crf: u8,
    /// Under `fit = off`, use CRF instead of bitrate (true when only `crf` was
    /// set).
    pub constant_quality: bool,
    /// Lossless VP9 flag.
    pub lossless: bool,
    /// Encoder effort level.
    pub effort: Effort,
    /// True if the output needs an alpha channel.
    pub alpha: bool,
    /// Container duration spoofing policy.
    pub spoof: Spoof,
    /// Duration written when spoofing, in seconds.
    pub fake_duration: f64,
    /// Optional title metadata.
    pub title: Option<String>,
    /// Whether to write tgradish metadata tags and padding signature.
    pub watermark: bool,
    /// Extra `libvpx-vp9` encoder options applied on top of tgradish's
    /// defaults.
    pub encoder_options: BTreeMap<String, String>,
    /// Extra raw `ffmpeg` arguments (external process backend only).
    pub ffmpeg_args: Vec<String>,
    /// Maximum threads for decoding, filtering and encoding (overridden by
    /// explicit `threads` in `encoder_options` or `ffmpeg_args`).
    pub threads: Threads,
}

impl Plan {
    /// Width and height of the used source region (crop or full frame).
    pub fn used(&self) -> (u32, u32) {
        self.crop.map_or((self.source.width, self.source.height), |crop| (crop.width, crop.height))
    }

    /// Minimum upscale factor along either axis.
    pub fn enlarges(&self) -> f64 {
        let used = self.used();
        let along = |scaled: u32, used: u32| f64::from(scaled) / f64::from(used.max(1));
        along(self.scaled_width, used.0).min(along(self.scaled_height, used.1))
    }

    /// Returns true if an encode of `length` seconds should have its header
    /// duration spoofed.
    pub fn spoofs(&self, length: f64) -> bool {
        match self.spoof {
            Spoof::Auto => telegram::too_long(length),
            Spoof::Always => true,
            Spoof::Never => false,
        }
    }
}

/// Number of frames encoded for `length` seconds at `fps` (minimum 1).
/// Floors with a `1e-6` tolerance so 3 s at 30 fps yields 90 frames (even if
/// floating-point arithmetic gives `89.99999999999999`), without ever rounding
/// up and exceeding the 3 s limit (e.g. 3 s at 29.97 fps yields 89 frames,
/// 2.97 s, rather than 90 frames, 3.003 s).
pub fn frame_count(length: f64, fps: f64) -> u64 {
    ((length * fps + 1e-6).floor() as u64).max(1)
}

/// Computes a safe seek timestamp just before `start` (at most 1 ms earlier,
/// capped at a quarter of the gap from `previous`) so container/decoder
/// timestamp rounding (such as Matroska's millisecond timebase) never skips
/// the target frame or lands early enough to pull in `previous`.
pub fn read_from(start: f64, previous: f64) -> f64 {
    start - ((start - previous) / 4.0).min(1e-3)
}

/// Maximum snapping tolerance around frame edges, in seconds (matching
/// [`tgradish_tgs::normalise::CUT_LEEWAY`]).
pub const CUT_LEEWAY: f64 = tgradish_tgs::normalise::CUT_LEEWAY.as_secs_f64();

/// Snapping tolerance around `times[index]` (`times`: frame starts in order,
/// optionally followed by the end of the last frame): [`CUT_LEEWAY`], capped
/// at a quarter of the adjacent frame durations so short frames keep at least
/// half their duration unsnapped in the middle.
pub fn edge_leeway(times: &[f64], index: usize) -> f64 {
    let before = index.checked_sub(1).map(|before| times[index] - times[before]);
    let after = times.get(index + 1).map(|after| after - times[index]);
    before.into_iter().chain(after).fold(CUT_LEEWAY, |leeway, frame| leeway.min(frame / 4.0))
}

/// Returns the index in `times` (frame starts in order, optionally followed by
/// the end of the last frame) closest to `time` within [`edge_leeway`], if any:
/// the edge a cut at `time` is meant for (written with fewer digits or
/// rounded).
pub fn frame_edge(time: f64, times: &[f64]) -> Option<usize> {
    let after = times.partition_point(|&edge| edge < time);
    [after.checked_sub(1), Some(after)]
        .into_iter()
        .flatten()
        .filter(|&index| index < times.len())
        .filter(|&index| (time - times[index]).abs() <= edge_leeway(times, index))
        .min_by(|&a, &b| (time - times[a]).abs().total_cmp(&(time - times[b]).abs()))
}

/// Adjusts `selection.start` and `selection.length` for drawn animations whose
/// frame boundaries are `times` (including the end of the last frame). Snaps a
/// `start` within [`edge_leeway`] of a frame edge via [`read_from`]; a `length`
/// whose end (from the start as given) is within it of a frame edge becomes the
/// time from the snapped frame to that edge. Cuts elsewhere are kept.
pub fn drawn_cut(selection: &Selection, times: &[f64]) -> Selection {
    let mut cut = selection.clone();
    let Some(last) = times.len().checked_sub(1).filter(|&last| last > 0) else { return cut };
    let asked = selection.start.unwrap_or(0.0);
    let first = frame_edge(asked, times).filter(|&first| first < last);
    if let Some(first) = first {
        let from = if first == 0 { 0.0 } else { read_from(times[first], times[first - 1]) };
        cut.start = selection.start.map(|_| from);
    }
    let from = first.map_or(asked, |first| times[first]);
    if let Some(length) = selection.length
        && let Some(end) = frame_edge(asked + length, times).filter(|&end| times[end] > from)
    {
        cut.length = Some(times[end] - from);
    }
    cut
}

/// Default size-fitting search range for `fit` given `fps` and `length`
/// (shared by the planner and GUI placeholders): bitrate from 8 kbit/s
/// (about the least that still gives a picture) to far past any sticker; CRF
/// 4 to 63 (stopping short of near-lossless, which no sticker fits); fps up
/// to the result's; length from 0.1 s, about the shortest that still moves.
pub fn default_fit_range(fit: Fit, fps: f64, length: f64) -> Range {
    match fit {
        Fit::Auto | Fit::Bitrate | Fit::Off => Range { min: 8.0, max: super::HIGHEST_BITRATE },
        Fit::Crf => Range { min: 4.0, max: f64::from(*super::CRFS.end()) },
        Fit::Fps => Range { min: 1.0, max: fps.floor().max(1.0) },
        Fit::Length => Range { min: length.min(0.1), max: length },
    }
}

/// Default output frame rate for `input_fps` played at `tempo`, clamped to
/// `1.0..=60.0` fps (at least 1, so a slow tempo cannot make one frame outlast
/// the result; shared with the GUI placeholder).
pub fn played_fps(input_fps: f64, tempo: f64) -> f64 {
    (input_fps * tempo).clamp(1.0, telegram::MAX_FPS)
}

/// Duration of [`frame_count`] frames, as written into the file header.
pub(crate) fn encoded_length(length: f64, fps: f64) -> f64 {
    frame_count(length, fps) as f64 / fps
}

/// Whether `name` can be an ffmpeg option name. Keeps the process backend's
/// `-NAME:v` argument from turning into something else.
fn is_option_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Validates and normalises an explicit `fit-range` against `fit` and `length`
/// (finite, `min <= max`, within [`Fit::range_limits`], and containing at least
/// one integer for integer-only strategies). Length ranges exceeding `length`
/// are clamped to `length` with a warning.
fn check_fit_range(
    range: Range,
    fit: Fit,
    length: f64,
    warnings: &mut Vec<String>,
) -> Result<Range> {
    if !(range.min.is_finite() && range.max.is_finite()) {
        return Err(Error::invalid("fit-range", format!("{range} must have finite ends")));
    }
    if range.min > range.max {
        return Err(Error::invalid("fit-range", format!("{range} must have MIN <= MAX")));
    }
    // Length fitting can only shorten: `length` already reflects the input
    // duration and spoofing policy.
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
            (f64::MIN_POSITIVE, _) => {
                format!("for {what} must be greater than 0 and at most {high}")
            }
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

/// Bitrate in kbit/s that should land slightly under `limit` bytes.
pub(crate) fn estimate_bitrate(length: f64, limit: u64) -> f64 {
    // Leave 7% headroom for container overhead and encoder overshoot.
    limit as f64 * 8.0 / length / 1000.0 * 0.93
}

/// Resolves options against the probed input. Returns the plan and warnings
/// about options that were adjusted or ignored.
pub fn plan(request: &Request, source: Probe) -> Result<(Plan, Vec<String>)> {
    plan_for(Format::Webm, request, source)
}

/// Plans a conversion for `format` (WebP reuses WebM planning for a single
/// frame with format-specific output dimensions).
pub(crate) fn plan_for(
    format: Format,
    request: &Request,
    source: Probe,
) -> Result<(Plan, Vec<String>)> {
    let (chosen, o) = (&request.selection, &request.options);
    let mut warnings = Vec::new();

    let target = chosen.target.unwrap_or_default();
    let resize = o.resize.unwrap_or(default_resize(target));
    if target.requires_exact_size() && resize == Resize::Contain {
        let (width, height) = target.box_size();
        let name = target.name();
        let reason = format!(
            "contain cannot produce {name} at exactly {width}×{height}; use pad, crop or stretch"
        );
        return Err(Error::invalid("resize", reason));
    }

    let output = request.output();

    let spoof = o.spoof_or_default();
    let fake_duration = o.fake_duration.unwrap_or(super::DEFAULT_FAKE_DURATION);
    if !(fake_duration > 0.0 && fake_duration <= telegram::MAX_SECONDS) {
        let reason =
            format!("must be greater than 0 and at most {} seconds", telegram::MAX_SECONDS);
        return Err(Error::invalid("fake-duration", reason));
    }

    let start = chosen.start.unwrap_or(0.0);
    if !(start >= 0.0 && start.is_finite()) {
        return Err(Error::invalid("start", "must be 0 seconds or greater"));
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
    let used = match chosen.length {
        Some(length) if !(length > 0.0 && length.is_finite()) => {
            return Err(Error::invalid("length", "must be greater than 0 seconds"));
        }
        // Ignore <= 10 ms overshoot caused by timestamp rounding.
        Some(length) if length > available + 0.01 => {
            warnings.push(format!("input is only {available:.2} s long after start"));
            available
        }
        Some(length) => length,
        None if available.is_finite() => available,
        None => telegram::MAX_SECONDS * tempo,
    };
    // Playback duration after applying tempo.
    let mut length = used / tempo;
    if spoof == Spoof::Never && length > telegram::MAX_SECONDS {
        warnings.push(format!(
            "trimming to {} s because spoofing is off (Telegram rejects longer videos)",
            telegram::MAX_SECONDS
        ));
        length = telegram::MAX_SECONDS;
    }
    // Default to input fps scaled by tempo (using average fps for VFR inputs),
    // clamped to at least 1 fps so a slow tempo never makes one frame outlast
    // the output duration.
    let fps = match o.fps {
        Some(fps) if !(fps > 0.0 && fps <= telegram::MAX_FPS) => {
            let reason = format!("must be greater than 0 and at most {}", telegram::MAX_FPS);
            return Err(Error::invalid("fps", reason));
        }
        Some(fps) => fps,
        None if source.still_image => played_fps(source.fps_or_default(), 1.0),
        None => played_fps(source.fps_or_default(), tempo),
    };

    let crf = o.crf.unwrap_or(super::DEFAULT_CRF);
    if !super::CRFS.contains(&crf) {
        let (low, high) = (super::CRFS.start(), super::CRFS.end());
        return Err(Error::invalid("crf", format!("must be from {low} to {high}")));
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
        let reason = format!("must be from 1 to {}", super::MAX_ATTEMPTS);
        return Err(Error::invalid("attempts", reason));
    }
    let lossless = o.lossless_or_default();
    if lossless && !matches!(fit, Fit::Fps | Fit::Length | Fit::Off) {
        return Err(Error::invalid("lossless", "only works with fit fps, length or off"));
    }
    // lossless encoding where it has no say was refused above
    for (key, why) in super::set_aside(o, o) {
        if key != "lossless" {
            warnings.push(format!("{key} is ignored because {why}"));
        }
    }

    let fit_range = match o.fit_range {
        Some(range) => check_fit_range(range, fit, length, &mut warnings)?,
        None => default_fit_range(fit, fps, length),
    };

    let encoder_options = o.encoder_options.clone().unwrap_or_default();
    if let Some(name) = encoder_options.keys().find(|name| !is_option_name(name)) {
        let reason = format!("contains invalid option name {name:?}; use letters, digits, - and _");
        return Err(Error::invalid("encoder-options", reason));
    }

    if let Some(crop) = chosen.crop {
        crop.check(source.width, source.height)?;
    }
    let used = chosen.crop.map_or((source.width, source.height), |crop| (crop.width, crop.height));
    let sizes = picture_sizes(format, target, o, used)?;

    let plan = Plan {
        input: request.input.first().to_path_buf(),
        output,
        target,
        crop: chosen.crop,
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
        effort: o.effort_or_default(),
        alpha: source.alpha || sizes.pads(),
        spoof,
        fake_duration,
        title: chosen.title.clone(),
        watermark: o.watermark.unwrap_or(true),
        encoder_options,
        ffmpeg_args: o.ffmpeg_args.clone().unwrap_or_default(),
        threads: request.threads,
        source,
    };
    Ok((plan, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::options::IntegerScale;
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
        plan_chosen(Selection::default(), options)
    }

    /// Planned with what `selection` chooses of the input.
    fn plan_chosen(selection: Selection, options: Options) -> Result<(Plan, Vec<String>)> {
        let input = Input::File("in.mp4".into());
        let request = Request { selection, options, ..Request::new(input) };
        plan(&request, source(4.84))
    }

    fn length(length: f64) -> Selection {
        Selection { length: Some(length), ..Selection::default() }
    }

    fn crop(crop: Option<Crop>) -> Selection {
        Selection { crop, ..Selection::default() }
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
            fit_range: Some(Range { min: 8.0, max: 4.0 }),
            ..Default::default()
        };
        assert!(plan_chosen(length(1.0), reversed).is_err());
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
    fn warns_of_what_sets_a_quality_or_bitrate_aside() {
        let crf = Options { crf: Some(20), ..Default::default() };
        let warned = |options: Options| plan_with(crf.clone().merged(&options)).unwrap().1;
        let lossless = Options { fit: Some(Fit::Fps), lossless: Some(true), ..Default::default() };
        assert_eq!(warned(lossless), ["crf is ignored because lossless is on"]);
        let bitrate = Options { fit: Some(Fit::Off), bitrate: Some(300.0), ..Default::default() };
        assert_eq!(warned(bitrate), ["crf is ignored because bitrate is set"]);
        let fitted = Options { fit: Some(Fit::Crf), ..Default::default() };
        assert_eq!(warned(fitted), ["crf is ignored because fit is crf"]);
        // and of fitting's own settings where it is off, and a duration
        // never written
        let off = Options {
            fit: Some(Fit::Off),
            attempts: Some(3),
            spoof: Some(Spoof::Never),
            fake_duration: Some(1.0),
            ..Default::default()
        };
        let warned = plan_with(off).unwrap().1;
        assert_eq!(
            warned[warned.len() - 2..],
            [
                "attempts is ignored because fit is off",
                "fake-duration is ignored because spoof is never",
            ]
        );
    }

    #[test]
    fn length_fit_stays_within_planned_length() {
        let options = Options {
            fit: Some(Fit::Length),
            fit_range: Some(Range { min: 1.0, max: 5.0 }),
            ..Default::default()
        };
        let (plan, warnings) = plan_chosen(length(2.0), options).unwrap();
        assert_eq!(plan.fit_range, Range { min: 1.0, max: 2.0 });
        assert_eq!(warnings.len(), 1);

        let tiny = Options { fit: Some(Fit::Length), ..Default::default() };
        let (plan, _) = plan_chosen(length(0.04), tiny).unwrap();
        assert!(plan.fit_range.min <= plan.fit_range.max);
    }

    #[test]
    fn cuts_drawn_frames_where_a_time_near_them_is_meant() {
        // frames of a third of a second, 33 ms, a sixtieth and others
        let durations = [1.0 / 3.0, 0.033, 1.0 / 60.0, 1.0 / 60.0, 0.1, 1.0 / 12.0, 0.007];
        let mut times = vec![0.0];
        for duration in durations {
            times.push(times.last().unwrap() + duration);
        }
        let last = durations.len();
        // as a number with `digits` decimals reads back
        let written =
            |seconds: f64, digits: usize| -> f64 { format!("{seconds:.digits$}").parse().unwrap() };
        for first in 0..last {
            for after in first + 1..=last {
                for digits in [5, 6, 9, 17] {
                    let selection = Selection {
                        start: (first > 0).then(|| written(times[first], digits)),
                        length: (after < last)
                            .then(|| written(times[after] - times[first], digits)),
                        ..Selection::default()
                    };
                    let cut = drawn_cut(&selection, &times);
                    let what = format!("{first}..{after} with {digits} digits: {cut:?}");
                    match first {
                        0 => assert_eq!(cut.start, None, "{what}"),
                        _ => {
                            let from = read_from(times[first], times[first - 1]);
                            assert!((cut.start.unwrap() - from).abs() < 1e-12, "{what}");
                        }
                    }
                    if let Some(length) = cut.length {
                        let frames = times[after] - times[first];
                        assert!((length - frames).abs() < 1e-12, "{what}");
                    }
                }
            }
        }
        // a cut into a frame is as asked, and so is what isn't drawn
        let into = Selection { start: Some(0.2), length: Some(0.15), ..Selection::default() };
        assert_eq!(drawn_cut(&into, &times), into);
        let exact = Selection { start: Some(times[1]), ..Selection::default() };
        assert_eq!(drawn_cut(&exact, &[]), exact);
        // which edge is the nearest, within a quarter of the frames beside
        assert_eq!(frame_edge(0.36633, &times), Some(2));
        assert_eq!(frame_edge(0.3, &times), None);
        assert_eq!(frame_edge(-0.00001, &times), Some(0));
        assert_eq!(frame_edge(times[last] + 0.00002, &times), Some(last));
        let short = [0.0, 0.1, 0.1002, 0.2];
        assert!((edge_leeway(&short, 1) - 0.00005).abs() < 1e-12);
        assert_eq!(edge_leeway(&short, 0), CUT_LEEWAY);
        assert_eq!(frame_edge(0.10006, &short), None);
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
        let cropped = Some(Crop { x: 100, y: 40, width: 300, height: 100 });
        let (plan, _) = plan_chosen(crop(cropped), Options::default()).unwrap();
        assert_eq!((plan.width, plan.height), (512, 170));
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p");
        assert!(
            filter.contains("crop=w=iw*300/640:h=ih*100/480:x=iw*100/640:y=ih*40/480:exact=1,"),
            "{filter}"
        );
        assert!(filter.contains(",scale=512:170:flags=lanczos,"), "{filter}");
        let outside = Some(Crop { x: 400, y: 0, width: 300, height: 100 });
        assert!(plan_chosen(crop(outside), Options::default()).is_err());
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
        let sizes = |resize, scaling| sizes(Format::Webm, Target::Sticker, resize, scaling, used);
        // auto is smooth until the picture turns out to be pixel art
        let auto = sizes(Resize::Contain, Scaling::Auto);
        assert_eq!((auto.scaled, auto.width, auto.height), ((512, 308), 512, 308));
        assert_eq!(auto.scaling, Scaling::Smooth);
        assert!(auto.enlarges(used) > 10.0);
        // ten times, with transparent margins up to the sticker's size
        let whole = sizes(Resize::Contain, Scaling::Integer);
        assert_eq!((whole.scaled, whole.width, whole.height), ((500, 300), 512, 308));
        assert!(whole.pads());
        let filled = sizes(Resize::Crop, Scaling::Integer);
        assert_eq!((filled.scaled, filled.width, filled.height), ((900, 540), 512, 512));
        // nothing to make whole when the picture shrinks
        let large = crate::picture::sizes(
            Format::Webm,
            Target::Sticker,
            Resize::Contain,
            Scaling::Integer,
            (900, 600),
        );
        assert_eq!(large.scaling, Scaling::Sharp);

        let cropped = crop(Some(Crop { x: 0, y: 0, width: 50, height: 30 }));
        let options = Options { scaling: Some(Scaling::Sharp), ..Default::default() };
        let (plan, _) = plan_chosen(cropped.clone(), options).unwrap();
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuv420p");
        // 11 times as blocks, then down to 512 x 308 blending only edges
        assert!(filter.contains("scale=550:330:flags=neighbor,format=gbrp16le"), "{filter}");
        assert!(filter.contains("scale=512:308:flags=area"), "{filter}");
        let options = Options { scaling: Some(Scaling::Integer), ..Default::default() };
        let (plan, _) = plan_chosen(cropped, options).unwrap();
        assert!(plan.alpha);
        let filter = crate::ffmpeg::video_filter(&plan, 30.0, 1.0, "yuva420p");
        assert!(
            // 6 and 4 pixels in, on the colour planes' grid
            filter.contains("scale=500:300:flags=neighbor,format=rgba,pad=512:308:6:4:"),
            "{filter}"
        );
    }

    #[test]
    fn scales_exactly() {
        let half: IntegerScale = "1/2".parse().unwrap();
        let twice: IntegerScale = "2".parse().unwrap();
        assert_eq!("0.5".parse::<IntegerScale>(), Ok(half));
        assert_eq!("2x".parse::<IntegerScale>(), Ok(twice));
        assert!("1.5".parse::<IntegerScale>().is_err() && "2/3".parse::<IntegerScale>().is_err());
        assert!("4294967295".parse::<IntegerScale>().is_err());
        let huge = IntegerScale { up: u32::MAX, down: 1 };
        let exact =
            |target, used, scale| exact_sizes(Format::Webm, target, Resize::Contain, used, scale);
        assert!(exact(Target::Sticker, (2, 2), huge).is_err());
        // made in code, with nothing to scale by
        for zero in [IntegerScale { up: 0, down: 1 }, IntegerScale { up: 1, down: 0 }] {
            let wrong = exact(Target::Sticker, (4, 4), zero).unwrap_err();
            assert!(wrong.to_string().contains("side of 0"), "{wrong}");
            assert!(matches!(wrong, Error::InvalidOption { option: "integer-scale", .. }));
        }
        assert_eq!((half.to_string(), twice.to_string()), ("1/2".into(), "2".into()));

        // 1024 x 576 halved makes a 512 x 288 sticker, pixels in 2x2 blocks
        let sizes = exact(Target::Sticker, (1024, 576), half).unwrap();
        assert_eq!((sizes.scaled, sizes.width, sizes.height), ((512, 288), 512, 288));
        assert_eq!(sizes.scaling, Scaling::Sharp);
        let grown = exact(Target::Sticker, (256, 100), twice).unwrap();
        assert_eq!((grown.width, grown.height, grown.scaling), (512, 200, Scaling::Integer));
        // scales that don't make 512 leave transparent margins
        let thrice: IntegerScale = "3".parse().unwrap();
        let margins = exact(Target::Sticker, (170, 101), thrice).unwrap();
        assert_eq!((margins.scaled, margins.width, margins.height), ((510, 303), 512, 304));
        assert!(margins.pads());
        let padded =
            exact_sizes(Format::Webm, Target::Sticker, Resize::Pad, (170, 101), thrice).unwrap();
        assert_eq!((padded.width, padded.height), (512, 512));
        let emoji = exact(Target::Emoji, (33, 20), thrice).unwrap();
        assert_eq!((emoji.scaled, emoji.width, emoji.height), ((99, 60), 100, 100));
        // never larger than the box, and only whole blocks when shrinking
        let wrong = exact(Target::Sticker, (300, 100), twice).unwrap_err();
        assert!(wrong.to_string().contains("at most 256 pixels"), "{wrong}");
        assert!(exact(Target::Emoji, (51, 40), twice).is_err());
        assert!(exact(Target::Sticker, (1023, 576), half).is_err());
        assert!(
            exact_sizes(Format::Webm, Target::Sticker, Resize::Crop, (256, 100), twice).is_err()
        );

        let cropped = crop(Some(Crop { x: 0, y: 0, width: 256, height: 144 }));
        let options = Options { integer_scale: Some(twice), ..Default::default() };
        let (plan, _) = plan_chosen(cropped, options).unwrap();
        assert_eq!((plan.width, plan.height, plan.scaling), (512, 288, Scaling::Integer));
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

    #[test]
    fn limits_ffmpegs_threads_where_asked() {
        use std::num::NonZeroU32;
        let words = |plan: &Plan| -> Vec<Vec<String>> {
            let params = super::super::first_params(plan);
            let commands = crate::ffmpeg::commands(plan, &params);
            let word = |arg: &std::ffi::OsString| arg.to_string_lossy().into_owned();
            commands.iter().map(|command| command.iter().map(word).collect()).collect()
        };
        // automatic: ffmpeg's own choice, as it always was
        let (auto, _) = plan_with(Options::default()).unwrap();
        assert_eq!(auto.threads, Threads::Auto);
        for command in words(&auto) {
            assert!(!command.iter().any(|word| word.contains("threads")), "{command:?}");
        }
        // at most three: the filters, the decoder before the input and
        // the encoder after its name, in both passes
        let three = Threads::Limit(NonZeroU32::new(3).unwrap());
        let request = Request { threads: three, ..Request::new(Input::File("in.mp4".into())) };
        let (plan, _) = plan(&request, source(4.84)).unwrap();
        assert_eq!(plan.threads, three);
        let commands = words(&plan);
        assert_eq!(commands.len(), 2);
        for command in &commands {
            let at = |word: &str| command.iter().position(|w| w == word).unwrap();
            let after = |index: usize| command[index + 1].as_str();
            assert_eq!(after(at("-filter_threads")), "3", "{command:?}");
            let threads: Vec<usize> =
                (0..command.len()).filter(|&index| command[index] == "-threads").collect();
            assert_eq!(threads.len(), 2, "{command:?}");
            assert!(threads.iter().all(|&index| after(index) == "3"), "{command:?}");
            assert!(threads[0] < at("-i") && at("-i") < at("libvpx-vp9"), "{command:?}");
            assert_eq!(threads[1], at("libvpx-vp9") + 1, "{command:?}");
        }
        // and the user's own goes over it, after it
        let options = Options {
            encoder_options: Some([("threads".to_owned(), "5".to_owned())].into()),
            ffmpeg_args: Some(vec!["-threads".into(), "6".into()]),
            ..Options::default()
        };
        let request = Request { options, ..request };
        let (planned, _) = super::plan(&request, source(4.84)).unwrap();
        let command = &words(&planned)[1];
        let ours = command.iter().rposition(|word| word == "3").unwrap();
        let theirs = command.iter().position(|word| word == "-threads:v").unwrap();
        assert_eq!(command[theirs + 1], "5");
        let raw = command.iter().rposition(|word| word == "-threads").unwrap();
        assert_eq!(command[raw + 1], "6");
        assert!(ours < theirs && theirs < raw, "{command:?}");
    }
}

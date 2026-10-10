//! Converting a video into a Telegram sticker or emoji.
//!
//! [`convert`] runs synchronously and reports [`Event`]s through a callback.
//! To run it in the background, call it from a thread and cancel it with a
//! [`CancelToken`]:
//!
//! ```no_run
//! # use tgradish_core::{backend::Backend, convert::{convert, Request}, ffmpeg::CancelToken};
//! let backend = Backend::select(Default::default(), None)?;
//! let cancel = CancelToken::new();
//! let request = Request::new("pig.mp4".into());
//! let worker = std::thread::spawn({
//!     let cancel = cancel.clone();
//!     move || convert(&backend, &request, &cancel, &mut |event| println!("{event:?}"))
//! });
//! // cancel.cancel() stops it early
//! let outcome = worker.join().unwrap()?;
//! # Ok::<(), tgradish_core::Error>(())
//! ```

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::backend::{Backend, Pass};
use crate::error::{Error, Result};
use crate::events::{Event, Params, Rate};
use crate::ffmpeg::{CancelToken, Output, Probe};
use crate::fit::{self, Attempt, Encoder};
use crate::options::{self, Crop, Fit, Options, Range, Resize, Speed, Spoof};
use crate::telegram::{self, Issue, Target};
use crate::webm::{self, Patch};

/// A conversion to run.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub input: PathBuf,
    /// Defaults to the input with a `.sticker.webm` or `.emoji.webm`
    /// extension.
    pub output: Option<PathBuf>,
    pub options: Options,
    /// Replace the output if it exists.
    pub overwrite: bool,
    /// Keep intermediate files and report where they are.
    pub keep_temp: bool,
}

impl Request {
    pub fn new(input: PathBuf) -> Self {
        Self {
            input,
            output: None,
            options: Options::default(),
            overwrite: false,
            keep_temp: false,
        }
    }
}

/// Every setting of a conversion, after defaults and input properties are
/// taken into account.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Plan {
    pub input: PathBuf,
    pub output: PathBuf,
    pub source: Probe,
    pub target: Target,
    /// The part of the source used, in its display pixels.
    pub crop: Option<Crop>,
    pub resize: Resize,
    /// Size of the encoded video.
    pub width: u32,
    pub height: u32,
    /// Size the source is scaled to before padding or cropping.
    pub scaled_width: u32,
    pub scaled_height: u32,
    pub fit: Fit,
    pub attempts: u32,
    pub fit_range: Range,
    /// Whether `fit = auto` may lower the frame rate. False when the frame
    /// rate was set explicitly.
    pub auto_fps: bool,
    pub start: f64,
    pub length: f64,
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
    pub extra_args: Vec<String>,
}

impl Plan {
    /// Whether a result of `length` seconds gets its duration spoofed.
    pub fn spoofs(&self, length: f64) -> bool {
        match self.spoof {
            Spoof::Auto => length > telegram::MAX_SECONDS + 1e-3,
            Spoof::Always => true,
            Spoof::Never => false,
        }
    }
}

/// Number of frames encoded for `length` seconds at `fps`. Rounds down so
/// the result never lasts longer than asked, which matters at the 3 second
/// limit, but always at least one frame.
pub fn frame_count(length: f64, fps: f64) -> u64 {
    ((length * fps + 1e-6).floor() as u64).max(1)
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

/// Output, input or option problems detected while planning.
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidOptions(message.into())
}

/// Whether `output` is the same file as `input`, following `..` and links.
fn same_file(input: &Path, output: &Path) -> bool {
    match (std::fs::canonicalize(input), std::fs::canonicalize(output)) {
        (Ok(input), Ok(output)) => input == output,
        _ => input == output,
    }
}

/// Validates a user-given fit range. Integer ranges must contain an integer.
fn check_range(range: Range, fit: Fit, length: f64) -> Result<Range> {
    if !(range.min.is_finite() && range.max.is_finite() && range.min <= range.max) {
        return Err(invalid(format!("fit range {range} must be MIN..MAX with MIN <= MAX")));
    }
    let (low, high, what) = match fit {
        Fit::Crf => (0.0, 63.0, "crf"),
        Fit::Fps => (1.0, telegram::MAX_FPS, "fps"),
        Fit::Length => (f64::MIN_POSITIVE, length, "length"),
        Fit::Auto | Fit::Bitrate | Fit::Off => (1.0, f64::INFINITY, "bitrate"),
    };
    if range.min < low || range.max > high {
        return Err(invalid(match (low, high.is_finite()) {
            (f64::MIN_POSITIVE, _) => format!("{what} range must be above 0 and at most {high}"),
            (_, false) => format!("{what} range must be at least {low}"),
            _ => format!("{what} range must be within {low}..{high}"),
        }));
    }
    if matches!(fit, Fit::Crf | Fit::Fps) && range.min.ceil() > range.max.floor() {
        return Err(invalid(format!("{what} range {range} contains no whole number")));
    }
    Ok(range)
}

fn even(value: f64) -> u32 {
    ((value / 2.0).round() as u32 * 2).max(2)
}

/// Bitrate in kbit/s that should land a bit under `limit` bytes.
pub fn estimate_bitrate(length: f64, limit: u64) -> f64 {
    // leaves room for container overhead and encoder overshoot
    limit as f64 * 8.0 / length / 1000.0 * 0.93
}

pub fn default_output(input: &Path, target: Target) -> PathBuf {
    input.with_extension(format!("{}.webm", target.name()))
}

/// Resolves options against the probed input. Returns the plan and warnings
/// about options that were adjusted or ignored.
pub fn plan(request: &Request, source: Probe) -> Result<(Plan, Vec<String>)> {
    let o = &request.options;
    let mut warnings = Vec::new();

    let target = o.target.unwrap_or_default();
    let resize = o.resize.unwrap_or(if target.requires_exact_size() {
        Resize::Pad
    } else {
        Resize::Contain
    });
    if target.requires_exact_size() && resize == Resize::Contain {
        return Err(invalid(format!(
            "{} must be exactly {}x{}, use resize pad, crop or stretch",
            target.name(),
            target.box_size().0,
            target.box_size().1
        )));
    }

    let output = request.output.clone().unwrap_or_else(|| default_output(&request.input, target));
    if same_file(&request.input, &output) {
        return Err(invalid("output would overwrite the input"));
    }
    if !request.overwrite && output.exists() {
        return Err(Error::OutputExists(output));
    }

    let spoof = o.spoof.unwrap_or(Spoof::Auto);
    let fake_duration = o.fake_duration.unwrap_or(options::DEFAULT_FAKE_DURATION);
    if !(fake_duration > 0.0 && fake_duration <= telegram::MAX_SECONDS) {
        return Err(invalid(format!(
            "fake duration must be more than 0 and at most {} seconds",
            telegram::MAX_SECONDS
        )));
    }

    let start = o.start.unwrap_or(0.0);
    if !(start >= 0.0 && start.is_finite()) {
        return Err(invalid("start must be 0 or more seconds"));
    }
    let available = match source.duration {
        Some(duration) if start >= duration => {
            return Err(invalid(format!("start is past the end of the input ({duration:.2} s)")));
        }
        Some(duration) => duration - start,
        None => f64::INFINITY,
    };
    let mut length = match o.length {
        Some(length) if !(length > 0.0 && length.is_finite()) => {
            return Err(invalid("length must be more than 0 seconds"));
        }
        Some(length) if length > available + 0.01 => {
            warnings.push(format!("the input is only {available:.2} s long after start"));
            available
        }
        Some(length) => length,
        None if available.is_finite() => available,
        None => telegram::MAX_SECONDS,
    };
    if spoof == Spoof::Never && length > telegram::MAX_SECONDS {
        warnings.push(format!(
            "cutting to {} s because spoofing is off; Telegram rejects longer videos",
            telegram::MAX_SECONDS
        ));
        length = telegram::MAX_SECONDS;
    }
    let source_fps = source.fps.unwrap_or(25.0);
    let fps = match o.fps {
        Some(fps) if !(fps > 0.0 && fps <= telegram::MAX_FPS) => {
            return Err(invalid(format!(
                "fps must be more than 0 and at most {}",
                telegram::MAX_FPS
            )));
        }
        Some(fps) => fps,
        None => source_fps.min(telegram::DEFAULT_MAX_FPS),
    };

    let crf = o.crf.unwrap_or(options::DEFAULT_CRF);
    if crf > 63 {
        return Err(invalid("crf must be between 0 and 63"));
    }
    let bitrate = match o.bitrate {
        Some(bitrate) if !(bitrate >= 1.0 && bitrate.is_finite()) => {
            return Err(invalid("bitrate must be at least 1 kbit/s"));
        }
        Some(bitrate) => bitrate,
        None => estimate_bitrate(length, target.max_bytes()),
    };

    let fit = o.fit.unwrap_or(Fit::Auto);
    let attempts = o.attempts.unwrap_or(options::DEFAULT_ATTEMPTS);
    if !(1..=options::MAX_ATTEMPTS).contains(&attempts) {
        return Err(invalid(format!("attempts must be between 1 and {}", options::MAX_ATTEMPTS)));
    }
    let lossless = o.lossless.unwrap_or(false);
    if lossless && !matches!(fit, Fit::Fps | Fit::Length | Fit::Off) {
        return Err(invalid("lossless only works with fit fps, length or off"));
    }
    match fit {
        Fit::Auto | Fit::Bitrate if o.crf.is_some() => {
            warnings.push("crf is ignored while fitting bitrate".into());
        }
        Fit::Crf | Fit::Fps | Fit::Length if o.bitrate.is_some() => {
            warnings.push("bitrate is ignored while fitting at constant quality".into());
        }
        _ => {}
    }

    // length fitting may only shorten: `length` already respects the input
    // and the spoofing policy
    let fit_range = match (o.fit_range, fit) {
        (Some(range), _) if !(range.min.is_finite() && range.max.is_finite()) => {
            return Err(invalid(format!("fit range {range} must be finite")));
        }
        (Some(range), _) if range.min > range.max => {
            return Err(invalid(format!("fit range {range} must have MIN <= MAX")));
        }
        (Some(range), Fit::Length) if range.max > length => {
            warnings.push(format!("length range capped at {length:.2} s"));
            check_range(Range { min: range.min.min(length), max: length }, fit, length)?
        }
        (Some(range), _) => check_range(range, fit, length)?,
        (None, Fit::Auto | Fit::Bitrate | Fit::Off) => Range { min: 8.0, max: 50_000.0 },
        (None, Fit::Crf) => Range { min: 4.0, max: 63.0 },
        (None, Fit::Fps) => Range { min: 1.0, max: fps.floor().max(1.0) },
        (None, Fit::Length) => Range { min: length.min(0.1), max: length },
    };

    let encoder_options = o.encoder_options.clone().unwrap_or_default();
    if let Some(name) = encoder_options.keys().find(|name| !is_option_name(name)) {
        return Err(invalid(format!(
            "{name:?} is not an encoder option name: use letters, digits, - and _"
        )));
    }

    if let Some(crop) = o.crop {
        crop.check(source.width, source.height).map_err(invalid)?;
    }
    let (box_w, box_h) = target.box_size();
    let (box_w, box_h) = (f64::from(box_w), f64::from(box_h));
    let (src_w, src_h) = match o.crop {
        Some(crop) => (f64::from(crop.width), f64::from(crop.height)),
        None => (f64::from(source.width), f64::from(source.height)),
    };
    let ((scaled_width, scaled_height), (width, height)) = match resize {
        Resize::Contain | Resize::Pad => {
            let scale = (box_w / src_w).min(box_h / src_h);
            let scaled =
                (even(src_w * scale).min(box_w as u32), even(src_h * scale).min(box_h as u32));
            let out = if resize == Resize::Pad { (box_w as u32, box_h as u32) } else { scaled };
            (scaled, out)
        }
        Resize::Crop => {
            let scale = (box_w / src_w).max(box_h / src_h);
            let scaled =
                (even(src_w * scale).max(box_w as u32), even(src_h * scale).max(box_h as u32));
            (scaled, (box_w as u32, box_h as u32))
        }
        Resize::Stretch => ((box_w as u32, box_h as u32), (box_w as u32, box_h as u32)),
    };

    let plan = Plan {
        input: request.input.clone(),
        output,
        target,
        crop: o.crop,
        resize,
        width,
        height,
        scaled_width,
        scaled_height,
        fit,
        attempts,
        fit_range,
        auto_fps: o.fps.is_none(),
        start,
        length,
        fps,
        bitrate,
        crf,
        constant_quality: o.crf.is_some() && o.bitrate.is_none(),
        lossless,
        speed: o.speed.unwrap_or(Speed::Balanced),
        alpha: source.alpha || resize == Resize::Pad,
        spoof,
        fake_duration,
        title: o.title.clone(),
        watermark: o.watermark.unwrap_or(true),
        encoder_options,
        extra_args: o.extra_args.clone().unwrap_or_default(),
        source,
    };
    Ok((plan, warnings))
}

/// Result of a finished conversion.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Outcome {
    pub output: PathBuf,
    pub bytes: u64,
    pub params: Params,
    pub spoofed: bool,
    pub issues: Vec<Issue>,
    /// Directory with intermediate files, if they were kept.
    pub temp_dir: Option<PathBuf>,
}

/// Runs fitting attempts through a [`Backend`]. Reuses first-pass logs:
/// libvpx-vp9 first-pass statistics do not depend on the target bitrate.
struct BackendEncoder<'a> {
    backend: &'a Backend,
    plan: &'a Plan,
    dir: &'a Path,
    cancel: &'a CancelToken,
    on_event: &'a mut dyn FnMut(Event),
    attempts: u32,
    pass_logs: HashMap<(u64, u64, bool), PathBuf>,
}

impl BackendEncoder<'_> {
    fn run_pass(
        &mut self,
        attempt: u32,
        (pass_number, passes): (u8, u8),
        params: &Params,
        pass: Pass,
        output: Option<&Path>,
    ) -> Result<()> {
        let length = params.length;
        let on_event = &mut *self.on_event;
        let progress = |fraction| Event::Progress { attempt, pass: pass_number, passes, fraction };
        on_event(progress(0.0));
        self.backend.encode(self.plan, params, pass, output, self.cancel, &mut |output| {
            match output {
                Output::Time { micros } => {
                    on_event(progress((micros as f64 / 1e6 / length).clamp(0.0, 1.0)));
                }
                Output::Line(line) => on_event(Event::Log { line }),
                // other -progress keys
                Output::Stdout(_) => {}
            }
        })?;
        on_event(progress(1.0));
        Ok(())
    }
}

impl Encoder for BackendEncoder<'_> {
    fn encode(&mut self, params: Params) -> Result<Attempt> {
        self.cancel.check()?;
        self.attempts += 1;
        let attempt = self.attempts;
        (self.on_event)(Event::AttemptStarted { attempt, params });
        let path = self.dir.join(format!("attempt-{attempt:02}.webm"));

        if params.rate == Rate::Lossless {
            self.run_pass(attempt, (1, 1), &params, Pass::Single, Some(&path))?;
        } else {
            let key = (
                params.fps.to_bits(),
                params.length.to_bits(),
                matches!(params.rate, Rate::Crf(_)),
            );
            let cached = self.pass_logs.get(&key).cloned();
            let passes = if cached.is_some() { 1 } else { 2 };
            let log = match cached {
                Some(log) => log,
                None => {
                    let log = self.dir.join(format!("pass-{}", self.pass_logs.len()));
                    self.run_pass(attempt, (1, passes), &params, Pass::First(&log), None)?;
                    self.pass_logs.insert(key, log.clone());
                    log
                }
            };
            self.run_pass(attempt, (passes, passes), &params, Pass::Second(&log), Some(&path))?;
        }

        let bytes = std::fs::metadata(&path)?.len();
        let fits = bytes <= self.plan.target.max_bytes();
        (self.on_event)(Event::AttemptFinished { attempt, params, bytes, fits });
        Ok(Attempt { number: attempt, params, bytes, path })
    }

    fn score(&mut self, attempt: &Attempt) -> Result<f64> {
        let on_event = &mut *self.on_event;
        let fps = attempt.params.fps;
        let ssim = self.backend.ssim(self.plan, &attempt.path, fps, self.cancel, &mut |line| {
            on_event(Event::Log { line })
        })?;
        on_event(Event::Scored { attempt: attempt.number, ssim });
        Ok(ssim)
    }
}

/// Converts a video into a Telegram sticker or emoji.
pub fn convert(
    backend: &Backend,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let source = backend.probe(&request.input, cancel)?;
    let (plan, warnings) = plan(request, source)?;
    if !plan.extra_args.is_empty() && !backend.supports_extra_args() {
        return Err(Error::InvalidOptions(
            "extra-args are ffmpeg command line arguments, so they need ffmpeg as a \
             separate program: use --ffmpeg-from system or --ffmpeg PATH, or \
             --encoder-options for encoder settings, which work with the built-in ffmpeg"
                .into(),
        ));
    }
    for message in warnings {
        on_event(Event::Warning { message });
    }
    on_event(Event::Started { plan: Box::new(plan.clone()) });

    let temp = tempfile::Builder::new().prefix("tgradish-").tempdir()?;
    let mut encoder = BackendEncoder {
        backend,
        plan: &plan,
        dir: temp.path(),
        cancel,
        on_event: &mut *on_event,
        attempts: 0,
        pass_logs: HashMap::new(),
    };
    let best = fit::run(&plan, &mut encoder, plan.target.max_bytes())?;
    cancel.check()?;
    let spoofed = plan.spoofs(encoded_length(best.params.length, best.params.fps));

    let changes = Patch {
        duration: spoofed.then_some(plan.fake_duration),
        muxing_app: plan.watermark.then(|| crate::TOOL_ID.to_string()),
        signature: plan.watermark.then(crate::signature),
        ..Default::default()
    };
    let mut bytes = std::fs::read(&best.path)?;
    if changes != Patch::default() {
        webm::patch(&mut bytes, &changes)?;
    }
    // checked again here: the output may have appeared while encoding
    crate::fsutil::write_file(&plan.output, &bytes, request.overwrite).map_err(|err| match err
        .kind()
    {
        std::io::ErrorKind::AlreadyExists => Error::OutputExists(plan.output.clone()),
        _ => Error::Io(err),
    })?;

    let info = webm::inspect_file(&plan.output)?;
    let issues = telegram::check(&info, plan.target);
    let temp_dir = request.keep_temp.then(|| temp.keep());
    on_event(Event::Finished {
        output: plan.output.clone(),
        bytes: info.file_size,
        attempt: best.number,
        params: best.params,
        spoofed,
        issues: issues.clone(),
    });
    Ok(Outcome {
        output: plan.output,
        bytes: info.file_size,
        params: best.params,
        spoofed,
        issues,
        temp_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(duration: f64) -> Probe {
        Probe {
            format: "mov,mp4".into(),
            codec: "h264".into(),
            width: 640,
            height: 480,
            fps: Some(30.0),
            duration: Some(duration),
            alpha: false,
            still_image: false,
            orientation: Default::default(),
            decoder: None,
        }
    }

    fn plan_with(options: Options) -> Result<(Plan, Vec<String>)> {
        let request = Request { options, ..Request::new("in.mp4".into()) };
        plan(&request, source(4.84))
    }

    #[test]
    fn refuses_to_overwrite_input_through_dot_dot() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        std::fs::write(&input, b"").unwrap();
        let name = dir.path().file_name().unwrap();
        let sneaky = dir.path().join("..").join(name).join("input.mp4");
        let request =
            Request { output: Some(sneaky), overwrite: true, ..Request::new(input.clone()) };
        assert!(matches!(plan(&request, source(4.84)), Err(Error::InvalidOptions(_))));
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
            filter
                .contains("crop=w=iw*300/640:h=ih*100/480:x=iw*100/640:y=ih*40/480,scale=512:170"),
            "{filter}"
        );
        let outside = Some(Crop { x: 400, y: 0, width: 300, height: 100 });
        assert!(plan_with(Options { crop: outside, ..Default::default() }).is_err());
    }

    #[test]
    fn spoofs_by_result_length() {
        let (plan, _) = plan_with(Options::default()).unwrap();
        assert_eq!(plan.spoof, Spoof::Auto);
        assert!(plan.spoofs(4.84) && !plan.spoofs(3.0));
    }
}

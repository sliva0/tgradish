//! Static WebP stickers and custom emoji: encodes an image or single video
//! frame as WebP (which `@Stickers` accepts for static sets alongside PNG),
//! losslessly when it fits Telegram's 512 KiB limit, or lossy at the highest
//! quality that fits.
//!
//! Takes [`Options`], those of [`crate::webm::Options`] a still picture has
//! (resize, scaling, integer scale and watermark), and of what is one file's
//! own, its [`Selection`] but the length and sprite sheet (the target, crop,
//! start timestamp, Aseprite tag and layers, and title), and plans as WebM
//! does.

mod encode;
mod inspect;
mod options;

pub use encode::Encoding;
#[cfg(test)]
pub(crate) use encode::encode;
use encode::{fit, with_xmp, xmp};
pub use inspect::{Info, creator_tool, inspect_file, preview};
pub use options::Options;

use std::path::PathBuf;

use crate::CancelToken;
use crate::backend::Backend;
use crate::config::Threads;
use crate::error::{Error, Result};
use crate::events::{Details, Event, Outcome, StartDetails, Started};
use crate::input::Input;
use crate::options::Selection;
use crate::picture;
use crate::presets::Format;
use crate::telegram;
use crate::webm::{self, Plan};

/// Parameters for a static WebP conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Input file or image sequence.
    pub input: Input,
    /// Destination path; defaults to [`Input::default_output`] (see
    /// [`Request::output`]).
    pub output: Option<PathBuf>,
    /// What of the input is read, and the target and title; a picture has
    /// no length, nor a sprite sheet.
    pub selection: Selection,
    /// Conversion options.
    pub options: Options,
    /// Replace an existing output file if present.
    pub overwrite: bool,
    /// Maximum threads for ffmpeg frame decoding (`libwebp` encodes
    /// single-threaded).
    pub threads: Threads,
}

impl Request {
    /// Creates a request with default options writing beside `input`, without
    /// overwriting existing files.
    pub fn new(input: Input) -> Self {
        let threads = Threads::Auto;
        let (selection, options) = (Selection::default(), Options::default());
        Self { input, output: None, selection, options, overwrite: false, threads }
    }

    /// Returns the explicit output path, or the default path beside the input.
    pub fn output(&self) -> PathBuf {
        let target = self.selection.target.unwrap_or_default();
        self.output.clone().unwrap_or_else(|| self.input.default_output(Format::Webp, target))
    }
}

/// Computes the seek timestamp for the frame displayed at `time` (snapping to
/// frames starting within [`webm::edge_leeway`], and seeking slightly early via
/// [`webm::read_from`] so ffmpeg does not skip past the target frame). Uses
/// explicit packet `starts` when available, or assumes uniform `1 / fps`
/// spacing.
pub fn frame_start(time: f64, starts: &[f64], fps: f64) -> f64 {
    let (start, previous) = if starts.is_empty() {
        let nearest = (time * fps).round();
        let on_edge = (time - nearest / fps).abs() <= webm::CUT_LEEWAY.min(0.25 / fps);
        let index = if on_edge { nearest } else { (time * fps).floor() };
        (index / fps, (index - 1.0) / fps)
    } else {
        let shown = starts.partition_point(|&start| start <= time).saturating_sub(1);
        match webm::frame_edge(time, starts).unwrap_or(shown) {
            0 => return 0.0,
            index => (starts[index], starts[index - 1]),
        }
    };
    if start <= 0.0 { 0.0 } else { webm::read_from(start, previous) }
}

/// Plans a static WebP conversion (resolving frame seek time, dimensions and
/// `.webp` output path). `starts` supplies packet timestamps when known (from
/// the caller's preview or probe; pass `&[]` to use `source.fps`).
pub fn plan(
    request: &Request,
    source: crate::ffmpeg::Probe,
    starts: &[f64],
) -> Result<(Plan, Vec<String>)> {
    let options = webm::Options::from(request.options.clone());
    let mut selection = request.selection.clone().for_format(Format::Webp);
    // Resolve the seek timestamp for the frame displayed at `start` (matching
    // preview); out-of-range timestamps are rejected later by `webm::plan_for`.
    if let (Some(start), Some(fps)) = (selection.start, source.fps.filter(|_| !source.still_image))
        && start >= 0.0
        && source.duration.is_none_or(|duration| start < duration)
    {
        selection.start = Some(frame_start(start, starts, fps));
    }
    let request = webm::Request {
        input: request.input.clone(),
        output: Some(request.output()),
        selection,
        options,
        overwrite: request.overwrite,
        keep_temp: false,
        threads: request.threads,
    };
    let (mut plan, warnings) = webm::plan_for(Format::Webp, &request, source)?;
    // Single frame duration.
    plan.length = 1.0 / plan.fps;
    Ok((plan, warnings))
}

/// Converts a single frame at `start` into a static WebP sticker or emoji.
pub fn convert(
    backend: &Backend,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    request.input.check_output(&request.output(), request.overwrite)?;
    // Render pixel-art inputs for ffmpeg using picture-only options (keeping
    // default frame timing for sequences because frame selection is
    // time-based).
    let drawing = webm::Options::from(request.options.clone());
    let mut readable =
        crate::drawn::readable(&request.input, &request.selection, &drawing, cancel)?;
    // Drop the in-memory raster frames since ffmpeg reads from disk.
    drop(readable.take_drawn());
    let input = readable.path();
    let source = readable.probe(backend, cancel)?;
    // Probe packet start timestamps to locate the frame shown at `start`,
    // falling back to uniform frame spacing if unavailable.
    let starts = match request.selection.start {
        Some(_) if !source.still_image => match backend.frame_starts(input, &source, cancel) {
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            starts => starts.unwrap_or_default(),
        },
        _ => Vec::new(),
    };
    let (mut plan, warnings) = plan(request, source, &starts)?;
    on_event(Event::Started(Started {
        input: request.input.first().to_path_buf(),
        output: plan.output.clone(),
        format: Format::Webp,
        target: plan.target,
        limit: Format::Webp.max_bytes(plan.target),
        details: StartDetails::Webp {},
    }));
    for message in warnings {
        on_event(Event::Warning { message });
    }
    plan.input = input.to_path_buf();
    picture::choose_auto_scaling(backend, &mut plan, &drawing, cancel)?;
    let rgba = backend.still(&plan, cancel)?;
    cancel.check()?;
    let alpha = rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255);
    let xmp = (plan.watermark || plan.title.is_some())
        .then(|| xmp(plan.title.as_deref(), plan.watermark));
    let finish = |webp: Vec<u8>| match &xmp {
        Some(xmp) => with_xmp(&webp, xmp, (plan.width, plan.height), alpha),
        None => webp,
    };
    let (webp, encoding) = fit(&rgba, plan.width, plan.height, finish)?;
    // WebP encoding cannot be interrupted mid-frame, so check cancellation
    // before writing the output file.
    cancel.check()?;
    crate::fsutil::write_output(&plan.output, &webp, request.overwrite)?;
    let bytes = webp.len() as u64;
    let outcome = Outcome {
        input: request.input.first().to_path_buf(),
        output: plan.output.clone(),
        format: Format::Webp,
        target: plan.target,
        bytes,
        limit: Format::Webp.max_bytes(plan.target),
        changed: matches!(encoding, Encoding::Lossy { .. }),
        issues: telegram::check_still(plan.width, plan.height, bytes, plan.target),
        details: Details::Webp { width: plan.width, height: plan.height, encoding },
    };
    on_event(Event::Finished(outcome.clone()));
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_where_to_read_for_a_frame() {
        let close = |found: f64, expected: f64| (found - expected).abs() < 1e-9;
        // Uniform 10 fps: seeks 1 ms before the start of the active frame.
        assert_eq!(frame_start(0.0, &[], 10.0), 0.0);
        assert_eq!(frame_start(0.05, &[], 10.0), 0.0);
        assert!(close(frame_start(0.25, &[], 10.0), 0.199));
        // Snaps to a frame boundary when float rounding or truncated decimals
        // land just before it.
        assert!(close(frame_start(0.3 - 1e-9, &[], 10.0), 0.299));
        assert!(close(frame_start(0.29996, &[], 10.0), 0.299));
        assert!(close(frame_start(0.2998, &[], 10.0), 0.199));
        let thirds = [0.0, 1.0 / 3.0, 2.0 / 3.0];
        assert!(close(frame_start(0.33333, &thirds, 3.0), 1.0 / 3.0 - 1e-3));
        assert!(close(frame_start(0.33333, &[], 3.0), 1.0 / 3.0 - 1e-3));
        assert!(close(frame_start(0.3332, &thirds, 3.0), 0.0));
        // Explicit packet starts: first frame, very short gap, and long frame.
        let starts = [0.0, 0.1, 0.1002, 0.7];
        assert_eq!(frame_start(0.05, &starts, 10.0), 0.0);
        assert!(close(frame_start(0.1, &starts, 10.0), 0.099));
        // Quarter of a gap shorter than 4 ms.
        assert!(close(frame_start(0.5, &starts, 10.0), 0.10015));
        assert!(close(frame_start(9.0, &starts, 10.0), 0.699));
        // The first known packet always seeks from 0.0, whereas uniform fps
        // seeks to the second frame interval at 0.04 s.
        assert_eq!(frame_start(0.05, &[0.04], 25.0), 0.0);
        assert_eq!(frame_start(0.05, &[], 25.0), 0.04 - 1e-3);
    }
}

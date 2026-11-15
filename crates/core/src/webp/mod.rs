//! Static stickers and emoji: one frame of a video, or a picture, as a
//! WebP, which @Stickers takes for its static sets as it takes PNG:
//! lossless where that fits the size limit, otherwise lossy at the best
//! quality that does.
//!
//! The picture is planned like a WebM sticker's, from the options a still
//! picture has: target, crop, resize, scaling and integer scale, `start`,
//! where the frame is, and an Aseprite file's tag and layers.

mod encode;
mod inspect;

/// An image takes WebM's options, of which it uses [`PICTURE_OPTIONS`].
pub use crate::webm::Options;
pub use encode::Encoding;
#[cfg(test)]
pub(crate) use encode::encode;
use encode::{fit, with_xmp, xmp};
pub use inspect::{Info, creator_tool, inspect_file, preview};

use std::path::PathBuf;

use crate::CancelToken;
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::events::{Details, Event, Outcome, StartDetails, Started};
use crate::input::Input;
use crate::picture;
use crate::presets::Format;
use crate::telegram;
use crate::webm::{self, Plan};

/// A picture to make.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// What to make it of.
    pub input: Input,
    /// Where the result goes: by default [`Input::default_output`], as
    /// [`Request::output`] gives it.
    pub output: Option<PathBuf>,
    /// How to make it: WebM's options, of which a picture uses some.
    pub options: Options,
    /// Replace the output if it exists.
    pub overwrite: bool,
}

impl Request {
    /// A picture of `input` with the default options, beside it, not
    /// replacing one there.
    pub fn new(input: Input) -> Self {
        Self { input, output: None, options: Options::default(), overwrite: false }
    }

    /// Where the result goes, as asked or by default.
    pub fn output(&self) -> PathBuf {
        let target = self.options.target.unwrap_or_default();
        self.output.clone().unwrap_or_else(|| self.input.default_output(Format::Webp, target))
    }
}

/// The options a still picture has, of WebM's: what to make, what is read
/// and where, how it is squared and scaled, and its metadata. The rest are
/// about time and encoding video, which a picture has none of.
pub const PICTURE_OPTIONS: [&str; 11] = [
    "target",
    "tag",
    "show-layers",
    "hide-layers",
    "crop",
    "resize",
    "scaling",
    "integer-scale",
    "start",
    "title",
    "watermark",
];

/// The options of `options` a still picture has, [`PICTURE_OPTIONS`].
pub fn picture_options(options: &Options) -> Options {
    let mut table = toml::Table::try_from(options).expect("options serialise to a table");
    table.retain(|key, _| PICTURE_OPTIONS.contains(&key));
    table.try_into().expect("options read back with fewer set")
}

/// Where to start reading for the frame that shows at `time`, or that
/// starts within [`webm::edge_leeway`] after it, as a time written with a
/// few digits is meant. `starts` are where frames start, in order; without
/// them, frames come every `1 / fps` seconds.
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

/// Plans the picture: its frame, a sticker's or emoji's size, and the
/// output, `.webp` unless `request` names one. `starts` are where the
/// input's frames start, when they are known.
pub fn plan(
    request: &Request,
    source: crate::ffmpeg::Probe,
    starts: &[f64],
) -> Result<(Plan, Vec<String>)> {
    let mut options = picture_options(&request.options);
    // the frame that shows at `start`, as previews show it; before the
    // start or past the end, planning says so
    if let (Some(start), Some(fps)) = (options.start, source.fps.filter(|_| !source.still_image))
        && start >= 0.0
        && source.duration.is_none_or(|duration| start < duration)
    {
        options.start = Some(frame_start(start, starts, fps));
    }
    let request = webm::Request {
        input: request.input.clone(),
        output: Some(request.output()),
        options,
        overwrite: request.overwrite,
        keep_temp: false,
    };
    let (mut plan, warnings) = webm::plan(&request, source)?;
    // one frame
    plan.length = 1.0 / plan.fps;
    Ok((plan, warnings))
}

/// Makes a static sticker or emoji from the frame at `start`.
pub fn convert(
    backend: &Backend,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    request.input.check_output(&request.output(), request.overwrite)?;
    // pixel art, drawn for ffmpeg with what a picture has: a sequence's
    // frames timed as by default, as frames are picked by time
    let drawing = picture_options(&request.options);
    let mut readable = crate::drawn::readable(&request.input, &drawing, cancel)?;
    // ffmpeg reads them from the file: not kept while it does
    drop(readable.take_drawn());
    let input = readable.path();
    let source = readable.probe(backend, cancel)?;
    // where frames start, to find the one that shows at a time; without
    // them, frames count as coming evenly, as before they were read
    let starts = match request.options.start {
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
    picture::choose_auto_scaling(backend, &mut plan, &request.options, cancel)?;
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
    // encoding can't stop part of the way, but a stopped one isn't kept
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
    on_event(Event::Finished(Box::new(outcome.clone())));
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_where_to_read_for_a_frame() {
        let close = |found: f64, expected: f64| (found - expected).abs() < 1e-9;
        // evenly, 10 a second: a millisecond before the frame showing
        assert_eq!(frame_start(0.0, &[], 10.0), 0.0);
        assert_eq!(frame_start(0.05, &[], 10.0), 0.0);
        assert!(close(frame_start(0.25, &[], 10.0), 0.199));
        // on a frame's start, which float error or few digits leave a
        // hair short of
        assert!(close(frame_start(0.3 - 1e-9, &[], 10.0), 0.299));
        assert!(close(frame_start(0.29996, &[], 10.0), 0.299));
        assert!(close(frame_start(0.2998, &[], 10.0), 0.199));
        let thirds = [0.0, 1.0 / 3.0, 2.0 / 3.0];
        assert!(close(frame_start(0.33333, &thirds, 3.0), 1.0 / 3.0 - 1e-3));
        assert!(close(frame_start(0.33333, &[], 3.0), 1.0 / 3.0 - 1e-3));
        assert!(close(frame_start(0.3332, &thirds, 3.0), 0.0));
        // where frames start: the first, and short and long frames
        let starts = [0.0, 0.1, 0.1002, 0.7];
        assert_eq!(frame_start(0.05, &starts, 10.0), 0.0);
        assert!(close(frame_start(0.1, &starts, 10.0), 0.099));
        // a quarter of a gap shorter than four milliseconds
        assert!(close(frame_start(0.5, &starts, 10.0), 0.10015));
        assert!(close(frame_start(9.0, &starts, 10.0), 0.699));
        // the first of the frames known is read from the start, unlike
        // the second of frames counted evenly
        assert_eq!(frame_start(0.05, &[0.04], 25.0), 0.0);
        assert_eq!(frame_start(0.05, &[], 25.0), 0.04 - 1e-3);
    }
}

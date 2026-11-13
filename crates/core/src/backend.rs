//! The two ways of running ffmpeg: as separate `ffmpeg` and `ffprobe`
//! processes, or through libraries linked into the binary (the `linked`
//! feature).

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;

use crate::CancelToken;
use crate::convert::Plan;
use crate::error::{Error, Result};
use crate::events::Params;
use crate::ffmpeg::{self, Capabilities, Ffmpeg, FfmpegChoice, Output, Probe};

/// Pass of a libvpx encode. Two-pass encodes share a statistics file.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Pass<'a> {
    Single,
    First(&'a Path),
    Second(&'a Path),
}

/// `path` from the root, as ffmpeg is given every file it reads. It takes
/// a name as a URL, so in one relative to the working directory what
/// comes before a colon, as in `2026-11-12T10:30:00.png`, is read as a
/// protocol, and its programs take one that starts with a dash for an
/// option. From the root a name starts with neither, on Windows with a
/// drive's letter, which ffmpeg knows. Links are left as they are, and so
/// is a path that can't be made absolute, as an empty one.
fn absolute(path: &Path) -> Cow<'_, Path> {
    if path.is_absolute() {
        return Cow::Borrowed(path);
    }
    std::path::absolute(path).map_or(Cow::Borrowed(path), Cow::Owned)
}

/// `plan` with its input as [`absolute`] makes it.
fn reading_absolute(plan: &Plan) -> Cow<'_, Plan> {
    match absolute(&plan.input) {
        Cow::Borrowed(_) => Cow::Borrowed(plan),
        Cow::Owned(input) => Cow::Owned(Plan { input, ..plan.clone() }),
    }
}

/// `result` with its error naming `given` again, where it names the path
/// [`absolute`] made of it: errors name files as the user did.
fn naming<T>(result: Result<T>, given: &Path, read: &Path) -> Result<T> {
    let named = |path: PathBuf| if path == read { given.to_path_buf() } else { path };
    result.map_err(|err| match err {
        Error::Probe { path, message } => Error::Probe { path: named(path), message },
        Error::NoVideo(path) => Error::NoVideo(named(path)),
        Error::NoFrame { path, at } => Error::NoFrame { path: named(path), at },
        err => err,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    /// ffmpeg and ffprobe executables.
    Process(Ffmpeg),
    /// ffmpeg libraries linked into tgradish.
    #[cfg(feature = "linked")]
    Linked,
}

/// Which backend is used, for status output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BackendInfo {
    Process {
        #[serde(flatten)]
        ffmpeg: Ffmpeg,
    },
    Builtin,
}

impl Backend {
    /// Picks the backend for `choice`, or the executables at `path`.
    pub fn select(choice: FfmpegChoice, path: Option<&Path>) -> Result<Backend> {
        if path.is_some() {
            return Ok(Backend::Process(ffmpeg::locate(choice, path)?));
        }
        match choice {
            #[cfg(feature = "linked")]
            FfmpegChoice::Auto | FfmpegChoice::Builtin => Ok(Backend::Linked),
            #[cfg(not(feature = "linked"))]
            FfmpegChoice::Builtin => {
                Err(Error::FfmpegNotFound(crate::error::FfmpegMissing::NotBuiltIn))
            }
            choice => Ok(Backend::Process(ffmpeg::locate(choice, None)?)),
        }
    }

    /// Whether `extra_args` can be used: they are ffmpeg command line
    /// arguments, and the built-in ffmpeg has no command line.
    pub fn supports_extra_args(&self) -> bool {
        matches!(self, Backend::Process(_))
    }

    pub fn info(&self) -> BackendInfo {
        match self {
            Backend::Process(ffmpeg) => BackendInfo::Process { ffmpeg: ffmpeg.clone() },
            #[cfg(feature = "linked")]
            Backend::Linked => BackendInfo::Builtin,
        }
    }

    pub fn probe(&self, input: &Path, cancel: &CancelToken) -> Result<Probe> {
        let read = absolute(input);
        let probe = match self {
            Backend::Process(ffmpeg) => ffmpeg::probe(ffmpeg, &read, cancel),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::probe(&read, cancel),
        };
        naming(probe, input, &read)
    }

    /// Where the frames of a video, `probe` probed, start, in order, in
    /// seconds into it as cuts count them: empty when its packets carry no
    /// times.
    pub fn frame_starts(
        &self,
        input: &Path,
        probe: &Probe,
        cancel: &CancelToken,
    ) -> Result<Vec<f64>> {
        let read = absolute(input);
        let in_file = match self {
            Backend::Process(ffmpeg) => ffmpeg::frame_starts(ffmpeg, &read, cancel),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::frame_starts(&read, cancel),
        };
        let in_file = naming(in_file, input, &read)?;
        Ok(in_video(in_file, probe.start))
    }

    pub fn capabilities(&self, cancel: &CancelToken) -> Result<Capabilities> {
        match self {
            Backend::Process(ffmpeg) => ffmpeg::capabilities(ffmpeg, cancel),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::capabilities(),
        }
    }

    /// Encodes one attempt, or runs the first pass when `output` is `None`.
    pub(crate) fn encode(
        &self,
        plan: &Plan,
        params: &Params,
        pass: Pass,
        output: Option<&Path>,
        cancel: &CancelToken,
        on_output: &mut dyn FnMut(Output),
    ) -> Result<()> {
        let read = reading_absolute(plan);
        let encoded = match self {
            Backend::Process(ffmpeg) => {
                let pass = match pass {
                    Pass::Single => None,
                    Pass::First(log) => Some((1, log)),
                    Pass::Second(log) => Some((2, log)),
                };
                let mut cmd = Command::new(&ffmpeg.ffmpeg);
                cmd.args(ffmpeg::encode_args(&read, params, pass, output));
                ffmpeg::run(cmd, "ffmpeg", cancel, on_output).map(drop)
            }
            #[cfg(feature = "linked")]
            Backend::Linked => {
                crate::linked::encode(&read, params, pass, output, cancel, on_output)
            }
        };
        naming(encoded, &plan.input, &read.input)
    }

    /// Frames of a video or image for showing it, see [`FramesRequest`].
    pub fn frames(
        &self,
        path: &Path,
        probe: &Probe,
        request: &FramesRequest,
        cancel: &CancelToken,
    ) -> Result<Frames> {
        let (start, fps, count) = spacing(probe, request);
        let (width, height) = scaled(probe, request.max_side);
        let read = absolute(path);
        let frames = match self {
            Backend::Process(ffmpeg) => {
                ffmpeg::frames(ffmpeg, &read, probe, (start, fps), (width, height), count, cancel)
            }
            #[cfg(feature = "linked")]
            Backend::Linked => {
                crate::linked::frames(&read, probe, (start, fps), (width, height), count, cancel)
            }
        };
        let frames = naming(frames, path, &read)?;
        Ok(Frames { width, height, start, fps, frames })
    }

    /// A finished sticker, decoded for a preview: straight RGBA at most
    /// `max_side` pixels on the longer side, each frame with how many 60 fps
    /// frames it shows for.
    pub fn preview(
        &self,
        path: &Path,
        max_side: u32,
        cancel: &CancelToken,
    ) -> Result<crate::preview::Preview> {
        let probe = self.probe(path, cancel)?;
        let fps = probe.fps_or_default().clamp(1.0, crate::preview::TICKS);
        // spoofed stickers say in their header that they are shorter than
        // they play; their frames tell the truth
        let played = crate::webm::inspect_file(path).ok().and_then(|info| info.content_duration);
        let longer = probe.width.max(probe.height).max(1);
        let scale = (f64::from(max_side) / f64::from(longer)).min(1.0);
        let frame_bytes = (f64::from(probe.width) * scale).ceil()
            * (f64::from(probe.height) * scale).ceil()
            * 4.0;
        let request = FramesRequest {
            start: 0.0,
            length: Some(played.or(probe.duration).unwrap_or(UNKNOWN_SECONDS)),
            fps,
            max_side,
            // long ones are shown at a lower frame rate
            max_frames: ((PREVIEW_BYTES as f64 / frame_bytes.max(1.0)) as usize).max(1),
        };
        let frames = self.frames(path, &probe, &request, cancel)?;
        let ticks = ticks(frames.frames.len(), frames.fps);
        let rgba = frames.frames.into_iter().zip(ticks).collect();
        Ok(crate::preview::Preview { width: frames.width, height: frames.height, frames: rgba })
    }

    /// The planned frame at the plan's start, cropped, scaled and padded as
    /// the plan says: straight RGBA of the plan's size, for image stickers.
    pub fn still(&self, plan: &Plan, cancel: &CancelToken) -> Result<Vec<u8>> {
        let read = reading_absolute(plan);
        let still = match self {
            Backend::Process(ffmpeg) => ffmpeg::still(ffmpeg, &read, cancel),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::still(&read, cancel),
        };
        naming(still, &plan.input, &read.input)
    }

    /// Compares an encoded attempt at `fps` with the source at the planned
    /// frame rate. The source drives the comparison and the attempt's
    /// frames are repeated in between, so lower frame rates lose points on
    /// frames they skip. Returns SSIM from 0 to 1, ignoring alpha.
    pub fn ssim(
        &self,
        plan: &Plan,
        candidate: &Path,
        fps: f64,
        cancel: &CancelToken,
        on_line: &mut dyn FnMut(String),
    ) -> Result<f64> {
        let (read, candidate) = (reading_absolute(plan), absolute(candidate));
        let ssim = match self {
            Backend::Process(ffmpeg) => {
                ffmpeg::ssim(ffmpeg, &read, &candidate, fps, cancel, on_line)
            }
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::ssim(&read, &candidate, fps, cancel),
        };
        naming(ssim, &plan.input, &read.input)
    }
}

/// Where frames of a video `probe` probed start, `in_file` seconds after the
/// file's start as its packets come, in seconds into the video, in order.
fn in_video(in_file: Vec<f64>, video_start: f64) -> Vec<f64> {
    let mut starts: Vec<f64> = in_file.into_iter().map(|start| start - video_start).collect();
    // packets come in decoding order, and decoders drop those before
    // the start
    starts.retain(|start| start.is_finite() && *start > -1e-6);
    starts.sort_by(f64::total_cmp);
    starts.dedup_by(|later, earlier| *later - *earlier < 1e-6);
    starts
}

/// Where the frames `request` asks of a video `probe` probed start, how
/// many a second, and how many: as asked, at most the video's own rate
/// and as many as asked for, spread over the length; one of a picture,
/// or where nothing is left to read.
fn spacing(probe: &Probe, request: &FramesRequest) -> (f64, f64, usize) {
    let start = if probe.still_image { 0.0 } else { request.start.max(0.0) };
    let length = match (request.length, probe.duration) {
        (Some(length), _) => length,
        (None, Some(duration)) => duration - start,
        (None, None) => 0.0,
    };
    let source_fps = probe.fps_or_default();
    let max_frames = request.max_frames.max(1);
    if probe.still_image || length <= 0.0 {
        return (start, source_fps, 1);
    }
    let fps = request.fps.min(source_fps).min(max_frames as f64 / length).max(1e-3);
    (start, fps, ((length * fps).ceil() as usize).clamp(1, max_frames))
}

/// The size of a video `probe` probed with at most `max_side` pixels on its
/// longer side, never larger, and at least a pixel each way.
fn scaled(probe: &Probe, max_side: u32) -> (u32, u32) {
    let longer = probe.width.max(probe.height).max(1);
    let scale = (f64::from(max_side) / f64::from(longer)).min(1.0);
    let size = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (size(probe.width), size(probe.height))
}

/// How many 60 fps frames each of `count` frames `fps` a second shows for:
/// whole ones, rounding where each starts, so they add up to the time they
/// play, and at least one.
fn ticks(count: usize, fps: f64) -> Vec<u32> {
    let start = |index: usize| (index as f64 * crate::preview::TICKS / fps).round() as u32;
    (0..count).map(|index| (start(index + 1) - start(index)).max(1)).collect()
}

/// Memory for a preview's frames.
const PREVIEW_BYTES: usize = 256 << 20;
/// How much of a sticker of unknown length previews show, in seconds.
const UNKNOWN_SECONDS: f64 = 3.0;

/// Which frames of a video to decode, and how small, for showing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramesRequest {
    /// Seconds into the input.
    pub start: f64,
    /// Seconds to decode; the rest of the input if `None`.
    pub length: Option<f64>,
    /// At most this many frames per second, and at most the input's.
    pub fps: f64,
    /// At most this many pixels on the longer side.
    pub max_side: u32,
    /// At most this many frames, spread over the length by lowering the
    /// frame rate.
    pub max_frames: usize,
}

/// Evenly spaced frames of a video, as straight RGBA in its display
/// orientation.
#[derive(Debug, Clone, PartialEq)]
pub struct Frames {
    pub width: u32,
    pub height: u32,
    /// Seconds into the input of the first frame.
    pub start: f64,
    /// Frames per second: the spacing of the frames.
    pub fps: f64,
    pub frames: Vec<Vec<u8>>,
}

/// Statistics file that ffmpeg's `-passlogfile PREFIX` uses.
#[cfg(feature = "linked")]
pub(crate) fn pass_log_file(prefix: &Path) -> std::path::PathBuf {
    let mut name = prefix.as_os_str().to_owned();
    name.push("-0.log");
    std::path::PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(duration: Option<f64>, fps: Option<f64>, still_image: bool) -> Probe {
        Probe {
            format: "matroska,webm".into(),
            codec: "vp9".into(),
            width: 640,
            height: 360,
            fps,
            base_fps: fps,
            duration,
            start: 0.0,
            alpha: false,
            still_image,
            orientation: Default::default(),
            decoder: None,
        }
    }

    fn request(start: f64, length: Option<f64>, fps: f64, max_frames: usize) -> FramesRequest {
        FramesRequest { start, length, fps, max_side: 320, max_frames }
    }

    #[test]
    fn spaces_frames_over_what_is_read() {
        let video = probe(Some(10.0), Some(25.0), false);
        // as asked, from the start, to the end
        assert_eq!(spacing(&video, &request(2.0, None, 10.0, 100)), (2.0, 10.0, 80));
        // fewer a second, to make no more than are asked for
        assert_eq!(spacing(&video, &request(1.0, Some(1.0), 10.0, 5)), (1.0, 5.0, 5));
        // never more than the video's own, and from no earlier than its start
        assert_eq!(spacing(&video, &request(-3.0, Some(1.0), 60.0, 100)), (0.0, 25.0, 25));
        // a part of a frame is a frame
        assert_eq!(spacing(&video, &request(0.0, Some(0.05), 25.0, 100)), (0.0, 25.0, 2));
        // one, where nothing is left, or nothing says how long it is
        assert_eq!(spacing(&video, &request(12.0, None, 10.0, 100)), (12.0, 25.0, 1));
        let unknown = probe(None, None, false);
        assert_eq!(spacing(&unknown, &request(0.0, None, 10.0, 100)), (0.0, 25.0, 1));
        // and of a picture, wherever asked
        let picture = probe(None, Some(25.0), true);
        assert_eq!(spacing(&picture, &request(4.0, Some(3.0), 10.0, 100)), (0.0, 25.0, 1));
        // asked for none, still one; a long video read slowly, not never
        assert_eq!(spacing(&video, &request(0.0, Some(1.0), 10.0, 0)).2, 1);
        let (_, fps, count) = spacing(&video, &request(0.0, Some(1e9), 10.0, 3));
        assert_eq!((fps, count), (1e-3, 3));
    }

    #[test]
    fn scales_frames_down_only() {
        let video = probe(Some(1.0), Some(25.0), false);
        assert_eq!(scaled(&video, 320), (320, 180));
        assert_eq!(scaled(&video, 4096), (640, 360));
        assert_eq!(scaled(&video, 0), (1, 1));
        let thin = Probe { width: 1000, height: 1, ..video };
        assert_eq!(scaled(&thin, 100), (100, 1));
    }

    #[test]
    fn times_preview_frames_in_whole_ticks() {
        // 2.4 ticks each, as they fall
        assert_eq!(ticks(5, 25.0), [2, 3, 2, 3, 2]);
        assert_eq!(ticks(25, 25.0).iter().sum::<u32>(), 60);
        assert_eq!(ticks(3, 60.0), [1, 1, 1]);
        assert_eq!(ticks(2, 0.5), [120, 120]);
        // what plays in a third of a second, 30 a second
        assert_eq!(ticks(10, 29.97).iter().sum::<u32>(), 20);
        assert!(ticks(0, 25.0).is_empty());
    }

    #[test]
    fn orders_frame_starts_from_the_video_start() {
        // in decoding order, as B-frames come, a copy rounded apart, one
        // before the start that decoders drop, and one without a time
        let packets = vec![0.5, 0.62, 0.54, 0.54 + 1e-9, 0.58, 0.46, f64::NAN];
        let starts = in_video(packets, 0.5);
        let expected = [0.0, 0.04, 0.08, 0.12];
        assert_eq!(starts.len(), expected.len(), "{starts:?}");
        for (start, expected) in starts.iter().zip(expected) {
            assert!((start - expected).abs() < 1e-9, "{starts:?}");
        }
        assert!(in_video(Vec::new(), 0.0).is_empty());
    }
}

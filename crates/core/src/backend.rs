//! Runs ffmpeg either as external `ffmpeg`/`ffprobe` processes or via linked
//! libraries (the built-in ffmpeg). The Cargo features `linked` and
//! `linked-static` and the `linked` module retain the internal name for
//! packaging compatibility.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;

use crate::CancelToken;
use crate::config::Threads;
use crate::error::{Error, Result};
use crate::ffmpeg::{self, Capabilities, Ffmpeg, FfmpegChoice, Output, Probe};
use crate::webm::Params;
use crate::webm::Plan;

/// Pass of a libvpx encode. Two-pass encodes share a statistics file.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Pass<'a> {
    Single,
    First(&'a Path),
    Second(&'a Path),
}

/// Resolves `path` to an absolute path before passing it to ffmpeg.
///
/// ffmpeg treats relative names with a colon (such as
/// `2026-11-12T10:30:00.png`) as URL protocols, and its CLI tools treat
/// leading dashes as flags. Absolute paths avoid both issues (ffmpeg
/// recognises Windows drive letters). Symlinks are preserved; paths that
/// cannot be made absolute are returned unchanged.
fn absolute(path: &Path) -> Cow<'_, Path> {
    if path.is_absolute() {
        return Cow::Borrowed(path);
    }
    std::path::absolute(path).map_or(Cow::Borrowed(path), Cow::Owned)
}

/// Returns `plan` with its input resolved via [`absolute`].
fn reading_absolute(plan: &Plan) -> Cow<'_, Plan> {
    match absolute(&plan.input) {
        Cow::Borrowed(_) => Cow::Borrowed(plan),
        Cow::Owned(input) => Cow::Owned(Plan { input, ..plan.clone() }),
    }
}

/// Restores the user-supplied path `given` in any error that refers to `read`.
fn naming<T>(result: Result<T>, given: &Path, read: &Path) -> Result<T> {
    let named = |path: PathBuf| if path == read { given.to_path_buf() } else { path };
    result.map_err(|err| match err {
        Error::Unreadable { path, message } => Error::Unreadable { path: named(path), message },
        Error::NoVideo(path) => Error::NoVideo(named(path)),
        Error::NoFrame { path, at } => Error::NoFrame { path: named(path), at },
        err => err,
    })
}

/// Selected ffmpeg backend used for reading video and encoding WebM.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Backend {
    /// External `ffmpeg` and `ffprobe` executables.
    Process(Ffmpeg),
    /// Built-in ffmpeg libraries linked via the `linked` feature.
    #[cfg(feature = "linked")]
    #[cfg_attr(docsrs, doc(cfg(feature = "linked")))]
    Builtin,
}

/// Summary of the active ffmpeg backend, for status output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum BackendInfo {
    /// External [`Backend::Process`] executables.
    Process {
        /// Executable paths and how they were located.
        #[serde(flatten)]
        ffmpeg: Ffmpeg,
    },
    /// Built-in ffmpeg libraries.
    Builtin,
}

impl Backend {
    /// Selects a backend according to `choice`, or uses the executables at
    /// `path`.
    pub fn select(choice: FfmpegChoice, path: Option<&Path>) -> Result<Backend> {
        if let Some(path) = path {
            return Ok(Backend::Process(Ffmpeg::at(path)?));
        }
        match choice {
            #[cfg(feature = "linked")]
            FfmpegChoice::Auto | FfmpegChoice::Builtin => Ok(Backend::Builtin),
            #[cfg(not(feature = "linked"))]
            FfmpegChoice::Builtin => {
                Err(Error::FfmpegNotFound(crate::error::FfmpegMissing::NotBuiltIn))
            }
            _ => Ok(Backend::Process(Ffmpeg::on_path()?)),
        }
    }

    /// Returns true if extra `ffmpeg_args` are supported (external processes
    /// only; the built-in ffmpeg has no command line).
    pub fn supports_ffmpeg_args(&self) -> bool {
        matches!(self, Backend::Process(_))
    }

    /// Returns status metadata describing this backend.
    pub fn info(&self) -> BackendInfo {
        match self {
            Backend::Process(ffmpeg) => BackendInfo::Process { ffmpeg: ffmpeg.clone() },
            #[cfg(feature = "linked")]
            Backend::Builtin => BackendInfo::Builtin,
        }
    }

    /// Probes `input` for video stream dimensions, frame rate, duration and
    /// metadata.
    pub fn probe(&self, input: &Path, cancel: &CancelToken) -> Result<Probe> {
        let read = absolute(input);
        let probe = match self {
            Backend::Process(ffmpeg) => ffmpeg::probe(ffmpeg, &read, cancel),
            #[cfg(feature = "linked")]
            Backend::Builtin => crate::linked::probe(&read, cancel),
        };
        naming(probe, input, &read)
    }

    /// Returns sorted frame start times in seconds relative to `probe.start`,
    /// matching how cuts measure time. Returns empty if packets lack
    /// timestamps.
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
            Backend::Builtin => crate::linked::frame_starts(&read, cancel),
        };
        let in_file = naming(in_file, input, &read)?;
        Ok(in_video(in_file, probe.start))
    }

    /// Queries the backend's version and whether VP9 encoding is available.
    pub fn capabilities(&self, cancel: &CancelToken) -> Result<Capabilities> {
        match self {
            Backend::Process(ffmpeg) => ffmpeg::capabilities(ffmpeg, cancel),
            #[cfg(feature = "linked")]
            Backend::Builtin => crate::linked::capabilities(),
        }
    }

    /// Runs one encode attempt, or the first pass when `output` is `None`.
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
            Backend::Builtin => {
                crate::linked::encode(&read, params, pass, output, cancel, on_output)
            }
        };
        naming(encoded, &plan.input, &read.input)
    }

    /// Decodes video or image frames for display; see [`FramesRequest`].
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
                let (size, threads) = ((width, height), request.threads);
                ffmpeg::frames(ffmpeg, &read, probe, (start, fps), size, count, threads, cancel)
            }
            #[cfg(feature = "linked")]
            Backend::Builtin => {
                let (size, threads) = ((width, height), request.threads);
                crate::linked::frames(&read, probe, (start, fps), size, count, threads, cancel)
            }
        };
        let frames = naming(frames, path, &read)?;
        Ok(Frames { width, height, start, fps, frames })
    }

    /// Decodes a finished sticker for preview as straight RGBA with the longer
    /// side at most `max_side` pixels. Each frame carries its display duration
    /// in 60 fps ticks.
    pub fn preview(
        &self,
        path: &Path,
        max_side: u32,
        cancel: &CancelToken,
    ) -> Result<crate::preview::Preview> {
        let probe = self.probe(path, cancel)?;
        let fps = probe.fps_or_default().clamp(1.0, crate::preview::TICKS);
        // Spoofed stickers declare a shorter header duration than they actually
        // play, so use the content duration from the container blocks.
        let played =
            crate::webm::container::inspect_file(path).ok().and_then(|info| info.content_duration);
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
            // Lower the preview frame rate on long videos to stay within
            // PREVIEW_BYTES.
            max_frames: ((PREVIEW_BYTES as f64 / frame_bytes.max(1.0)) as usize).max(1),
            threads: Threads::Auto,
        };
        let frames = self.frames(path, &probe, &request, cancel)?;
        let ticks = ticks(frames.frames.len(), frames.fps);
        let rgba = frames.frames.into_iter().zip(ticks).collect();
        Ok(crate::preview::Preview { width: frames.width, height: frames.height, frames: rgba })
    }

    /// Decodes the single frame at `plan.start`, cropped, scaled and padded
    /// according to `plan`, as straight RGBA for static stickers.
    pub fn still(&self, plan: &Plan, cancel: &CancelToken) -> Result<Vec<u8>> {
        let read = reading_absolute(plan);
        let still = match self {
            Backend::Process(ffmpeg) => ffmpeg::still(ffmpeg, &read, cancel),
            #[cfg(feature = "linked")]
            Backend::Builtin => crate::linked::still(&read, cancel),
        };
        naming(still, &plan.input, &read.input)
    }

    /// Computes SSIM (0 to 1, ignoring alpha) comparing `candidate` (at `fps`)
    /// against the source (at the planned frame rate). Repeating candidate
    /// frames between ticks penalises lower frame rates on skipped frames.
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
            Backend::Builtin => crate::linked::ssim(&read, &candidate, fps, cancel),
        };
        naming(ssim, &plan.input, &read.input)
    }
}

/// Converts packet timestamps from file time to sorted, deduplicated seconds
/// relative to `video_start`.
fn in_video(in_file: Vec<f64>, video_start: f64) -> Vec<f64> {
    let mut starts: Vec<f64> = in_file.into_iter().map(|start| start - video_start).collect();
    // Packets arrive in decoding order, and decoders drop frames before start.
    starts.retain(|start| start.is_finite() && *start > -1e-6);
    starts.sort_by(f64::total_cmp);
    starts.dedup_by(|later, earlier| *later - *earlier < 1e-6);
    starts
}

/// Computes `(start, fps, frame_count)` for `request` against `probe`: at
/// most `request.fps` and the video's own rate, lowered so at most
/// `request.max_frames` frames span the length. Still images and empty ranges
/// give one frame.
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

/// Scales `probe`'s dimensions down so the longer side is at most `max_side`
/// (never scaling up, minimum 1x1).
fn scaled(probe: &Probe, max_side: u32) -> (u32, u32) {
    let longer = probe.width.max(probe.height).max(1);
    let scale = (f64::from(max_side) / f64::from(longer)).min(1.0);
    let size = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (size(probe.width), size(probe.height))
}

/// Converts `count` frames at `fps` into integer 60 fps tick durations,
/// rounding frame boundaries so the total matches playback duration (minimum 1
/// tick per frame).
fn ticks(count: usize, fps: f64) -> Vec<u32> {
    let start = |index: usize| (index as f64 * crate::preview::TICKS / fps).round() as u32;
    (0..count).map(|index| (start(index + 1) - start(index)).max(1)).collect()
}

/// Memory budget for decoded preview frames.
const PREVIEW_BYTES: usize = 256 << 20;
/// Fallback preview duration (in seconds) when a sticker's duration is unknown.
const UNKNOWN_SECONDS: f64 = 3.0;

/// Parameters for decoding display frames from a video or image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramesRequest {
    /// Start offset in seconds.
    pub start: f64,
    /// Duration to decode in seconds, or the remainder of the input if `None`.
    pub length: Option<f64>,
    /// Maximum frame rate (clamped to the input's frame rate).
    pub fps: f64,
    /// Maximum length of the longer side in pixels.
    pub max_side: u32,
    /// Maximum number of frames; the frame rate is lowered to stay within
    /// this cap.
    pub max_frames: usize,
    /// Number of threads used for decoding.
    pub threads: Threads,
}

/// Evenly spaced straight-RGBA video frames in display orientation.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Frames {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Timestamp of the first frame in seconds.
    pub start: f64,
    /// Frame rate between consecutive frames.
    pub fps: f64,
    /// Raw RGBA buffers (`width * height * 4` bytes each).
    pub frames: Vec<Vec<u8>>,
}

/// Path to the statistics file used by ffmpeg's `-passlogfile PREFIX`.
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
        FramesRequest { start, length, fps, max_side: 320, max_frames, threads: Threads::Auto }
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

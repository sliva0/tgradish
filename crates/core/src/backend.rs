//! The two ways of running ffmpeg: as separate `ffmpeg` and `ffprobe`
//! processes, or through libraries linked into the binary (the `linked`
//! feature).

use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;

use crate::CancelToken;
use crate::convert::Plan;
use crate::error::Result;
use crate::events::Params;
use crate::ffmpeg::{self, Capabilities, Ffmpeg, FfmpegChoice, Output, Probe};

/// Pass of a libvpx encode. Two-pass encodes share a statistics file.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Pass<'a> {
    Single,
    First(&'a Path),
    Second(&'a Path),
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
#[serde(tag = "kind", rename_all = "snake_case")]
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
            FfmpegChoice::Builtin => Err(crate::Error::FfmpegNotFound(
                "this tgradish build has no built-in ffmpeg".into(),
            )),
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
        match self {
            Backend::Process(ffmpeg) => ffmpeg::probe(ffmpeg, input, cancel),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::probe(input, cancel),
        }
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
        let in_file = match self {
            Backend::Process(ffmpeg) => ffmpeg::frame_starts(ffmpeg, input, cancel)?,
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::frame_starts(input, cancel)?,
        };
        let mut starts: Vec<f64> = in_file.into_iter().map(|start| start - probe.start).collect();
        // packets come in decoding order, and decoders drop those before
        // the start
        starts.retain(|start| start.is_finite() && *start > -1e-6);
        starts.sort_by(f64::total_cmp);
        starts.dedup_by(|later, earlier| *later - *earlier < 1e-6);
        Ok(starts)
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
        match self {
            Backend::Process(ffmpeg) => {
                let pass = match pass {
                    Pass::Single => None,
                    Pass::First(log) => Some((1, log)),
                    Pass::Second(log) => Some((2, log)),
                };
                let mut cmd = Command::new(&ffmpeg.ffmpeg);
                cmd.args(ffmpeg::encode_args(plan, params, pass, output));
                ffmpeg::run(cmd, "ffmpeg", cancel, on_output).map(drop)
            }
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::encode(plan, params, pass, output, cancel, on_output),
        }
    }

    /// Frames of a video or image for showing it, see [`FramesRequest`].
    pub fn frames(
        &self,
        path: &Path,
        probe: &Probe,
        request: &FramesRequest,
        cancel: &CancelToken,
    ) -> Result<Frames> {
        let start = if probe.still_image { 0.0 } else { request.start.max(0.0) };
        let length = match (request.length, probe.duration) {
            (Some(length), _) => length,
            (None, Some(duration)) => duration - start,
            (None, None) => 0.0,
        };
        let source_fps = probe.fps_or_default();
        let max_frames = request.max_frames.max(1);
        let (fps, count) = if probe.still_image || length <= 0.0 {
            (source_fps, 1)
        } else {
            let fps = request.fps.min(source_fps).min(max_frames as f64 / length).max(1e-3);
            (fps, ((length * fps).ceil() as usize).clamp(1, max_frames))
        };
        let longer = probe.width.max(probe.height).max(1);
        let scale = (f64::from(request.max_side) / f64::from(longer)).min(1.0);
        let size = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
        let (width, height) = (size(probe.width), size(probe.height));
        let frames = match self {
            Backend::Process(ffmpeg) => {
                ffmpeg::frames(ffmpeg, path, probe, (start, fps), (width, height), count, cancel)?
            }
            #[cfg(feature = "linked")]
            Backend::Linked => {
                crate::linked::frames(path, probe, (start, fps), (width, height), count, cancel)?
            }
        };
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
        let fps = probe.fps_or_default().clamp(1.0, 60.0);
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
        // whole 60 fps frames, rounding where each starts
        let start = |index: usize| (index as f64 * 60.0 / frames.fps).round() as u32;
        let rgba = frames
            .frames
            .into_iter()
            .enumerate()
            .map(|(index, rgba)| (rgba, (start(index + 1) - start(index)).max(1)))
            .collect();
        Ok(crate::preview::Preview { width: frames.width, height: frames.height, frames: rgba })
    }

    /// The planned frame at the plan's start, cropped, scaled and padded as
    /// the plan says: straight RGBA of the plan's size, for image stickers.
    pub fn still(&self, plan: &Plan, cancel: &CancelToken) -> Result<Vec<u8>> {
        match self {
            Backend::Process(ffmpeg) => ffmpeg::still(ffmpeg, plan, cancel),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::still(plan, cancel),
        }
    }

    /// See [`ffmpeg::ssim`].
    pub fn ssim(
        &self,
        plan: &Plan,
        candidate: &Path,
        fps: f64,
        cancel: &CancelToken,
        on_line: &mut dyn FnMut(String),
    ) -> Result<f64> {
        match self {
            Backend::Process(ffmpeg) => ffmpeg::ssim(ffmpeg, plan, candidate, fps, cancel, on_line),
            #[cfg(feature = "linked")]
            Backend::Linked => crate::linked::ssim(plan, candidate, fps, cancel),
        }
    }
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

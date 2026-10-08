//! The two ways of running ffmpeg: as separate `ffmpeg` and `ffprobe`
//! processes, or through libraries linked into the binary (the `linked`
//! feature).

use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;

use crate::convert::Plan;
use crate::error::Result;
use crate::events::Params;
use crate::ffmpeg::{self, CancelToken, Capabilities, Ffmpeg, FfmpegChoice, Output, Probe};

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

    /// The first seconds of a video, decoded for a preview: straight RGBA
    /// at most [`PREVIEW_SIDE`] pixels on the longer side, each frame with
    /// how many 60 fps frames it shows for.
    pub fn preview(&self, path: &Path, cancel: &CancelToken) -> Result<crate::tgs::Preview> {
        let probe = self.probe(path, cancel)?;
        let fps = probe.fps.unwrap_or(25.0).clamp(1.0, 60.0);
        let count = (PREVIEW_SECONDS * fps).ceil() as usize;
        let scale =
            (f64::from(PREVIEW_SIDE) / f64::from(probe.width.max(probe.height).max(1))).min(1.0);
        let size = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
        let (width, height) = (size(probe.width), size(probe.height));
        let frames = match self {
            Backend::Process(ffmpeg) => {
                ffmpeg::preview_frames(ffmpeg, path, &probe, (width, height), count, cancel)?
            }
            #[cfg(feature = "linked")]
            Backend::Linked => {
                crate::linked::preview_frames(path, &probe, (width, height), count, cancel)?
            }
        };
        // whole 60 fps frames, rounding where each starts
        let start = |index: usize| (index as f64 * 60.0 / fps).round() as u32;
        let frames = frames
            .into_iter()
            .enumerate()
            .map(|(index, rgba)| (rgba, (start(index + 1) - start(index)).max(1)))
            .collect();
        Ok(crate::tgs::Preview { width, height, frames })
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

/// Longer side of previews, in pixels.
pub const PREVIEW_SIDE: u32 = 320;
/// How much of a video previews show, in seconds.
const PREVIEW_SECONDS: f64 = 3.0;

/// Statistics file that ffmpeg's `-passlogfile PREFIX` uses.
#[cfg(feature = "linked")]
pub(crate) fn pass_log_file(prefix: &Path) -> std::path::PathBuf {
    let mut name = prefix.as_os_str().to_owned();
    name.push("-0.log");
    std::path::PathBuf::from(name)
}

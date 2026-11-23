//! External `ffmpeg`/`ffprobe` process runner (cancellable via
//! [`CancelToken`]) and types shared with the linked backend: [`Probe`],
//! [`Orientation`], filter graphs and command-line builders.

mod args;
mod locate;
mod orientation;
mod probe;
mod process;
mod ssim;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::CancelToken;
use serde::{Deserialize, Serialize};

pub use args::commands;
pub(crate) use args::encode_args;
#[cfg(test)]
pub(crate) use args::input_args;
use args::threads_args;
#[cfg(any(feature = "linked", test))]
pub(crate) use args::video_filter;
#[cfg(feature = "linked")]
pub(crate) use args::{frames_filter, picture_filter, read_seconds, ssim_graph};
pub use orientation::Orientation;
pub use probe::Probe;
#[cfg(feature = "linked")]
pub(crate) use probe::{Facts, Timing};
pub(crate) use probe::{frame_starts, probe};
pub(crate) use process::{Output, run};
pub(crate) use ssim::ssim;

/// Where to look for ffmpeg.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum FfmpegChoice {
    /// The built-in ffmpeg if there is one, otherwise the system one.
    #[default]
    Auto,
    /// ffmpeg and ffprobe from `PATH`.
    System,
    /// The ffmpeg built into tgradish, in builds with the `linked`
    /// feature.
    Builtin,
}

/// Where the ffmpeg in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FfmpegSource {
    /// Explicitly configured path.
    Path,
    /// Found on `PATH`.
    System,
}

/// What an ffmpeg build can do, as far as tgradish cares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Capabilities {
    /// First line of `ffmpeg -version`.
    pub version: String,
    /// Whether it has the libvpx VP9 encoder that stickers need.
    pub libvpx_vp9: bool,
}

/// Runs `ffmpeg -version` and `ffmpeg -encoders`.
pub(crate) fn capabilities(ffmpeg: &Ffmpeg, cancel: &CancelToken) -> crate::Result<Capabilities> {
    let stdout = |arg: &str| -> crate::Result<Vec<String>> {
        let mut cmd = std::process::Command::new(&ffmpeg.ffmpeg);
        cmd.args(["-hide_banner", arg]);
        let mut lines = Vec::new();
        run(cmd, "ffmpeg", cancel, &mut |output| {
            if let Output::Stdout(line) = output {
                lines.push(line);
            }
        })?;
        Ok(lines)
    };
    let version = stdout("-version")?.into_iter().next().unwrap_or_default();
    let libvpx_vp9 = stdout("-encoders")?.iter().any(|line| line.contains(" libvpx-vp9 "));
    Ok(Capabilities { version, libvpx_vp9 })
}

/// ffmpeg and ffprobe executables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Ffmpeg {
    /// The `ffmpeg` executable.
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub ffmpeg: PathBuf,
    /// The `ffprobe` executable, beside it or on `PATH` with it.
    #[serde(serialize_with = "crate::wire::serialize_path")]
    pub ffprobe: PathBuf,
    /// Where they were found.
    pub source: FfmpegSource,
}

/// Decodes up to `count` straight-RGBA frames of `input` starting at `start`
/// seconds at `fps`, scaled to `(width, height)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn frames(
    ffmpeg: &Ffmpeg,
    input: &Path,
    probe: &Probe,
    (start, fps): (f64, f64),
    (width, height): (u32, u32),
    count: usize,
    threads: crate::config::Threads,
    cancel: &CancelToken,
) -> crate::Result<Vec<Vec<u8>>> {
    cancel.check()?;
    let mut cmd = Command::new(&ffmpeg.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-v", "error"]);
    cmd.args(threads_args(threads, "-filter_threads"));
    let seek = probe.seek(start);
    if seek > 0.0 && !probe.still_image {
        cmd.args(["-ss", &format!("{seek:.6}")]);
    }
    if let Some(decoder) = &probe.decoder {
        cmd.args(["-c:v", decoder]);
    }
    cmd.args(threads_args(threads, "-threads"));
    cmd.arg("-i").arg(input);
    let filter = args::frames_filter(probe.still_image || count == 1, fps, (width, height));
    cmd.args(["-map", "0:v:0", "-frames:v", &count.to_string(), "-vf", &filter]);
    cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
    let frame = width as usize * height as usize * 4;
    process::run_pieces(cmd, "ffmpeg", frame, cancel)
}

/// Decodes the planned frame at `plan.start`, cropped, scaled and padded to
/// the plan's size, as straight RGBA for static WebP output.
pub(crate) fn still(
    ffmpeg: &Ffmpeg,
    plan: &crate::webm::Plan,
    cancel: &CancelToken,
) -> crate::Result<Vec<u8>> {
    cancel.check()?;
    let length = 1.0 / plan.fps;
    let mut cmd = Command::new(&ffmpeg.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-v", "error"]);
    cmd.args(threads_args(plan.threads, "-filter_threads"));
    cmd.args(args::input_args(plan, length, plan.fps));
    // Take the first frame directly without fps resampling, which can skip a
    // short frame.
    cmd.args([
        "-map",
        "0:v:0",
        "-vf",
        &args::picture_filter(plan, crate::presets::Format::Webp, "rgba"),
    ]);
    cmd.args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
    let out = process::run_bytes(cmd, "ffmpeg", cancel)?;
    let size = plan.width as usize * plan.height as usize * 4;
    match out.get(..size) {
        Some(pixels) => Ok(pixels.to_vec()),
        None => Err(crate::Error::NoFrame { path: plan.input.clone(), at: plan.start }),
    }
}

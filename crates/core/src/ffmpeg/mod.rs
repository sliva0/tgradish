//! Running ffmpeg as a separate process.

mod args;
mod locate;
mod orientation;
mod probe;
mod process;
mod ssim;

use std::path::{Path, PathBuf};
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) use args::encode_args;
#[cfg(feature = "linked")]
pub(crate) use args::video_filter;
pub use locate::locate;
pub use orientation::Orientation;
#[cfg(feature = "linked")]
pub(crate) use probe::pix_fmt_has_alpha;
pub use probe::{Probe, probe};
pub use process::CancelToken;
pub(crate) use process::{Output, run};
pub use ssim::ssim;

/// Where to look for ffmpeg.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FfmpegChoice {
    /// The built-in ffmpeg if there is one, otherwise the system one.
    #[default]
    Auto,
    /// ffmpeg and ffprobe from `PATH`.
    System,
    /// ffmpeg linked into tgradish, in builds with the `linked` feature.
    Builtin,
}

/// Where the ffmpeg in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FfmpegSource {
    /// Explicitly configured path.
    Path,
    /// Found on `PATH`.
    System,
}

/// What an ffmpeg build can do, as far as tgradish cares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Capabilities {
    /// First line of `ffmpeg -version`.
    pub version: String,
    /// Whether it has the libvpx VP9 encoder that stickers need.
    pub libvpx_vp9: bool,
}

/// Runs `ffmpeg -version` and `ffmpeg -encoders`.
pub fn capabilities(ffmpeg: &Ffmpeg, cancel: &CancelToken) -> crate::Result<Capabilities> {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Ffmpeg {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub source: FfmpegSource,
}

/// Up to `count` frames of `input`, scaled to `size`, as straight RGBA.
pub(crate) fn preview_frames(
    ffmpeg: &Ffmpeg,
    input: &Path,
    probe: &Probe,
    (width, height): (u32, u32),
    count: usize,
    cancel: &CancelToken,
) -> crate::Result<Vec<Vec<u8>>> {
    cancel.check()?;
    let mut cmd = Command::new(&ffmpeg.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-v", "error"]);
    if let Some(decoder) = &probe.decoder {
        cmd.args(["-c:v", decoder]);
    }
    cmd.arg("-i").arg(input);
    cmd.args(["-frames:v", &count.to_string(), "-vf", &format!("scale={width}:{height}")]);
    cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(crate::Error::Ffmpeg {
            program: "ffmpeg",
            status: out.status.to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    let frame = width as usize * height as usize * 4;
    Ok(out.stdout.chunks_exact(frame).map(<[u8]>::to_vec).collect())
}

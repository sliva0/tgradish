//! Running ffmpeg as a separate process.

mod args;
#[cfg(feature = "download")]
pub mod download;
mod locate;
mod probe;
mod process;
mod ssim;

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) use args::encode_args;
pub use locate::{bundled_dir, downloaded_dir, locate};
pub use probe::{Probe, probe};
pub use process::CancelToken;
pub(crate) use process::{Output, run};
pub use ssim::ssim;

/// Where to look for ffmpeg.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FfmpegChoice {
    /// The bundled build, then a downloaded one, then the system one.
    #[default]
    Auto,
    /// The build shipped next to the tgradish executable.
    Bundled,
    /// The build fetched by `tgradish ffmpeg download`.
    Downloaded,
    /// ffmpeg from `PATH`.
    System,
}

/// Where the ffmpeg in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FfmpegSource {
    /// Explicitly configured path.
    Path,
    Bundled,
    Downloaded,
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

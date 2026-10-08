//! Running ffmpeg as a separate process.

mod args;
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
pub(crate) use ssim::ssim;

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

/// ffmpeg and ffprobe executables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Ffmpeg {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub source: FfmpegSource,
}

//! Finding ffmpeg and ffprobe: at a path the user gave, or on PATH.

use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use super::{Ffmpeg, FfmpegSource};
use crate::error::{Error, FfmpegMissing, Result};

fn exe(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{EXE_SUFFIX}"))
}

impl Ffmpeg {
    /// The ffmpeg and ffprobe on PATH, ffprobe from beside ffmpeg where
    /// it is there.
    pub fn on_path() -> Result<Ffmpeg> {
        let ffmpeg = which::which("ffmpeg").ok();
        // prefer the ffprobe from the same installation
        let ffprobe = ffmpeg.as_deref().and_then(|ffmpeg| {
            let beside = ffmpeg.parent().map(|dir| exe(dir, "ffprobe"));
            beside.filter(|path| path.is_file()).or_else(|| which::which("ffprobe").ok())
        });
        match (ffmpeg, ffprobe) {
            (Some(ffmpeg), Some(ffprobe)) => {
                Ok(Ffmpeg { ffmpeg, ffprobe, source: FfmpegSource::System })
            }
            _ => Err(Error::FfmpegNotFound(FfmpegMissing::NotOnPath)),
        }
    }

    /// The ffmpeg at `path`, the executable or the folder holding it, with
    /// the ffprobe beside it.
    pub(crate) fn at(path: &Path) -> Result<Ffmpeg> {
        let dir = if path.is_dir() { path } else { path.parent().unwrap_or(Path::new(".")) };
        let ffmpeg = if path.is_dir() { exe(dir, "ffmpeg") } else { path.to_path_buf() };
        let ffprobe = exe(dir, "ffprobe");
        if !ffmpeg.is_file() {
            return Err(Error::FfmpegNotFound(FfmpegMissing::NoProgram(ffmpeg)));
        }
        if !ffprobe.is_file() {
            return Err(Error::FfmpegNotFound(FfmpegMissing::NoFfprobe(ffprobe)));
        }
        Ok(Ffmpeg { ffmpeg, ffprobe, source: FfmpegSource::Path })
    }
}

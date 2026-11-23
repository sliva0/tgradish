//! Locates external `ffmpeg` and `ffprobe` executables from a configured path
//! or `PATH`.

use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use super::{Ffmpeg, FfmpegSource};
use crate::error::{Error, FfmpegMissing, Result};

fn exe(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{EXE_SUFFIX}"))
}

impl Ffmpeg {
    /// Locates `ffmpeg` on `PATH` and prefers the sibling `ffprobe` from the
    /// same directory before falling back to `PATH`.
    pub fn on_path() -> Result<Ffmpeg> {
        let ffmpeg = which::which("ffmpeg").ok();
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

    /// Resolves `ffmpeg` and its sibling `ffprobe` from an explicit executable
    /// or directory `path`.
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

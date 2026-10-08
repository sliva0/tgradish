use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use super::{Ffmpeg, FfmpegChoice, FfmpegSource};
use crate::error::{Error, Result};

fn exe(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{EXE_SUFFIX}"))
}

fn system() -> Option<Ffmpeg> {
    let ffmpeg = which::which("ffmpeg").ok()?;
    // prefer the ffprobe from the same installation
    let ffprobe = ffmpeg
        .parent()
        .map(|dir| exe(dir, "ffprobe"))
        .filter(|path| path.is_file())
        .or_else(|| which::which("ffprobe").ok())?;
    Some(Ffmpeg { ffmpeg, ffprobe, source: FfmpegSource::System })
}

/// Finds ffmpeg. `path` overrides `choice` and may point at the ffmpeg
/// executable or at the directory containing it; ffprobe must be next to it.
pub fn locate(choice: FfmpegChoice, path: Option<&Path>) -> Result<Ffmpeg> {
    if let Some(path) = path {
        let dir = if path.is_dir() { path } else { path.parent().unwrap_or(Path::new(".")) };
        let ffmpeg = if path.is_dir() { exe(dir, "ffmpeg") } else { path.to_path_buf() };
        let ffprobe = exe(dir, "ffprobe");
        if !ffmpeg.is_file() {
            return Err(Error::FfmpegNotFound(format!("{} does not exist", ffmpeg.display())));
        }
        if !ffprobe.is_file() {
            return Err(Error::FfmpegNotFound(format!(
                "ffprobe is missing, expected it at {}",
                ffprobe.display()
            )));
        }
        return Ok(Ffmpeg { ffmpeg, ffprobe, source: FfmpegSource::Path });
    }

    let found = match choice {
        FfmpegChoice::Auto | FfmpegChoice::System => system(),
        FfmpegChoice::Builtin => None,
    };
    found.ok_or_else(|| {
        let hint = match choice {
            FfmpegChoice::Auto | FfmpegChoice::System => {
                "ffmpeg and ffprobe are not on PATH: install them or pass --ffmpeg PATH"
            }
            FfmpegChoice::Builtin => "the built-in ffmpeg is not an executable",
        };
        Error::FfmpegNotFound(hint.into())
    })
}

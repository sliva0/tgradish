use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use super::{Ffmpeg, FfmpegChoice, FfmpegSource};
use crate::error::{Error, Result};

fn exe(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{EXE_SUFFIX}"))
}

/// Finds ffmpeg and ffprobe in `dir`, if both are there.
fn in_dir(dir: &Path, source: FfmpegSource) -> Option<Ffmpeg> {
    let ffmpeg = exe(dir, "ffmpeg");
    let ffprobe = exe(dir, "ffprobe");
    (ffmpeg.is_file() && ffprobe.is_file()).then_some(Ffmpeg { ffmpeg, ffprobe, source })
}

/// Directory of the build shipped next to the tgradish executable.
pub fn bundled_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

/// Directory that `tgradish ffmpeg download` installs into.
pub fn downloaded_dir() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("ffmpeg"))
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

    let bundled = || bundled_dir().and_then(|dir| in_dir(&dir, FfmpegSource::Bundled));
    let downloaded = || downloaded_dir().and_then(|dir| in_dir(&dir, FfmpegSource::Downloaded));
    let found = match choice {
        FfmpegChoice::Auto => bundled().or_else(downloaded).or_else(system),
        FfmpegChoice::Bundled => bundled(),
        FfmpegChoice::Downloaded => downloaded(),
        FfmpegChoice::System => system(),
    };
    found.ok_or_else(|| {
        let hint = match choice {
            FfmpegChoice::Auto => {
                "install ffmpeg, run `tgradish ffmpeg download` or pass --ffmpeg PATH"
            }
            FfmpegChoice::Bundled => "this tgradish build does not include ffmpeg",
            FfmpegChoice::Downloaded => "run `tgradish ffmpeg download` first",
            FfmpegChoice::System => "ffmpeg and ffprobe are not on PATH",
        };
        Error::FfmpegNotFound(hint.into())
    })
}

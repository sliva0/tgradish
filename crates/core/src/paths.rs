//! Per-user directories, following each OS's conventions: XDG on Linux,
//! `%APPDATA%` on Windows, `~/Library/Application Support` on macOS.

use std::path::PathBuf;

use directories::{ProjectDirs, UserDirs};

fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "tgradish")
}

/// Directory for `config.toml` and user presets.
pub fn config_dir() -> Option<PathBuf> {
    Some(dirs()?.config_dir().to_path_buf())
}

/// The user's pictures directory, or else their home directory.
pub fn pictures_dir() -> Option<PathBuf> {
    let dirs = UserDirs::new()?;
    Some(dirs.picture_dir().unwrap_or(dirs.home_dir()).to_path_buf())
}

//! Per-user directories, following each OS's conventions: XDG on Linux,
//! `%APPDATA%` on Windows, `~/Library/Application Support` on macOS.

use std::path::PathBuf;

use directories::ProjectDirs;

fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "tgradish")
}

/// Directory for `config.toml` and user presets.
pub fn config_dir() -> Option<PathBuf> {
    Some(dirs()?.config_dir().to_path_buf())
}

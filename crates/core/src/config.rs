#![doc = concat!(
    "User configuration, `config.toml` in the config dir:\n\n```toml\n",
    include_str!("config.example.toml"),
    "```"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, FileAction, Result};
use crate::ffmpeg::FfmpegChoice;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Preset used when none is given, for WebM.
    pub preset: Option<String>,
    /// Preset used when none is given, for `.tgs`.
    pub tgs_preset: Option<String>,
    pub ffmpeg: FfmpegConfig,
    /// The window's settings, which the window reads and words itself:
    /// kept here as written, and saved again with the rest. They aren't
    /// held to this file's strictness, so a setting a newer window writes
    /// never breaks the command line.
    #[serde(skip_serializing_if = "toml::Table::is_empty")]
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub gui: toml::Table,
}

impl Config {
    /// The preset to use for `format` when none is given.
    pub fn preset_for(&self, format: crate::presets::Format) -> &str {
        let configured = match format {
            crate::presets::Format::Webm | crate::presets::Format::Webp => &self.preset,
            crate::presets::Format::Tgs => &self.tgs_preset,
        };
        configured.as_deref().unwrap_or(format.default_preset())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct FfmpegConfig {
    /// Where to look for ffmpeg, as `--ffmpeg-from` says.
    #[serde(rename = "from")]
    pub choice: FfmpegChoice,
    /// ffmpeg executable or the directory containing ffmpeg and ffprobe.
    /// Overrides `from`.
    pub path: Option<PathBuf>,
}

impl Config {
    /// Default location of `config.toml`.
    pub fn default_path() -> Option<PathBuf> {
        Some(crate::paths::config_dir()?.join("config.toml"))
    }

    /// Reads `path`. A missing file gives the default config.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|err| Error::Config {
                path: path.to_path_buf(),
                message: err.to_string(),
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(Error::io(FileAction::Read, path)(err)),
        }
    }

    /// Writes the config to `path`, creating its directory. Comments in an
    /// existing file are lost.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string(self)
            .map_err(|err| Error::Config { path: path.to_path_buf(), message: err.to_string() })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(Error::io(FileAction::Create, dir))?;
        }
        crate::fsutil::write_file(path, text.as_bytes(), true)
            .map_err(Error::io(FileAction::Write, path))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_example() {
        let example: Config = toml::from_str(include_str!("config.example.toml")).unwrap();
        let presets = crate::presets::Presets::builtin();
        assert!(presets.webm(example.preset.as_deref().unwrap()).is_ok());
        assert!(presets.tgs(example.tgs_preset.as_deref().unwrap()).is_ok());
    }

    #[test]
    fn reads_config() {
        let config: Config =
            toml::from_str("preset = 'fast'\n[ffmpeg]\nfrom = 'system'\n").unwrap();
        assert_eq!(config.preset.as_deref(), Some("fast"));
        assert_eq!(config.ffmpeg.choice, FfmpegChoice::System);
        // named as --ffmpeg-from is
        assert!(toml::from_str::<Config>("[ffmpeg]\nuse = 'system'\n").is_err());
    }

    #[test]
    fn saves_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut config = Config { tgs_preset: Some("tgs-fast".into()), ..Config::default() };
        config.gui.insert("output-dir".into(), "out".into());
        config.ffmpeg.choice = FfmpegChoice::System;
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn leaves_the_window_its_own_settings() {
        // what the window writes is its own to read, even keys it learns
        // later; the rest of the file still catches typos
        let text = "preset = 'fast'\n[gui]\nnewer-setting = { any = 1 }\ntheme = 7\n";
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.gui["theme"].as_integer(), Some(7));
        let saved: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(saved, config);
        assert!(toml::from_str::<Config>("presett = 'fast'\n").is_err());
        assert!(!toml::to_string(&Config::default()).unwrap().contains("[gui]"));
    }

    #[cfg(unix)]
    #[test]
    fn saves_through_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let (managed, path) = (dir.path().join("managed.toml"), dir.path().join("config.toml"));
        std::fs::write(&managed, "preset = 'fast'\n").unwrap();
        std::os::unix::fs::symlink(&managed, &path).unwrap();
        let config = Config { preset: Some("best".into()), ..Config::load(&path).unwrap() };
        config.save(&path).unwrap();
        // still a link, to what was saved
        assert!(path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(Config::load(&managed).unwrap(), config);
    }

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(&dir.path().join("config.toml")).unwrap(), Config::default());
    }
}

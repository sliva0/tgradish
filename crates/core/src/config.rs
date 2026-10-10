//! User configuration, `config.toml` in the config dir:
//!
//! ```toml
//! preset = "sticker"   # preset used when none is given, for WebM
//! tgs-preset = "tgs-sticker"  # the same for .tgs
//!
//! [ffmpeg]
//! use = "auto"         # auto (built in if there is one, else system),
//!                      # builtin or system
//! path = "/opt/ffmpeg" # ffmpeg executable or its directory, overrides `use`
//!
//! [gui]
//! output-dir = "/home/me/stickers"  # results go here instead of next to inputs
//! overwrite = false                  # replace existing results
//! smooth-scrolling = false           # ease scrolling instead of following the wheel
//! ```

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ffmpeg::FfmpegChoice;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Preset used when none is given, for WebM.
    pub preset: Option<String>,
    /// Preset used when none is given, for `.tgs`.
    pub tgs_preset: Option<String>,
    pub ffmpeg: FfmpegConfig,
    pub gui: GuiConfig,
}

/// Settings of the tgradish window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GuiConfig {
    /// Directory for results, instead of next to each input.
    pub output_dir: Option<PathBuf>,
    /// Replace existing results.
    pub overwrite: bool,
    /// What new files become: stickers or emoji.
    pub target: Option<crate::telegram::Target>,
    /// Ease scrolling over a few frames instead of following the wheel.
    pub smooth_scrolling: bool,
}

impl Config {
    /// The preset to use for `format` when none is given.
    pub fn preset_for(&self, format: crate::presets::Format) -> &str {
        let configured = match format {
            crate::presets::Format::Webm => &self.preset,
            crate::presets::Format::Tgs => &self.tgs_preset,
        };
        configured.as_deref().unwrap_or(format.default_preset())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct FfmpegConfig {
    /// Where to look for ffmpeg.
    #[serde(rename = "use")]
    pub choice: FfmpegChoice,
    /// ffmpeg executable or the directory containing ffmpeg and ffprobe.
    /// Overrides `use`.
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
            Ok(text) => toml::from_str(&text)
                .map_err(|err| Error::InvalidOptions(format!("{}: {err}", path.display()))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err.into()),
        }
    }

    /// Writes the config to `path`, creating its directory. Comments in an
    /// existing file are lost.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string(self)
            .map_err(|err| Error::InvalidOptions(format!("cannot write the config: {err}")))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::fsutil::write_file(path, text.as_bytes(), true)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_config() {
        let config: Config =
            toml::from_str("preset = 'emoji'\n[ffmpeg]\nuse = 'system'\n").unwrap();
        assert_eq!(config.preset.as_deref(), Some("emoji"));
        assert_eq!(config.ffmpeg.choice, FfmpegChoice::System);
        assert!(toml::from_str::<Config>("[ffmpeg]\nsource = 'system'\n").is_err());
    }

    #[test]
    fn saves_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut config = Config { tgs_preset: Some("tgs-fast".into()), ..Config::default() };
        config.gui.output_dir = Some(dir.path().join("out"));
        config.ffmpeg.choice = FfmpegChoice::System;
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(&dir.path().join("config.toml")).unwrap(), Config::default());
    }
}

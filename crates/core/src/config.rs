#![doc = concat!(
    "User configuration, `config.toml` in the config dir:\n\n```toml\n",
    include_str!("config.example.toml"),
    "```"
)]

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
    /// The window's language, as a tag like `de`; the system's when unset.
    pub language: Option<String>,
    /// Light or dark, or as the system has it.
    pub theme: Theme,
    /// Start with no files, rather than those open when the window last
    /// closed.
    pub forget_files: bool,
    /// Show the timeline's cuts and times in frames rather than seconds.
    pub timeline_frames: bool,
    pub result_preview: ResultPreview,
}

/// The result's small picture beside the input in the window: how large
/// it is, and in which corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ResultPreview {
    /// From 1, the smallest, to 5.
    pub size: u8,
    pub corner: Corner,
}

impl Default for ResultPreview {
    fn default() -> ResultPreview {
        ResultPreview { size: 3, corner: Corner::default() }
    }
}

/// A corner of the preview.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

impl Corner {
    pub fn left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }

    pub fn top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }

    /// The corner on the `left` or right, at the `top` or bottom.
    pub fn at(left: bool, top: bool) -> Corner {
        match (left, top) {
            (true, true) => Corner::TopLeft,
            (false, true) => Corner::TopRight,
            (true, false) => Corner::BottomLeft,
            (false, false) => Corner::BottomRight,
        }
    }
}

/// The window's colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    /// As the system has it.
    #[default]
    System,
    Light,
    Dark,
}

impl Config {
    /// The preset to use for `format` when none is given.
    pub fn preset_for(&self, format: crate::presets::Format) -> &str {
        let configured = match format {
            crate::presets::Format::Webm | crate::presets::Format::Image => &self.preset,
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
    #[test]
    fn reads_the_documented_example() {
        let example: Config = toml::from_str(include_str!("config.example.toml")).unwrap();
        let presets = crate::presets::Presets::builtin();
        assert!(presets.webm(example.preset.as_deref().unwrap()).is_ok());
        assert!(presets.tgs(example.tgs_preset.as_deref().unwrap()).is_ok());
    }

    use super::*;

    #[test]
    fn reads_config() {
        let config: Config = toml::from_str("preset = 'fast'\n[ffmpeg]\nuse = 'system'\n").unwrap();
        assert_eq!(config.preset.as_deref(), Some("fast"));
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

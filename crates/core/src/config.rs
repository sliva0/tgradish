//! User configuration, `config.toml` in the config dir:
//!
//! ```toml
//! preset = "sticker"   # preset used when none is given
//!
//! [ffmpeg]
//! use = "auto"         # auto, bundled, downloaded or system
//! path = "/opt/ffmpeg" # ffmpeg executable or its directory, overrides `use`
//! ```

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ffmpeg::FfmpegChoice;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Preset used when none is given.
    pub preset: Option<String>,
    pub ffmpeg: FfmpegConfig,
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
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(&dir.path().join("config.toml")).unwrap(), Config::default());
    }
}

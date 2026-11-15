#![doc = concat!(
    "User configuration, `config.toml` in the config dir:\n\n```toml\n",
    include_str!("config.example.toml"),
    "```"
)]

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, FileAction, Result};
use crate::ffmpeg::FfmpegChoice;

/// A config file setting everything there is to set, each explained, as
/// these docs show it.
pub const EXAMPLE: &str = include_str!("config.example.toml");

/// [`EXAMPLE`] to start a config file from: what only the machine it was
/// written on has, as a path to ffmpeg or a folder for results, left
/// there as comments, and the rest as it is.
pub fn starting_example() -> String {
    let machines_own = ["path = ", "output-dir = "];
    EXAMPLE
        .lines()
        .map(|line| match machines_own.iter().any(|key| line.starts_with(key)) {
            true => format!("# {line}\n"),
            false => format!("{line}\n"),
        })
        .collect()
}

/// The user's settings, from `config.toml` ([`Config::default_path`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Preset used when none is given, for WebM.
    pub preset: Option<String>,
    /// Preset used when none is given, for `.tgs`.
    pub tgs_preset: Option<String>,
    /// How many threads conversions use, as `--threads` says.
    pub threads: Threads,
    /// Which ffmpeg to use.
    pub ffmpeg: FfmpegConfig,
    /// The GUI's settings, which the GUI reads and words itself: kept
    /// here as written, and saved again with the rest. They aren't held
    /// to this file's strictness, so a setting a newer GUI writes
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

/// Which ffmpeg to use: the `[ffmpeg]` table of the config file.
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

/// How many threads a conversion uses at once: as many as each part of it
/// makes use of, or at most a number. Written `"auto"` or as the number,
/// as the config file and `--threads` take it.
///
/// A `.tgs` comes out the same whatever it is; a WebM can come out a few
/// bytes apart, as libvpx encodes a little differently in another number
/// of threads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Threads {
    /// ffmpeg's own choice, which goes by the machine's cores, and for
    /// `.tgs` a thread for each candidate fitting tries at once, up to as
    /// many as the machine runs.
    #[default]
    Auto,
    /// At most this many: ffmpeg decodes, filters and encodes in this
    /// many, and fitting a `.tgs` tries this many candidates at once.
    Limit(NonZeroU32),
}

impl Threads {
    /// The most threads, unless it is automatic.
    pub fn limit(self) -> Option<NonZeroU32> {
        match self {
            Threads::Auto => None,
            Threads::Limit(threads) => Some(threads),
        }
    }
}

impl std::fmt::Display for Threads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Threads::Auto => f.write_str("auto"),
            Threads::Limit(threads) => write!(f, "{threads}"),
        }
    }
}

impl std::str::FromStr for Threads {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim() {
            "auto" => Ok(Threads::Auto),
            text => match text.parse::<u32>() {
                Ok(threads) => Threads::try_from(u64::from(threads)),
                Err(_) => Err(format!("{s:?}: expected auto or a number of threads, from 1")),
            },
        }
    }
}

impl TryFrom<u64> for Threads {
    type Error = String;

    fn try_from(threads: u64) -> std::result::Result<Self, Self::Error> {
        u32::try_from(threads)
            .ok()
            .and_then(NonZeroU32::new)
            .map(Threads::Limit)
            .ok_or_else(|| format!("{threads} threads: expected at least 1, or auto"))
    }
}

impl Serialize for Threads {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Threads::Auto => serializer.serialize_str("auto"),
            Threads::Limit(threads) => serializer.serialize_u32(threads.get()),
        }
    }
}

impl<'de> Deserialize<'de> for Threads {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = Threads;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("\"auto\" or a number of threads, from 1")
            }

            fn visit_str<E: serde::de::Error>(self, text: &str) -> std::result::Result<Threads, E> {
                match text {
                    "auto" => Ok(Threads::Auto),
                    _ => Err(E::invalid_value(serde::de::Unexpected::Str(text), &self)),
                }
            }

            fn visit_u64<E: serde::de::Error>(
                self,
                threads: u64,
            ) -> std::result::Result<Threads, E> {
                Threads::try_from(threads).map_err(E::custom)
            }

            fn visit_i64<E: serde::de::Error>(
                self,
                threads: i64,
            ) -> std::result::Result<Threads, E> {
                match u64::try_from(threads) {
                    Ok(threads) => self.visit_u64(threads),
                    Err(_) => Err(E::invalid_value(serde::de::Unexpected::Signed(threads), &self)),
                }
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

impl JsonSchema for Threads {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Threads".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [{ "const": "auto" }, { "type": "integer", "minimum": 1 }],
            "description": "How many threads a conversion uses at once: auto, as many as \
                            each part of it makes use of, or at most a number"
        })
    }
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
    fn starts_from_the_example_without_another_machines_paths() {
        let example: Config = toml::from_str(&starting_example()).unwrap();
        assert_eq!(example.ffmpeg.path, None);
        assert!(!example.gui.contains_key("output-dir"));
        assert_eq!(example.preset.as_deref(), Some("balanced"));
        assert!(starting_example().contains("# path = "));
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
    fn leaves_the_gui_its_own_settings() {
        // what the GUI writes is its own to read, even keys it learns
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
    fn reads_and_writes_threads() {
        let threads = |text: &str| toml::from_str::<Config>(text).map(|config| config.threads);
        assert_eq!(threads("").unwrap(), Threads::Auto);
        assert_eq!(threads("threads = 'auto'").unwrap(), Threads::Auto);
        let four = Threads::Limit(NonZeroU32::new(4).unwrap());
        assert_eq!(threads("threads = 4").unwrap(), four);
        for wrong in ["threads = 0", "threads = -2", "threads = 'all'", "threads = 1.5"] {
            assert!(threads(wrong).is_err(), "{wrong}");
        }
        // as the command line writes it
        assert_eq!("auto".parse::<Threads>(), Ok(Threads::Auto));
        assert_eq!("4".parse::<Threads>(), Ok(four));
        assert_eq!(four.to_string().parse::<Threads>(), Ok(four));
        assert!("0".parse::<Threads>().unwrap_err().contains("at least 1"));
        assert!("many".parse::<Threads>().is_err());
        // and back from the file
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config { threads: four, ..Config::default() };
        config.save(&path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("threads = 4"));
        assert_eq!(Config::load(&path).unwrap(), config);
        let auto = toml::to_string(&Config::default()).unwrap();
        assert!(auto.contains("threads = \"auto\""), "{auto}");
        // JSON, as the events' plans carry it
        assert_eq!(serde_json::to_value(four).unwrap(), serde_json::json!(4));
        assert_eq!(serde_json::from_str::<Threads>("\"auto\"").unwrap(), Threads::Auto);
    }

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(&dir.path().join("config.toml")).unwrap(), Config::default());
    }
}

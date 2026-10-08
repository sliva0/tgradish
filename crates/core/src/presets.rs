//! Named sets of options: a few built in, more from TOML files in the
//! `presets` directory of the config dir.
//!
//! A preset file looks like this, and is named after the preset:
//!
//! ```toml
//! description = "Small and fast, for previews"
//! extends = "sticker"  # optional, options of the base preset come first
//!
//! [options]
//! fit = "bitrate"
//! speed = "fast"
//! ```
//!
//! A user preset with the name of a built-in one replaces it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::options::{Fit, Options, Speed};
use crate::telegram::Target;

pub const DEFAULT_PRESET: &str = "sticker";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct PresetFile {
    pub description: String,
    pub extends: Option<String>,
    pub options: Options,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Preset {
    pub name: String,
    pub description: String,
    pub extends: Option<String>,
    /// Options set by this preset itself, without the presets it extends.
    pub options: Options,
    /// File the preset was loaded from, `None` for built-in presets.
    pub path: Option<PathBuf>,
}

fn builtin() -> Vec<Preset> {
    let preset = |name: &str, description: &str, extends: Option<&str>, options| Preset {
        name: name.into(),
        description: description.into(),
        extends: extends.map(Into::into),
        options,
        path: None,
    };
    vec![
        preset(
            "sticker",
            "Video sticker, 512 px on the longer side",
            None,
            Options { target: Some(Target::Sticker), ..Default::default() },
        ),
        preset(
            "emoji",
            "Custom emoji, 100x100 px",
            None,
            Options { target: Some(Target::Emoji), ..Default::default() },
        ),
        preset(
            "fast",
            "Sticker in a few seconds: fits bitrate only, with the fast encoder",
            Some("sticker"),
            Options { fit: Some(Fit::Bitrate), speed: Some(Speed::Fast), ..Default::default() },
        ),
    ]
}

/// All presets, by name. Broken preset files are kept as errors so that
/// one bad file does not break the others.
#[derive(Debug, Clone)]
pub struct Presets {
    entries: BTreeMap<String, std::result::Result<Preset, String>>,
}

impl Presets {
    /// Built-in presets only.
    pub fn builtin() -> Self {
        Self { entries: builtin().into_iter().map(|p| (p.name.clone(), Ok(p))).collect() }
    }

    /// Built-in presets plus the `*.toml` files in `dir`, if it exists.
    pub fn load(dir: Option<&Path>) -> Result<Self> {
        let mut presets = Self::builtin();
        let Some(dir) = dir.filter(|dir| dir.is_dir()) else { return Ok(presets) };
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "toml") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let loaded = std::fs::read_to_string(&path)
                .map_err(|err| err.to_string())
                .and_then(|text| toml::from_str::<PresetFile>(&text).map_err(|err| err.to_string()))
                .map(|file| Preset {
                    name: name.to_string(),
                    description: file.description,
                    extends: file.extends,
                    options: file.options,
                    path: Some(path.clone()),
                })
                .map_err(|err| format!("{}: {err}", path.display()));
            presets.entries.insert(name.to_string(), loaded);
        }
        Ok(presets)
    }

    /// Presets from the user's config dir, see [`crate::paths::config_dir`].
    pub fn load_user() -> Result<Self> {
        Self::load(user_dir().as_deref())
    }

    /// Every preset, with an error message for the ones that failed to load.
    pub fn iter(&self) -> impl Iterator<Item = (&str, std::result::Result<&Preset, &str>)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.as_ref().map_err(|e| e.as_str())))
    }

    pub fn get(&self, name: &str) -> Result<&Preset> {
        match self.entries.get(name) {
            Some(Ok(preset)) => Ok(preset),
            Some(Err(message)) => Err(Error::InvalidOptions(format!("broken preset {message}"))),
            None => {
                let known: Vec<_> = self.entries.keys().map(String::as_str).collect();
                Err(Error::InvalidOptions(format!(
                    "unknown preset {name:?}, available: {}",
                    known.join(", ")
                )))
            }
        }
    }

    /// Options of `name` with everything it extends applied first.
    pub fn resolve(&self, name: &str) -> Result<Options> {
        let mut chain = vec![self.get(name)?];
        while let Some(base) = &chain.last().unwrap().extends {
            if chain.iter().any(|p| &p.name == base) {
                return Err(Error::InvalidOptions(format!("preset {name:?} extends itself")));
            }
            chain.push(self.get(base)?);
        }
        Ok(chain.iter().rev().fold(Options::default(), |acc, p| acc.merged(&p.options)))
    }
}

/// Directory with user preset files.
pub fn user_dir() -> Option<PathBuf> {
    Some(crate::paths::config_dir()?.join("presets"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(format!("{name}.toml")), text).unwrap();
    }

    #[test]
    fn resolves_builtin_chain() {
        let presets = Presets::builtin();
        let fast = presets.resolve("fast").unwrap();
        assert_eq!(fast.target, Some(Target::Sticker));
        assert_eq!(fast.speed, Some(Speed::Fast));
        assert!(presets.resolve("nope").is_err());
    }

    #[test]
    fn loads_user_presets() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tiny",
            "description = 'tiny emoji'\nextends = 'emoji'\n[options]\nfps = 10.0\n",
        );
        write(dir.path(), "sticker", "[options]\ncrf = 20\n");
        write(dir.path(), "broken", "[options]\nfsp = 10\n");
        write(dir.path(), "loop-a", "extends = 'loop-b'\n");
        write(dir.path(), "loop-b", "extends = 'loop-a'\n");
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let presets = Presets::load(Some(dir.path())).unwrap();
        let tiny = presets.resolve("tiny").unwrap();
        assert_eq!((tiny.target, tiny.fps), (Some(Target::Emoji), Some(10.0)));
        // the user file replaced the built-in sticker preset
        assert_eq!(presets.resolve("sticker").unwrap().target, None);
        assert_eq!(presets.resolve("fast").unwrap().crf, Some(20));

        let broken = presets.resolve("broken").unwrap_err().to_string();
        assert!(broken.contains("broken.toml") && broken.contains("fsp"), "{broken}");
        assert!(presets.resolve("loop-a").unwrap_err().to_string().contains("extends itself"));
        assert_eq!(presets.iter().count(), 7);
    }
}

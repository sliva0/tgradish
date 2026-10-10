//! Named sets of options that say how to convert: three built in (fast,
//! balanced and best), more from TOML files in the `presets` directory of
//! the config dir. What to make, a sticker or an emoji as WebM or `.tgs`,
//! is chosen separately, so one preset serves every kind of result.
//!
//! A preset file looks like this, and is named after the preset:
//!
//! ```toml
//! description = "Small and fast, for previews"
//! extends = "fast"  # optional, options of the base preset come first
//!
//! [webm]            # options for WebM results
//! crf = 40
//!
//! [tgs]             # options for .tgs results
//! speed = "balanced"
//! ```
//!
//! Either table can be left out. A user preset with the name of a built-in
//! one replaces it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::options::{Fit, Options, Speed};
use crate::tgs::TgsOptions;

/// The preset used for WebM output when none is given.
pub const DEFAULT_PRESET: &str = "balanced";
/// The preset used for `.tgs` output when none is given: its best encoding
/// takes seconds, not minutes.
pub const DEFAULT_TGS_PRESET: &str = "best";

/// What kind of file a conversion makes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Video stickers and emoji, from any video or image.
    #[default]
    Webm,
    /// Animated stickers, from pixel art.
    Tgs,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Webm => "webm",
            Format::Tgs => "tgs",
        }
    }

    /// The format a file name asks for, by its extension.
    pub fn of_path(path: &Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "webm" => Some(Format::Webm),
            "tgs" => Some(Format::Tgs),
            _ => None,
        }
    }

    pub fn default_preset(self) -> &'static str {
        match self {
            Format::Webm => DEFAULT_PRESET,
            Format::Tgs => DEFAULT_TGS_PRESET,
        }
    }
}

/// Options of either format.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum FormatOptions {
    Webm(Options),
    Tgs(TgsOptions),
}

impl FormatOptions {
    pub fn format(&self) -> Format {
        match self {
            FormatOptions::Webm(_) => Format::Webm,
            FormatOptions::Tgs(_) => Format::Tgs,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct PresetFile {
    description: String,
    extends: Option<String>,
    webm: toml::Table,
    tgs: toml::Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Preset {
    pub name: String,
    pub description: String,
    pub extends: Option<String>,
    /// Options for WebM results set by this preset itself, without the
    /// presets it extends, as written in the file.
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub webm: toml::Table,
    /// The same for `.tgs` results.
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub tgs: toml::Table,
    /// File the preset was loaded from, `None` for built-in presets.
    pub path: Option<PathBuf>,
}

fn table(options: &impl Serialize) -> toml::Table {
    toml::Table::try_from(options).expect("options serialize to a table")
}

fn builtin() -> Vec<Preset> {
    let preset = |name: &str, description: &str, webm: Options, tgs: TgsOptions| Preset {
        name: name.into(),
        description: description.into(),
        extends: None,
        webm: table(&webm),
        tgs: table(&tgs),
        path: None,
    };
    let speed = |speed| TgsOptions { speed: Some(speed), ..Default::default() };
    vec![
        preset(
            "fast",
            "Quick: WebM fits only the bitrate, with the fast encoder; .tgs comes out a few \
             percent larger",
            Options { fit: Some(Fit::Bitrate), speed: Some(Speed::Fast), ..Default::default() },
            speed(Speed::Fast),
        ),
        preset(
            "balanced",
            "Good looking WebM in reasonable time: tries a few frame rates",
            Options { fit: Some(Fit::Auto), speed: Some(Speed::Balanced), ..Default::default() },
            speed(Speed::Balanced),
        ),
        preset(
            "best",
            "Best looking WebM, slowly; the smallest .tgs, in seconds",
            Options { fit: Some(Fit::Auto), speed: Some(Speed::Best), ..Default::default() },
            speed(Speed::Best),
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
                    webm: file.webm,
                    tgs: file.tgs,
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

    /// `name` and the presets it extends, itself first.
    fn chain(&self, name: &str) -> Result<Vec<&Preset>> {
        let mut chain = vec![self.get(name)?];
        while let Some(base) = &chain.last().unwrap().extends {
            if chain.iter().any(|p| &p.name == base) {
                return Err(Error::InvalidOptions(format!("preset {name:?} extends itself")));
            }
            chain.push(self.get(base)?);
        }
        Ok(chain)
    }

    /// Options of `name` for `format`, with everything it extends applied
    /// first.
    pub fn resolve(&self, name: &str, format: Format) -> Result<FormatOptions> {
        Ok(match format {
            Format::Webm => FormatOptions::Webm(self.webm(name)?),
            Format::Tgs => FormatOptions::Tgs(self.tgs(name)?),
        })
    }

    /// Options of `name` for WebM results.
    pub fn webm(&self, name: &str) -> Result<Options> {
        self.fold(name, |preset| &preset.webm, Options::default(), Options::merged)
    }

    /// Options of `name` for `.tgs` results.
    pub fn tgs(&self, name: &str) -> Result<TgsOptions> {
        self.fold(name, |preset| &preset.tgs, TgsOptions::default(), TgsOptions::merged)
    }

    fn fold<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        table: fn(&Preset) -> &toml::Table,
        empty: T,
        merged: fn(T, &T) -> T,
    ) -> Result<T> {
        self.chain(name)?.iter().rev().try_fold(empty, |acc, preset| {
            let options: T = table(preset).clone().try_into().map_err(|err| {
                let place =
                    preset.path.as_ref().map_or(preset.name.clone(), |p| p.display().to_string());
                Error::InvalidOptions(format!("broken preset {place}: {err}"))
            })?;
            Ok(merged(acc, &options))
        })
    }
}

/// Directory with user preset files.
pub fn user_dir() -> Option<PathBuf> {
    Some(crate::paths::config_dir()?.join("presets"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::Target;

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(format!("{name}.toml")), text).unwrap();
    }

    #[test]
    fn resolves_builtin_presets_for_either_format() {
        let presets = Presets::builtin();
        let fast = presets.webm("fast").unwrap();
        assert_eq!(
            (fast.fit, fast.speed, fast.target),
            (Some(Fit::Bitrate), Some(Speed::Fast), None)
        );
        assert_eq!(presets.tgs("fast").unwrap().speed, Some(Speed::Fast));
        assert!(matches!(presets.resolve("best", Format::Tgs), Ok(FormatOptions::Tgs(_))));
        assert!(presets.webm("nope").is_err());
        for format in [Format::Webm, Format::Tgs] {
            assert!(presets.get(format.default_preset()).is_ok());
        }
    }

    #[test]
    fn loads_user_presets() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tiny",
            "description = 'tiny emoji'\nextends = 'fast'\n[webm]\ntarget = 'emoji'\nfps = 10.0\n",
        );
        write(dir.path(), "balanced", "[webm]\ncrf = 20\n");
        write(dir.path(), "broken", "[webm]\nfsp = 10\n");
        write(dir.path(), "loop-a", "extends = 'loop-b'\n");
        write(dir.path(), "loop-b", "extends = 'loop-a'\n");
        write(dir.path(), "pixels", "extends = 'best'\n[tgs]\nkeep-canvas = true\n");
        write(dir.path(), "wrong", "[tgs]\ncrf = 20\n");
        write(dir.path(), "old", "format = 'tgs'\n[options]\nspeed = 'fast'\n");
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let presets = Presets::load(Some(dir.path())).unwrap();
        let tiny = presets.webm("tiny").unwrap();
        assert_eq!(
            (tiny.target, tiny.fps, tiny.speed),
            (Some(Target::Emoji), Some(10.0), Some(Speed::Fast))
        );
        // only WebM options were set, the rest comes from the base
        assert_eq!(presets.tgs("tiny").unwrap().speed, Some(Speed::Fast));
        // the user file replaced the built-in preset
        assert_eq!(
            presets.webm("balanced").unwrap(),
            Options { crf: Some(20), ..Default::default() }
        );
        let pixels = presets.tgs("pixels").unwrap();
        assert_eq!((pixels.keep_canvas, pixels.speed), (Some(true), Some(Speed::Best)));

        let broken = presets.webm("broken").unwrap_err().to_string();
        assert!(broken.contains("broken.toml") && broken.contains("fsp"), "{broken}");
        // broken for one format only
        assert!(presets.tgs("broken").is_ok());
        assert!(presets.webm("loop-a").unwrap_err().to_string().contains("extends itself"));
        // crf isn't a .tgs option
        assert!(presets.tgs("wrong").unwrap_err().to_string().contains("crf"));
        assert!(presets.get("old").is_err());
        assert_eq!(presets.iter().count(), 10);
    }
}

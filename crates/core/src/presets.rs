//! Named sets of options: a few built in, more from TOML files in the
//! `presets` directory of the config dir.
//!
//! A preset file looks like this, and is named after the preset:
//!
//! ```toml
//! description = "Small and fast, for previews"
//! extends = "sticker"  # optional, options of the base preset come first
//! format = "webm"      # optional: webm or tgs, by default the base's
//!
//! [options]
//! fit = "bitrate"
//! speed = "fast"
//! ```
//!
//! The options are those of the preset's format. A user preset with the
//! name of a built-in one replaces it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::options::{Fit, Options, Speed};
use crate::telegram::Target;
use crate::tgs::TgsOptions;

/// The preset used for WebM output when none is given.
pub const DEFAULT_PRESET: &str = "sticker";
/// The preset used for `.tgs` output when none is given.
pub const DEFAULT_TGS_PRESET: &str = "tgs-sticker";

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
    format: Option<Format>,
    options: toml::Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Preset {
    pub name: String,
    pub description: String,
    pub extends: Option<String>,
    /// The format this preset sets; others take their base's.
    pub format: Option<Format>,
    /// Options set by this preset itself, without the presets it extends,
    /// as written in the file.
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub options: toml::Table,
    /// File the preset was loaded from, `None` for built-in presets.
    pub path: Option<PathBuf>,
}

fn table(options: &impl Serialize) -> toml::Table {
    toml::Table::try_from(options).expect("options serialize to a table")
}

fn builtin() -> Vec<Preset> {
    let preset = |name: &str, description: &str, extends: Option<&str>, format, options| Preset {
        name: name.into(),
        description: description.into(),
        extends: extends.map(Into::into),
        format,
        options,
        path: None,
    };
    let webm = |options: Options| table(&options);
    let tgs = |options: TgsOptions| table(&options);
    vec![
        preset(
            "sticker",
            "Video sticker, 512 px on the longer side",
            None,
            Some(Format::Webm),
            webm(Options { target: Some(Target::Sticker), ..Default::default() }),
        ),
        preset(
            "emoji",
            "Custom emoji, 100x100 px",
            None,
            Some(Format::Webm),
            webm(Options { target: Some(Target::Emoji), ..Default::default() }),
        ),
        preset(
            "fast",
            "Sticker in a few seconds: fits bitrate only, with the fast encoder",
            Some("sticker"),
            None,
            webm(Options {
                fit: Some(Fit::Bitrate),
                speed: Some(Speed::Fast),
                ..Default::default()
            }),
        ),
        preset(
            "tgs-sticker",
            "Animated sticker from pixel art",
            None,
            Some(Format::Tgs),
            tgs(TgsOptions { target: Some(Target::Sticker), ..Default::default() }),
        ),
        preset(
            "tgs-emoji",
            "Animated custom emoji from pixel art",
            None,
            Some(Format::Tgs),
            tgs(TgsOptions { target: Some(Target::Emoji), ..Default::default() }),
        ),
        preset(
            "tgs-fast",
            "Animated sticker in under a second, a few percent larger",
            Some("tgs-sticker"),
            None,
            tgs(TgsOptions { speed: Some(Speed::Fast), ..Default::default() }),
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
                    format: file.format,
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

    /// The format of `name`: its own, or the first one a base sets.
    pub fn format(&self, name: &str) -> Result<Format> {
        let chain = self.chain(name)?;
        let mut formats = chain.iter().filter_map(|preset| preset.format.map(|f| (preset, f)));
        let Some((_, format)) = formats.next() else { return Ok(Format::default()) };
        if let Some((other, _)) = formats.find(|(_, other)| *other != format) {
            return Err(Error::InvalidOptions(format!(
                "preset {name:?} is for {}, but extends {:?}, which is for {}",
                format.extension(),
                other.name,
                other.format.unwrap_or_default().extension()
            )));
        }
        Ok(format)
    }

    /// Options of `name` with everything it extends applied first.
    pub fn resolve(&self, name: &str) -> Result<FormatOptions> {
        let format = self.format(name)?;
        let chain = self.chain(name)?;
        let invalid = |preset: &Preset, err: toml::de::Error| {
            let place =
                preset.path.as_ref().map_or(preset.name.clone(), |p| p.display().to_string());
            Error::InvalidOptions(format!("broken preset {place}: {err}"))
        };
        Ok(match format {
            Format::Webm => FormatOptions::Webm(chain.iter().rev().try_fold(
                Options::default(),
                |acc, preset| {
                    let options: Options =
                        preset.options.clone().try_into().map_err(|err| invalid(preset, err))?;
                    Ok::<_, Error>(acc.merged(&options))
                },
            )?),
            Format::Tgs => FormatOptions::Tgs(chain.iter().rev().try_fold(
                TgsOptions::default(),
                |acc, preset| {
                    let options: TgsOptions =
                        preset.options.clone().try_into().map_err(|err| invalid(preset, err))?;
                    Ok::<_, Error>(acc.merged(&options))
                },
            )?),
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

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(format!("{name}.toml")), text).unwrap();
    }

    fn webm(options: FormatOptions) -> Options {
        match options {
            FormatOptions::Webm(options) => options,
            other => panic!("{other:?} is not for webm"),
        }
    }

    #[test]
    fn resolves_builtin_chain() {
        let presets = Presets::builtin();
        let fast = webm(presets.resolve("fast").unwrap());
        assert_eq!(fast.target, Some(Target::Sticker));
        assert_eq!(fast.speed, Some(Speed::Fast));
        assert!(presets.resolve("nope").is_err());
        let FormatOptions::Tgs(tgs) = presets.resolve("tgs-fast").unwrap() else { panic!() };
        assert_eq!((tgs.target, tgs.speed), (Some(Target::Sticker), Some(Speed::Fast)));
        assert_eq!(presets.format("tgs-fast").unwrap(), Format::Tgs);
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
        write(dir.path(), "pixels", "extends = 'tgs-sticker'\n[options]\nkeep-canvas = true\n");
        write(dir.path(), "mixed", "extends = 'tgs-sticker'\nformat = 'webm'\n");
        write(dir.path(), "wrong", "format = 'tgs'\n[options]\ncrf = 20\n");
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let presets = Presets::load(Some(dir.path())).unwrap();
        let tiny = webm(presets.resolve("tiny").unwrap());
        assert_eq!((tiny.target, tiny.fps), (Some(Target::Emoji), Some(10.0)));
        // the user file replaced the built-in sticker preset
        assert_eq!(webm(presets.resolve("sticker").unwrap()).target, None);
        assert_eq!(webm(presets.resolve("fast").unwrap()).crf, Some(20));
        // the format comes from the base
        let FormatOptions::Tgs(pixels) = presets.resolve("pixels").unwrap() else { panic!() };
        assert_eq!((pixels.keep_canvas, pixels.target), (Some(true), Some(Target::Sticker)));

        let broken = presets.resolve("broken").unwrap_err().to_string();
        assert!(broken.contains("broken.toml") && broken.contains("fsp"), "{broken}");
        assert!(presets.resolve("loop-a").unwrap_err().to_string().contains("extends itself"));
        assert!(presets.resolve("mixed").unwrap_err().to_string().contains("for tgs"));
        // crf isn't a .tgs option
        assert!(presets.resolve("wrong").unwrap_err().to_string().contains("crf"));
        assert_eq!(presets.iter().count(), 13);
    }
}

//! Named sets of options that say how to convert: three built in (fast,
//! balanced and best), more from TOML files in the `presets` directory of
//! the config dir. What to make, a sticker or an emoji as WebM, `.tgs` or
//! an image, is chosen separately, so one preset serves every kind of
//! result.
//!
//! A preset file looks like this, and is named after the preset:
//!
//! ```toml
//! description = "Small and fast, for previews"
//! extends = "fast"  # optional, options of the base preset come first
//!
//! [webm]            # options for WebM results, and for image results
//! crf = 40          # where a still picture has them
//!
//! [tgs]             # options for TGS results
//! speed = "balanced"
//! ```
//!
//! Either table can be left out. A user preset with the name of a built-in
//! one replaces it. A preset says how to convert, never what: the options
//! of one file, [`PER_FILE`], are refused in one, and left out of one
//! saved.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, FileAction, Result};
use crate::options::{Fit, Speed};
use crate::telegram::Target;
use crate::tgs;
use crate::webm::Options;

/// The built-in presets' names, fastest first.
pub const BUILTIN: [&str; 3] = ["fast", "balanced", "best"];

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
    /// Image stickers and emoji: one frame of a video, or an image, as
    /// WebP. They take WebM's options where a still picture has them.
    /// Read as `image` too, the kind of sticker it makes.
    #[serde(alias = "image")]
    Webp,
}

impl Format {
    /// What `--format` and the JSON call it, which messages name it by.
    pub fn name(self) -> &'static str {
        match self {
            Format::Webm => "webm",
            Format::Tgs => "tgs",
            Format::Webp => "webp",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Webm => "webm",
            Format::Tgs => "tgs",
            Format::Webp => "webp",
        }
    }

    /// The format a file name asks for, by its extension.
    pub fn of_path(path: &Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "webm" => Some(Format::Webm),
            "tgs" => Some(Format::Tgs),
            "webp" => Some(Format::Webp),
            _ => None,
        }
    }

    /// Whether this format uses the option `key`, as files, JSON and flags
    /// name it: WebM all of [`Options`], `.tgs` all of [`tgs::Options`], and
    /// images those of WebM's a still picture has,
    /// [`crate::webp::PICTURE_OPTIONS`].
    pub fn uses(self, key: &str) -> bool {
        static WEBM: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        static TGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        let known = |keys: &std::sync::OnceLock<Vec<String>>, schema: fn() -> schemars::Schema| {
            let keys = keys.get_or_init(|| {
                let schema = serde_json::to_value(schema()).unwrap_or_default();
                schema["properties"].as_object().map_or(Vec::new(), |p| p.keys().cloned().collect())
            });
            keys.iter().any(|known| known == key)
        };
        match self {
            Format::Webm => known(&WEBM, || schemars::schema_for!(Options)),
            Format::Tgs => known(&TGS, || schemars::schema_for!(tgs::Options)),
            Format::Webp => crate::webp::PICTURE_OPTIONS.contains(&key),
        }
    }

    /// Those of `keys` this format doesn't use, as [`Format::uses`] says,
    /// in their order: what a front-end refuses rather than quietly drop.
    pub fn unused<'k>(self, keys: impl IntoIterator<Item = &'k str>) -> Vec<&'k str> {
        let mut unused: Vec<&str> = Vec::new();
        for key in keys {
            if !self.uses(key) && !unused.contains(&key) {
                unused.push(key);
            }
        }
        unused
    }

    pub fn default_preset(self) -> &'static str {
        match self {
            Format::Webm | Format::Webp => DEFAULT_PRESET,
            Format::Tgs => DEFAULT_TGS_PRESET,
        }
    }
}

/// Options of either format.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum FormatOptions {
    Webm(Options),
    Tgs(tgs::Options),
    /// WebM's options, for an image.
    Webp(Options),
}

impl FormatOptions {
    pub fn format(&self) -> Format {
        match self {
            FormatOptions::Webm(_) => Format::Webm,
            FormatOptions::Tgs(_) => Format::Tgs,
            FormatOptions::Webp(_) => Format::Webp,
        }
    }

    /// What they make, a sticker unless they say.
    pub fn target(&self) -> Target {
        match self {
            FormatOptions::Webm(options) | FormatOptions::Webp(options) => options.target,
            FormatOptions::Tgs(options) => options.target,
        }
        .unwrap_or_default()
    }
}

/// The options that say something of one file only: what to make, the
/// part of it to use and its time, its tag and layers, its title, and how a
/// sprite sheet is laid out. A preset says how to convert any file, so it
/// holds none of them: they are refused in one's file, and left out of
/// one saved. The size of the art's pixels isn't among them, as files from
/// one place often share it.
pub const PER_FILE: [&str; 10] = [
    "target",
    "tag",
    "show-layers",
    "hide-layers",
    "crop",
    "start",
    "length",
    "title",
    "sheet",
    "sheet-frames",
];

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
    /// Options for WebM and image results set by this preset itself,
    /// without the presets it extends: those its file sets.
    pub webm: Options,
    /// The same for `.tgs` results.
    pub tgs: tgs::Options,
    /// File the preset was loaded from, `None` for built-in presets.
    pub path: Option<PathBuf>,
}

impl Preset {
    /// The preset in `text`, a preset file at `path`, named `name`.
    fn read(name: &str, path: &Path, text: &str) -> std::result::Result<Preset, PresetError> {
        let place = path.display().to_string();
        // TOML's errors end in a line break, which a message shouldn't
        let broken = |err: &dyn std::fmt::Display| {
            PresetError::Broken(format!("{place}: {}", err.to_string().trim_end()))
        };
        let file: PresetFile = toml::from_str(text).map_err(|err| broken(&err))?;
        for (table, options) in [("webm", &file.webm), ("tgs", &file.tgs)] {
            if let Some(key) = options.keys().find(|key| PER_FILE.contains(&key.as_str())) {
                return Err(PresetError::PerFile { place, key: format!("{table}.{key}") });
            }
        }
        let webm = file.webm.try_into().map_err(|err| broken(&format_args!("[webm]: {err}")))?;
        let tgs = file.tgs.try_into().map_err(|err| broken(&format_args!("[tgs]: {err}")))?;
        Ok(Preset {
            name: name.to_owned(),
            description: file.description,
            extends: file.extends,
            webm,
            tgs,
            path: Some(path.to_path_buf()),
        })
    }
}

fn table(options: &impl Serialize) -> toml::Table {
    toml::Table::try_from(options).expect("options serialise to a table")
}

fn builtin() -> Vec<Preset> {
    let preset = |name: &str, description: &str, webm: Options, tgs: tgs::Options| Preset {
        name: name.into(),
        description: description.into(),
        extends: None,
        webm,
        tgs,
        path: None,
    };
    let speed = |speed| tgs::Options { speed: Some(speed), ..Default::default() };
    vec![
        preset(
            "fast",
            "WebM fits only the bitrate, with the fast encoder: takes seconds, but blurrier. TGS \
             takes under a second, a few percent larger",
            Options { fit: Some(Fit::Bitrate), speed: Some(Speed::Fast), ..Default::default() },
            speed(Speed::Fast),
        ),
        preset(
            "balanced",
            "WebM tries a few frame rates and keeps the one that looks best. TGS is between fast \
             and best",
            Options { fit: Some(Fit::Auto), speed: Some(Speed::Balanced), ..Default::default() },
            speed(Speed::Balanced),
        ),
        preset(
            "best",
            "WebM is like balanced, with the slowest encoder: takes minutes for long videos. TGS \
             takes a few seconds, the smallest file",
            Options { fit: Some(Fit::Auto), speed: Some(Speed::Best), ..Default::default() },
            speed(Speed::Best),
        ),
    ]
}

/// Why a preset can't be had, or saved, renamed or described as asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PresetError {
    /// No preset has the name; `known` are those that do.
    #[error("unknown preset {name:?}, available: {}", known.join(", "))]
    Unknown { name: String, known: Vec<String> },
    /// Its file, or that of one it extends, isn't a preset's: the message
    /// names it and says why.
    #[error("broken preset {0}")]
    Broken(String),
    #[error("preset {0:?} extends itself")]
    ExtendsItself(String),
    /// A name too short, too long or starting with a space.
    #[error("a preset's name has 1 to {LONGEST_NAME} characters, not starting with a space")]
    NameLength,
    /// A name with what a file's name shouldn't have.
    #[error("a preset's name has only letters, digits, spaces, - and _")]
    NameCharacters,
    /// Another preset, built in or not, has the name.
    #[error("another preset is named {0:?}")]
    NameTaken(String),
    /// Renaming or deleting it would break the preset named, which
    /// extends it.
    #[error("{0:?} extends it: change that first")]
    Extended(String),
    /// A preset's file sets an option of one file, one of [`PER_FILE`],
    /// as `key` names it in its table, `webm.crop`; `place` is the file.
    #[error("broken preset {place}: {key} is one file's to choose, which a preset says nothing of")]
    PerFile { place: String, key: String },
}

/// All presets, by name. Broken preset files are kept as errors so that
/// one bad file does not break the others.
#[derive(Debug, Clone)]
pub struct Presets {
    entries: BTreeMap<String, std::result::Result<Preset, PresetError>>,
    /// Each preset's options with what it extends applied, worked out
    /// once as presets load: the window asks for them many times a frame,
    /// and each would otherwise merge the whole chain again.
    resolved: BTreeMap<String, Resolved>,
}

/// A preset's options for either kind of result, or why they can't be
/// had.
#[derive(Debug, Clone)]
struct Resolved {
    webm: std::result::Result<Options, PresetError>,
    tgs: std::result::Result<tgs::Options, PresetError>,
}

impl Presets {
    /// Built-in presets only.
    pub fn builtin() -> Self {
        let entries = builtin().into_iter().map(|p| (p.name.clone(), Ok(p))).collect();
        Self { entries, resolved: BTreeMap::new() }.resolved()
    }

    /// The presets with every one's options worked out.
    fn resolved(mut self) -> Self {
        let message = |err: Error| match err {
            Error::Preset(err) => err,
            other => PresetError::Broken(other.to_string()),
        };
        self.resolved = self
            .entries
            .keys()
            .map(|name| {
                let webm = self.fold(name, |preset| &preset.webm, Options::merged);
                let tgs = self.fold(name, |preset| &preset.tgs, tgs::Options::merged);
                let resolved = Resolved { webm: webm.map_err(message), tgs: tgs.map_err(message) };
                (name.clone(), resolved)
            })
            .collect();
        self
    }

    /// Built-in presets plus the `*.toml` files in `dir`, if it exists.
    pub fn load(dir: Option<&Path>) -> Result<Self> {
        let mut presets = Self::builtin();
        let Some(dir) = dir.filter(|dir| dir.is_dir()) else { return Ok(presets) };
        let list = |err| Error::io(FileAction::List, dir)(err);
        for entry in std::fs::read_dir(dir).map_err(list)? {
            let path = entry.map_err(list)?.path();
            if path.extension().is_none_or(|ext| ext != "toml") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let loaded = match std::fs::read_to_string(&path) {
                Ok(text) => Preset::read(name, &path, &text),
                Err(err) => Err(PresetError::Broken(format!("{}: {err}", path.display()))),
            };
            presets.entries.insert(name.to_string(), loaded);
        }
        Ok(presets.resolved())
    }

    /// Presets from the user's config dir, see [`crate::paths::config_dir`].
    pub fn load_user() -> Result<Self> {
        Self::load(user_dir().as_deref())
    }

    /// Every preset, with why for the ones that failed to load.
    pub fn iter(&self) -> impl Iterator<Item = (&str, std::result::Result<&Preset, &PresetError>)> {
        self.entries.iter().map(|(name, entry)| (name.as_str(), entry.as_ref()))
    }

    pub fn get(&self, name: &str) -> Result<&Preset> {
        match self.entries.get(name) {
            Some(Ok(preset)) => Ok(preset),
            Some(Err(err)) => Err(err.clone().into()),
            None => {
                let known = self.entries.keys().cloned().collect();
                Err(PresetError::Unknown { name: name.to_owned(), known }.into())
            }
        }
    }

    /// `name` and the presets it extends, itself first.
    fn chain(&self, name: &str) -> Result<Vec<&Preset>> {
        let mut chain = vec![self.get(name)?];
        while let Some(base) = &chain.last().unwrap().extends {
            if chain.iter().any(|p| &p.name == base) {
                return Err(PresetError::ExtendsItself(name.to_owned()).into());
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
            Format::Webp => FormatOptions::Webp(self.webm(name)?),
        })
    }

    /// Options of `name` for WebM results.
    pub fn webm(&self, name: &str) -> Result<Options> {
        match self.resolved.get(name) {
            Some(resolved) => resolved.webm.clone().map_err(Error::Preset),
            None => Err(self.get(name).expect_err("every preset is resolved")),
        }
    }

    /// Options of `name` for `.tgs` results.
    pub fn tgs(&self, name: &str) -> Result<tgs::Options> {
        match self.resolved.get(name) {
            Some(resolved) => resolved.tgs.clone().map_err(Error::Preset),
            None => Err(self.get(name).expect_err("every preset is resolved")),
        }
    }

    /// The options `options` takes from each preset of `name`'s chain,
    /// merged from the base up.
    fn fold<T: Default>(
        &self,
        name: &str,
        options: fn(&Preset) -> &T,
        merged: fn(T, &T) -> T,
    ) -> Result<T> {
        let chain = self.chain(name)?;
        Ok(chain.iter().rev().fold(T::default(), |acc, preset| merged(acc, options(preset))))
    }

    /// The user's preset another extends, which renaming or deleting
    /// `name` would break.
    fn extending(&self, name: &str) -> Option<&Preset> {
        self.iter()
            .filter_map(|(_, preset)| preset.ok())
            .find(|preset| preset.extends.as_deref() == Some(name))
    }
}

/// Directory with user preset files.
pub fn user_dir() -> Option<PathBuf> {
    Some(crate::paths::config_dir()?.join("presets"))
}

/// Longest name a preset saved here can have.
pub const LONGEST_NAME: usize = 64;

/// Whether `name` can name a preset file: letters, digits, spaces, `-`
/// and `_`, not starting with a space, at most 64 of them.
pub fn check_name(name: &str) -> Result<()> {
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_');
    if name.trim().is_empty() || name.starts_with(' ') || name.chars().count() > LONGEST_NAME {
        return Err(PresetError::NameLength.into());
    }
    if !name.chars().all(allowed) {
        return Err(PresetError::NameCharacters.into());
    }
    Ok(())
}

/// The options of `options` a preset keeps: all but [`PER_FILE`].
pub fn shareable<T: Serialize + serde::de::DeserializeOwned>(options: &T) -> T {
    let mut table = table(options);
    table.retain(|key, _| !PER_FILE.contains(&key));
    table.try_into().expect("options read back with fewer set")
}

/// Writes a preset named `name` into `dir`, said to be `description`, of
/// `webm` and `tgs` without what only makes sense for one file
/// ([`PER_FILE`]), and returns its file. One there already is replaced
/// only with `replace`.
pub fn save(
    dir: &Path,
    name: &str,
    description: &str,
    (webm, tgs): (&Options, &tgs::Options),
    replace: bool,
) -> Result<PathBuf> {
    check_name(name)?;
    let mut file = toml::Table::new();
    if !description.is_empty() {
        file.insert("description".into(), description.into());
    }
    file.insert("webm".into(), table(&shareable(webm)).into());
    file.insert("tgs".into(), table(&shareable(tgs)).into());
    let text = toml::to_string(&file).map_err(|err| PresetError::Broken(err.to_string()))?;
    std::fs::create_dir_all(dir).map_err(Error::io(FileAction::Create, dir))?;
    let path = dir.join(format!("{name}.toml"));
    crate::fsutil::write_output(&path, text.as_bytes(), replace)?;
    Ok(path)
}

/// Whether the preset `from` can be renamed `to`: a name no other preset
/// has, built in or not, for one no other preset extends, as that one
/// would break.
pub fn check_rename(from: &str, to: &str, presets: &Presets) -> Result<()> {
    check_name(to)?;
    if to != from && presets.iter().any(|(name, _)| name == to) {
        return Err(PresetError::NameTaken(to.to_owned()).into());
    }
    if let Some(other) = presets.extending(from) {
        return Err(PresetError::Extended(other.name.clone()).into());
    }
    Ok(())
}

/// Whether the preset `name` can be deleted: not while another extends
/// it, which would break.
pub fn check_remove(name: &str, presets: &Presets) -> Result<()> {
    match presets.extending(name) {
        Some(other) => Err(PresetError::Extended(other.name.clone()).into()),
        None => Ok(()),
    }
}

/// Renames the preset `from` in `dir` to `to`, as [`check_rename`] allows.
pub fn rename(dir: &Path, from: &str, to: &str, presets: &Presets) -> Result<PathBuf> {
    check_rename(from, to, presets)?;
    let (old, new) = (dir.join(format!("{from}.toml")), dir.join(format!("{to}.toml")));
    // a name changed only in case names the same file where case doesn't
    // matter; where it does, the other file is another preset's
    if new.exists() && !same_file(&old, &new) {
        return Err(Error::OutputExists(new));
    }
    std::fs::rename(&old, &new).map_err(Error::io(FileAction::Rename, &old))?;
    Ok(new)
}

/// Whether `a` and `b` are one file, under two names or the same.
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
    }
}

/// Says the preset at `path` is `description`, keeping its options. The
/// file is written anew, without any comments it had.
pub fn set_description(path: &Path, description: &str) -> Result<()> {
    let text = std::fs::read_to_string(path).map_err(Error::io(FileAction::Read, path))?;
    let mut file: toml::Table = toml::from_str(&text)
        .map_err(|err| PresetError::Broken(format!("{}: {err}", path.display())))?;
    file.remove("description");
    if !description.is_empty() {
        file.insert("description".into(), description.into());
    }
    let text = toml::to_string(&file).map_err(|err| PresetError::Broken(err.to_string()))?;
    crate::fsutil::write_output(path, text.as_bytes(), true)
}

/// Deletes the preset `name` in `dir`, as [`check_remove`] allows, and
/// returns what it held, which [`restore`] puts back.
pub fn remove(dir: &Path, name: &str, presets: &Presets) -> Result<String> {
    check_remove(name, presets)?;
    let path = &dir.join(format!("{name}.toml"));
    let text = std::fs::read_to_string(path).map_err(Error::io(FileAction::Read, path))?;
    std::fs::remove_file(path).map_err(Error::io(FileAction::Remove, path))?;
    Ok(text)
}

/// Puts back as `name` in `dir` a preset [`remove`] deleted, which held
/// `text`, unless another of its name is there now.
pub fn restore(dir: &Path, name: &str, text: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(Error::io(FileAction::Create, dir))?;
    let path = dir.join(format!("{name}.toml"));
    crate::fsutil::write_output(&path, text.as_bytes(), false)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_describes_and_removes_presets() {
        let dir = tempfile::tempdir().unwrap();
        let webm = Options { crf: Some(28), ..Options::default() };
        let tgs = tgs::Options::default();
        save(dir.path(), "small", "Small ones", (&webm, &tgs), false).unwrap();
        save(dir.path(), "other", "", (&webm, &tgs), false).unwrap();
        let load = || Presets::load(Some(dir.path())).unwrap();

        // not onto another, nor to a name that can't be a file's
        assert!(rename(dir.path(), "small", "other", &load()).is_err());
        assert!(rename(dir.path(), "small", "a/b", &load()).is_err());
        rename(dir.path(), "small", "tiny", &load()).unwrap();
        assert_eq!(load().webm("tiny").unwrap().crf, Some(28));
        assert!(load().get("small").is_err());

        // described anew, its options as they were
        let path = dir.path().join("tiny.toml");
        set_description(&path, "Tiny ones").unwrap();
        assert_eq!(load().get("tiny").unwrap().description, "Tiny ones");
        set_description(&path, "").unwrap();
        assert_eq!(load().get("tiny").unwrap().description, "");
        assert_eq!(load().webm("tiny").unwrap().crf, Some(28));

        // gone, and back as it was
        let text = remove(dir.path(), "tiny", &load()).unwrap();
        assert!(load().get("tiny").is_err());
        restore(dir.path(), "tiny", &text).unwrap();
        assert_eq!(load().webm("tiny").unwrap().crf, Some(28));
        assert!(matches!(restore(dir.path(), "tiny", &text), Err(Error::OutputExists(_))));

        // not a built-in one's, which it would stand in for
        assert!(rename(dir.path(), "tiny", "fast", &load()).is_err());
        // a name changed in case alone: another file's where case matters,
        // which stays as it is, and the same file where it doesn't
        let capital = dir.path().join("Tiny.toml");
        match save(dir.path(), "Tiny", "", (&webm, &tgs), false) {
            Err(Error::OutputExists(_)) => {
                rename(dir.path(), "tiny", "Tiny", &load()).unwrap();
                assert_eq!(load().webm("Tiny").unwrap().crf, Some(28));
                rename(dir.path(), "Tiny", "tiny", &load()).unwrap();
            }
            saved => {
                saved.unwrap();
                let other = std::fs::read_to_string(&capital).unwrap();
                assert!(rename(dir.path(), "tiny", "Tiny", &load()).is_err());
                assert_eq!(std::fs::read_to_string(&capital).unwrap(), other);
                std::fs::remove_file(&capital).unwrap();
            }
        }

        // one another extends keeps its name, and stays
        write(dir.path(), "child", "extends = 'tiny'\n");
        let extended = |result: Result<()>| match result {
            Err(Error::Preset(PresetError::Extended(other))) => other == "child",
            _ => false,
        };
        assert!(extended(rename(dir.path(), "tiny", "small", &load()).map(|_| ())));
        assert!(extended(remove(dir.path(), "tiny", &load()).map(|_| ())));
        assert!(load().get("tiny").is_ok());
        remove(dir.path(), "child", &load()).unwrap();
        remove(dir.path(), "tiny", &load()).unwrap();
    }

    #[test]
    fn saves_presets_that_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let webm = Options {
            fit: Some(Fit::Crf),
            crf: Some(28),
            // one file's: left out
            crop: Some(crate::options::Crop { x: 0, y: 0, width: 8, height: 8 }),
            title: Some("pig".into()),
            ..Options::default()
        };
        let tgs = tgs::Options {
            speed: Some(Speed::Fast),
            exact: Some(false),
            pixel_scale: Some(2),
            // one file's: left out
            sheet: "4x2".parse().ok(),
            ..tgs::Options::default()
        };
        let path = save(dir.path(), "my small", "Small ones", (&webm, &tgs), false).unwrap();
        assert_eq!(path, dir.path().join("my small.toml"));
        let presets = Presets::load(Some(dir.path())).unwrap();
        assert_eq!(presets.get("my small").unwrap().description, "Small ones");
        assert_eq!(presets.webm("my small").unwrap(), shareable(&webm));
        assert_eq!(presets.tgs("my small").unwrap(), shareable(&tgs));
        // none of one file's options are kept
        let preset = presets.get("my small").unwrap();
        assert_eq!((preset.webm.crop, preset.webm.title.as_deref()), (None, None));
        assert_eq!((preset.tgs.sheet.as_ref(), preset.webm.crf), (None, Some(28)));
        assert_eq!(presets.tgs("my small").unwrap().pixel_scale, Some(2));
        // there already, replaced only when asked
        let again = save(dir.path(), "my small", "", (&webm, &tgs), false);
        assert!(matches!(again, Err(Error::OutputExists(_))));
        assert!(save(dir.path(), "my small", "", (&webm, &tgs), true).is_ok());
        for bad in ["", " lead", "a/b", "..", &"x".repeat(65)] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(format!("{name}.toml")), text).unwrap();
    }

    #[test]
    fn says_which_options_each_format_uses() {
        let keys = ["crf", "exact", "lossless", "crop", "keep-canvas", "spoof", "crf", "nope"];
        assert_eq!(Format::Webm.unused(keys), ["exact", "keep-canvas", "nope"]);
        assert_eq!(Format::Tgs.unused(keys), ["crf", "lossless", "spoof", "nope"]);
        assert_eq!(
            Format::Webp.unused(keys),
            ["crf", "exact", "lossless", "keep-canvas", "spoof", "nope"]
        );
        // every option a picture has is WebM's
        for key in crate::webp::PICTURE_OPTIONS {
            assert!(Format::Webm.uses(key), "{key}");
        }
        let all = Options {
            crf: Some(30),
            crop: Some(crate::options::Crop::whole((8, 8))),
            ..Default::default()
        };
        assert_eq!(crate::options::set_keys(&crate::webp::picture_options(&all)), ["crop"]);
    }

    #[test]
    fn names_the_image_format_webp() {
        assert_eq!(serde_json::to_value(Format::Webp).unwrap(), "webp");
        for name in ["\"webp\"", "\"image\""] {
            assert_eq!(serde_json::from_str::<Format>(name).unwrap(), Format::Webp, "{name}");
        }
        assert_eq!(Format::of_path(Path::new("a.WebP")), Some(Format::Webp));
        assert_eq!(Format::Webp.name(), "webp");
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
        let names: Vec<String> = builtin().into_iter().map(|preset| preset.name).collect();
        assert_eq!(names, BUILTIN);
    }

    #[test]
    fn loads_user_presets() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tiny",
            "description = 'tiny ones'\nextends = 'fast'\n[webm]\nfps = 10.0\n",
        );
        write(dir.path(), "balanced", "[webm]\ncrf = 20\n");
        write(dir.path(), "broken", "[webm]\nfsp = 10\n");
        write(dir.path(), "loop-a", "extends = 'loop-b'\n");
        write(dir.path(), "loop-b", "extends = 'loop-a'\n");
        write(dir.path(), "pixels", "extends = 'best'\n[tgs]\nkeep-canvas = true\n");
        write(dir.path(), "wrong", "[tgs]\ncrf = 20\n");
        write(dir.path(), "old", "format = 'tgs'\n[options]\nspeed = 'fast'\n");
        // what one file has is no preset's to say
        write(dir.path(), "emoji", "[webm]\ntarget = 'emoji'\n");
        write(dir.path(), "sheet", "[tgs]\nspeed = 'fast'\nsheet = '4x1'\n");
        write(dir.path(), "unreadable", "[webm\n");
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let presets = Presets::load(Some(dir.path())).unwrap();
        let tiny = presets.webm("tiny").unwrap();
        assert_eq!((tiny.fps, tiny.speed), (Some(10.0), Some(Speed::Fast)));
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
        // a file broken in one table is broken as a whole
        assert!(presets.tgs("broken").is_err());
        for (name, key) in [("emoji", "webm.target"), ("sheet", "tgs.sheet")] {
            let err = presets.get(name).unwrap_err();
            assert!(
                matches!(&err, Error::Preset(PresetError::PerFile { key: named, .. }) if named == key),
                "{err}"
            );
        }
        assert!(presets.webm("loop-a").unwrap_err().to_string().contains("extends itself"));
        // crf isn't a .tgs option
        assert!(presets.tgs("wrong").unwrap_err().to_string().contains("crf"));
        assert!(presets.get("old").is_err());
        // a file's own faults say it is broken, and end where the message does
        for name in ["broken", "wrong", "emoji", "sheet", "unreadable"] {
            let err = presets.get(name).unwrap_err().to_string();
            assert!(err.starts_with("broken preset ") && err.trim_end() == err, "{err:?}");
        }
        assert_eq!(presets.iter().count(), 13);
    }
}

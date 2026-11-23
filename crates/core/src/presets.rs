//! Named conversion presets: three built-in presets (`fast`, `balanced`,
//! `best`) plus user-defined `<name>.toml` files in `<config-dir>/presets/`.
//! The target (sticker or emoji) is chosen separately from the preset.
//!
//! Each preset can configure both `[webm]` (used for WebM and WebP) and `[tgs]`
//! so a single preset works across all output formats:
//!
//! ```toml
//! description = "Small and fast, for previews"
//! extends = "fast"  # optional; inherits base preset options
//!
//! [webm]            # options for WebM and static WebP
//! crf = 40
//!
//! [tgs]             # options for TGS
//! effort = "balanced"
//! ```
//!
//! Either table may be omitted. A user preset with the same name as a built-in
//! preset overrides it. Per-file options ([`PER_FILE`]) are rejected when
//! loading presets and stripped when saving them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, FileAction, Result};
use crate::options::{Effort, Fit};
use crate::telegram::Target;
use crate::tgs;
use crate::webm::Options;

/// The built-in presets' names, fastest first.
pub const BUILTIN: [&str; 3] = ["fast", "balanced", "best"];

/// The default preset for WebM and WebP output.
pub const DEFAULT_PRESET: &str = "balanced";
/// The default preset for `.tgs` output (`best` takes seconds, not minutes).
pub const DEFAULT_TGS_PRESET: &str = "best";

/// Output format for conversions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Video stickers and custom emoji (VP9 in WebM), from any video or image.
    #[default]
    Webm,
    /// Animated vector stickers and custom emoji (TGS), from pixel art.
    Tgs,
    /// Static stickers and custom emoji (WebP), from an image or a single video
    /// frame. Accepts the WebM options that apply to still images. Also parsed
    /// as `static` or `image`.
    #[serde(alias = "static", alias = "image")]
    Webp,
}

impl Format {
    /// Canonical format name used by `--format`, JSON output and error
    /// messages.
    pub fn name(self) -> &'static str {
        match self {
            Format::Webm => "webm",
            Format::Tgs => "tgs",
            Format::Webp => "webp",
        }
    }

    /// Output file extension without a leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Webm => "webm",
            Format::Tgs => "tgs",
            Format::Webp => "webp",
        }
    }

    /// Maximum file size Telegram accepts for this format and `target`, in
    /// bytes.
    pub fn max_bytes(self, target: Target) -> u64 {
        match self {
            Format::Webm => target.max_bytes(),
            Format::Tgs => crate::tgs::MAX_BYTES,
            Format::Webp => crate::telegram::MAX_STILL_BYTES,
        }
    }

    /// Returns true if pixel dimensions must be even. VP9 uses 4:2:0 chroma
    /// subsampling (2x2 blocks), so WebM requires even sides; WebP supports
    /// any dimension and `.tgs` is vector-based.
    pub fn even_sides(self) -> bool {
        matches!(self, Format::Webm)
    }

    /// Infers the format from `path`'s extension, if recognised.
    pub fn of_path(path: &Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "webm" => Some(Format::Webm),
            "tgs" => Some(Format::Tgs),
            "webp" => Some(Format::Webp),
            _ => None,
        }
    }

    /// Returns true if this format supports option `key` (or its legacy alias):
    /// all of [`Options`] for WebM, all of [`tgs::Options`] for `.tgs`, and
    /// [`crate::webp::PICTURE_OPTIONS`] for WebP.
    pub fn uses(self, key: &str) -> bool {
        let key = crate::options::current_key(key);
        match self {
            Format::Webm => Options::keys().any(|known| known == key),
            Format::Tgs => tgs::Options::keys().any(|known| known == key),
            Format::Webp => crate::webp::PICTURE_OPTIONS.contains(&key),
        }
    }

    /// Returns the deduplicated keys from `keys` that this format does not use,
    /// preserving order. Front-ends refuse these rather than drop them
    /// silently.
    pub fn unused<'k>(self, keys: impl IntoIterator<Item = &'k str>) -> Vec<&'k str> {
        let mut unused: Vec<&str> = Vec::new();
        for key in keys {
            if !self.uses(key) && !unused.contains(&key) {
                unused.push(key);
            }
        }
        unused
    }

    /// Built-in default preset name for this format ([`DEFAULT_PRESET`] or
    /// [`DEFAULT_TGS_PRESET`]).
    pub fn default_preset(self) -> &'static str {
        match self {
            Format::Webm | Format::Webp => DEFAULT_PRESET,
            Format::Tgs => DEFAULT_TGS_PRESET,
        }
    }
}

/// Format-specific options bundled with their target format.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FormatOptions {
    /// Options for WebM output.
    Webm(Options),
    /// Options for `.tgs` output.
    Tgs(tgs::Options),
    /// Options for static WebP output.
    Webp(Options),
}

impl FormatOptions {
    /// Returns the [`Format`] these options target.
    pub fn format(&self) -> Format {
        match self {
            FormatOptions::Webm(_) => Format::Webm,
            FormatOptions::Tgs(_) => Format::Tgs,
            FormatOptions::Webp(_) => Format::Webp,
        }
    }

    /// Returns the configured [`Target`], defaulting to [`Target::Sticker`].
    pub fn target(&self) -> Target {
        match self {
            FormatOptions::Webm(options) | FormatOptions::Webp(options) => options.target,
            FormatOptions::Tgs(options) => options.target,
        }
        .unwrap_or_default()
    }
}

/// Per-file options that are disallowed in presets (rejected on load and
/// stripped on save). `pixel-scale` is excluded because files from the same
/// source often share a pixel scale.
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

/// Named preset definition (built-in or loaded from a `.toml` file).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preset {
    /// Preset name (filename without `.toml` for user presets).
    pub name: String,
    /// Optional human-readable description.
    pub description: String,
    /// Base preset name inherited before applying this preset's own options.
    pub extends: Option<String>,
    /// WebM and WebP options set directly by this preset (before inheritance).
    pub webm: Options,
    /// `.tgs` options set directly by this preset (before inheritance).
    pub tgs: tgs::Options,
    /// Source file path, or `None` for built-in presets.
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
        for (table, format, options) in
            [("webm", Format::Webm, &file.webm), ("tgs", Format::Tgs, &file.tgs)]
        {
            if let Some(key) = options.keys().find(|key| PER_FILE.contains(&key.as_str())) {
                return Err(PresetError::PerFile { place, key: format!("{table}.{key}") });
            }
            // said here, as serde would list the options of one file among
            // those it takes, which a preset doesn't
            if let Some(key) = options.keys().find(|key| !format.uses(key)) {
                let known = preset_keys(format).join(", ");
                return Err(broken(&format_args!(
                    "[{table}] {key} is not a valid preset option; valid options: {known}"
                )));
            }
        }
        // serde says which value is wrong on a line of its own
        let value = |table: &str, err: toml::de::Error| {
            let text = err.to_string();
            let text = text.trim_end();
            match text.rsplit_once("\nin `") {
                Some((why, key)) => format!("[{table}] {}: {why}", key.trim_end_matches('`')),
                None => format!("[{table}]: {text}"),
            }
        };
        let webm = file.webm.try_into().map_err(|err| broken(&value("webm", err)))?;
        let tgs = file.tgs.try_into().map_err(|err| broken(&value("tgs", err)))?;
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

/// The options a preset's table for `format` can set, in the order of
/// their fields: all that `format` takes but [`PER_FILE`].
fn preset_keys(format: Format) -> Vec<String> {
    let keys: Vec<String> = match format {
        Format::Tgs => tgs::Options::keys().collect(),
        Format::Webm | Format::Webp => Options::keys().collect(),
    };
    keys.into_iter().filter(|key| !PER_FILE.contains(&key.as_str())).collect()
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
    let effort = |effort| tgs::Options { effort: Some(effort), ..Default::default() };
    vec![
        preset(
            "fast",
            "WebM fits bitrate only with the fast encoder: seconds, but blurrier. TGS takes under \
             a second, a few percent larger.",
            Options { fit: Some(Fit::Bitrate), effort: Some(Effort::Fast), ..Default::default() },
            effort(Effort::Fast),
        ),
        preset(
            "balanced",
            "WebM fits bitrate and tries lower frame rates when they look better. TGS sits \
             between fast and best.",
            Options { fit: Some(Fit::Auto), effort: Some(Effort::Balanced), ..Default::default() },
            effort(Effort::Balanced),
        ),
        preset(
            "best",
            "WebM as in balanced, with the slowest encoder: minutes for long videos. TGS takes a \
             few seconds for the smallest file.",
            Options { fit: Some(Fit::Auto), effort: Some(Effort::Best), ..Default::default() },
            effort(Effort::Best),
        ),
    ]
}

/// Why a preset can't be had, or saved, renamed or described as asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PresetError {
    /// No preset has the name; `known` are those that do.
    #[error("preset {name:?} does not exist; available: {}", known.join(", "))]
    Unknown {
        /// The name asked for.
        name: String,
        /// The presets there are.
        known: Vec<String>,
    },
    /// Its file, or that of one it extends, isn't a preset's: the message
    /// names it and says why.
    #[error("invalid preset {0}")]
    Broken(String),
    /// Presets that extend each other, in a loop: each extends the next,
    /// and the last is the first again.
    #[error("{}", extends_loop(.0))]
    ExtendsInLoop(Vec<String>),
    /// A name too short, too long or starting with a space.
    #[error("preset name must be 1 to {LONGEST_NAME} characters and cannot start with a space")]
    NameLength,
    /// A name with what a file's name shouldn't have.
    #[error("preset name may only contain letters, digits, spaces, - and _")]
    NameCharacters,
    /// Another preset, built in or not, has the name.
    #[error("preset {0:?} already exists")]
    NameTaken(String),
    /// Renaming or deleting it would break the preset named, which
    /// extends it.
    #[error("preset {0:?} extends this preset; change {0:?} first")]
    Extended(String),
    /// A preset's file sets an option of one file, one of [`PER_FILE`],
    /// as `key` names it in its table, `webm.crop`; `place` is the file.
    #[error("invalid preset {place}: {key} is a per-file option and cannot be set in a preset")]
    PerFile {
        /// The preset's file.
        place: String,
        /// The option, as its table names it.
        key: String,
    },
}

/// What [`PresetError::ExtendsInLoop`] says: "a extends itself", or
/// "circular inheritance: a → b → a".
fn extends_loop(names: &[String]) -> String {
    match names {
        [one, _] => format!("{one} extends itself"),
        names => format!("circular inheritance: {}", names.join(" → ")),
    }
}

/// All presets, by name. Broken preset files are kept as errors so that
/// one bad file does not break the others.
#[derive(Debug, Clone)]
pub struct Presets {
    entries: BTreeMap<String, std::result::Result<Preset, PresetError>>,
    /// Each preset's options with what it extends applied, worked out
    /// once as presets load: the GUI asks for them many times a frame,
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

/// One preset as `preset list --json` prints it ([`Presets::listed`]).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Listed {
    /// Preset name.
    pub name: String,
    /// Preset description (empty if omitted or broken).
    pub description: String,
    /// True for built-in presets; false for user preset files.
    pub builtin: bool,
    /// Resolved WebM/WebP options (with `extends` applied), or `None` if
    /// broken.
    pub webm: Option<Options>,
    /// Resolved `.tgs` options (with `extends` applied), or `None` if broken.
    pub tgs: Option<tgs::Options>,
    /// Error message if the preset is invalid for either format.
    pub error: Option<String>,
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

    /// Every preset as `preset list --json` prints it, broken ones too.
    pub fn listed(&self) -> Vec<Listed> {
        self.iter()
            .map(|(name, preset)| {
                let (webm, tgs) = (self.webm(name), self.tgs(name));
                let error = match (&webm, &tgs) {
                    (Err(err), _) | (_, Err(err)) => Some(err.to_string()),
                    _ => None,
                };
                Listed {
                    name: name.to_string(),
                    description: preset.map(|p| p.description.clone()).unwrap_or_default(),
                    builtin: preset.is_ok_and(|p| p.path.is_none()),
                    webm: webm.ok(),
                    tgs: tgs.ok(),
                    error,
                }
            })
            .collect()
    }

    /// The preset `name`, as loaded: [`PresetError::Unknown`] when there
    /// is none, and why it is broken when its file is.
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
            if let Some(start) = chain.iter().position(|p| &p.name == base) {
                let mut names: Vec<String> =
                    chain[start..].iter().map(|p| p.name.clone()).collect();
                names.push(base.clone());
                return Err(PresetError::ExtendsInLoop(names).into());
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

/// Validates `name` for saving a preset: 1 to [`LONGEST_NAME`] characters,
/// using letters, digits, spaces, `-` or `_`, and not starting with a space.
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

/// Returns a copy of `options` with all [`PER_FILE`] keys removed.
pub fn shareable<T: Serialize + serde::de::DeserializeOwned>(options: &T) -> T {
    let mut table = table(options);
    table.retain(|key, _| !PER_FILE.contains(&key));
    table.try_into().expect("options read back with fewer set")
}

/// Saves a preset named `name` into `dir` with `description` and the shareable
/// options from `webm` and `tgs` (excluding [`PER_FILE`]). Replaces an existing
/// file only when `replace` is true.
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
    let path = file_in(dir, name)?;
    std::fs::create_dir_all(dir).map_err(Error::io(FileAction::Create, dir))?;
    crate::fsutil::write_output(&path, text.as_bytes(), replace)?;
    Ok(path)
}

/// Validates renaming preset `from` to `to`: `to` must be valid and unused,
/// and no other preset may extend `from`.
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

/// Validates deleting preset `name` (fails if another preset extends it).
pub fn check_remove(name: &str, presets: &Presets) -> Result<()> {
    match presets.extending(name) {
        Some(other) => Err(PresetError::Extended(other.name.clone()).into()),
        None => Ok(()),
    }
}

/// Renames preset `from` to `to` in `dir` after validating via
/// [`check_rename`].
pub fn rename(dir: &Path, from: &str, to: &str, presets: &Presets) -> Result<PathBuf> {
    check_rename(from, to, presets)?;
    let (old, new) = (file_in(dir, from)?, file_in(dir, to)?);
    // Allow case-only renames on case-insensitive filesystems; refuse
    // clobbering a distinct file on case-sensitive filesystems.
    if new.exists() && !same_file(&old, &new) {
        return Err(Error::OutputExists(new));
    }
    std::fs::rename(&old, &new).map_err(Error::io(FileAction::Rename, &old))?;
    Ok(new)
}

/// Resolves `<name>.toml` inside `dir`, rejecting path traversal. Less strict
/// than [`check_name`] so hand-created preset files can still be managed.
fn file_in(dir: &Path, name: &str) -> Result<PathBuf> {
    let file = format!("{name}.toml");
    let mut parts = Path::new(&file).components();
    let plain = matches!(parts.next(), Some(std::path::Component::Normal(part)) if part == file.as_str())
        && parts.next().is_none()
        && !name.contains(['/', '\\', '\0'])
        && !name.is_empty();
    if !plain {
        return Err(PresetError::NameCharacters.into());
    }
    Ok(dir.join(file))
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

/// Updates the `description` field of the preset file at `path`, keeping all
/// options. Rewrites the file without preserving TOML comments.
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

/// Deletes preset `name` in `dir` (after [`check_remove`]) and returns its
/// previous file contents so [`restore`] can undo the deletion.
pub fn remove(dir: &Path, name: &str, presets: &Presets) -> Result<String> {
    check_remove(name, presets)?;
    let path = &file_in(dir, name)?;
    let text = std::fs::read_to_string(path).map_err(Error::io(FileAction::Read, path))?;
    std::fs::remove_file(path).map_err(Error::io(FileAction::Remove, path))?;
    Ok(text)
}

/// Restores a previously removed preset file to `dir` unless `<name>.toml`
/// already exists in `dir`.
pub fn restore(dir: &Path, name: &str, text: &str) -> Result<PathBuf> {
    let path = file_in(dir, name)?;
    std::fs::create_dir_all(dir).map_err(Error::io(FileAction::Create, dir))?;
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
            effort: Some(Effort::Fast),
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

    #[test]
    fn keeps_to_the_presets_folder() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("presets");
        std::fs::create_dir(&dir).unwrap();
        let outside = root.path().join("x.toml");
        std::fs::write(&outside, "description = 'not a preset of the folder'\n").unwrap();
        let presets = Presets::load(Some(&dir)).unwrap();
        for name in ["../x", "..\\x", "/x", "a/../../x"] {
            let refused = |result: Result<()>| {
                matches!(result, Err(Error::Preset(PresetError::NameCharacters)))
            };
            assert!(refused(remove(&dir, name, &presets).map(drop)), "{name}");
            assert!(refused(restore(&dir, name, "").map(drop)), "{name}");
            assert!(refused(rename(&dir, name, "y", &presets).map(drop)), "{name}");
        }
        assert!(outside.exists());
        assert!(!root.path().join("y.toml").exists());
        // one saved by hand under a name the GUI wouldn't give is still
        // the user's to rename and delete
        write(&dir, "by.hand", "description = ''\n");
        let presets = Presets::load(Some(&dir)).unwrap();
        let text = remove(&dir, "by.hand", &presets).unwrap();
        restore(&dir, "by.hand", &text).unwrap();
        rename(&dir, "by.hand", "by hand", &presets).unwrap();
        assert!(dir.join("by hand.toml").exists());
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
        // the keys named are the ones serde reads, as it refuses others
        for key in Options::keys() {
            let read = serde_json::from_value::<Options>(serde_json::json!({ &key: null }));
            assert!(read.is_ok(), "{key}: {read:?}");
        }
        for key in tgs::Options::keys() {
            let read = serde_json::from_value::<tgs::Options>(serde_json::json!({ &key: null }));
            assert!(read.is_ok(), "{key}: {read:?}");
        }
        let all = Options {
            crf: Some(30),
            crop: Some(crate::options::Crop::whole((8, 8))),
            ..Default::default()
        };
        assert_eq!(crate::options::set_keys(&crate::webp::picture_options(&all)), ["crop"]);
    }

    #[test]
    fn names_the_static_format_webp() {
        assert_eq!(serde_json::to_value(Format::Webp).unwrap(), "webp");
        for name in ["\"webp\"", "\"static\"", "\"image\""] {
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
            (fast.fit, fast.effort, fast.target),
            (Some(Fit::Bitrate), Some(Effort::Fast), None)
        );
        assert_eq!(presets.tgs("fast").unwrap().effort, Some(Effort::Fast));
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
        write(dir.path(), "sheet", "[tgs]\neffort = 'fast'\nsheet = '4x1'\n");
        // as it was once named
        let renamed = "[webm]\nspeed = 'best'\nscaling = 'pixel-perfect'\nexact-scale = '3'\n\
                       [tgs]\nspeed = 'fast'\n";
        write(dir.path(), "renamed", renamed);
        write(dir.path(), "unreadable", "[webm\n");
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let presets = Presets::load(Some(dir.path())).unwrap();
        let tiny = presets.webm("tiny").unwrap();
        assert_eq!((tiny.fps, tiny.effort), (Some(10.0), Some(Effort::Fast)));
        // only WebM options were set, the rest comes from the base
        assert_eq!(presets.tgs("tiny").unwrap().effort, Some(Effort::Fast));
        // the user file replaced the built-in preset
        assert_eq!(
            presets.webm("balanced").unwrap(),
            Options { crf: Some(20), ..Default::default() }
        );
        let pixels = presets.tgs("pixels").unwrap();
        assert_eq!((pixels.keep_canvas, pixels.effort), (Some(true), Some(Effort::Best)));
        let renamed = presets.webm("renamed").unwrap();
        assert_eq!(
            (renamed.effort, renamed.scaling),
            (Some(Effort::Best), Some(crate::options::Scaling::Integer))
        );
        assert_eq!(renamed.integer_scale, "3".parse().ok());
        assert_eq!(presets.tgs("renamed").unwrap().effort, Some(Effort::Fast));

        let broken = presets.webm("broken").unwrap_err().to_string();
        assert!(broken.contains("broken.toml") && broken.contains("fsp"), "{broken}");
        // naming only what a preset may set, not what one file chooses
        assert!(broken.contains("fsp is not a valid preset option; valid options: "), "{broken}");
        // of the options, not of where the file is, which may be anywhere
        let options = broken.split("valid options: ").nth(1).unwrap_or_default();
        assert!(options.contains(" fps,") && !options.contains("target"), "{broken}");
        // a file broken in one table is broken as a whole
        assert!(presets.tgs("broken").is_err());
        for (name, key) in [("emoji", "webm.target"), ("sheet", "tgs.sheet")] {
            let err = presets.get(name).unwrap_err();
            assert!(
                matches!(&err, Error::Preset(PresetError::PerFile { key: named, .. }) if named == key),
                "{err}"
            );
        }
        let looped = presets.webm("loop-a").unwrap_err().to_string();
        assert!(looped.starts_with("circular inheritance: ") && looped.contains(" → "), "{looped}");
        // crf isn't a .tgs option
        assert!(presets.tgs("wrong").unwrap_err().to_string().contains("crf"));
        assert!(presets.get("old").is_err());
        // a file's own faults say it is broken, and end where the message does
        for name in ["broken", "wrong", "emoji", "sheet", "unreadable"] {
            let err = presets.get(name).unwrap_err().to_string();
            assert!(err.starts_with("invalid preset ") && err.trim_end() == err, "{err:?}");
        }
        assert_eq!(presets.iter().count(), 14);
    }
}

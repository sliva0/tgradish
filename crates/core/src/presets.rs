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
    /// Image stickers and emoji: one frame of a video, or an image, as
    /// WebP. They take WebM's options where a still picture has them.
    Image,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Webm => "webm",
            Format::Tgs => "tgs",
            Format::Image => "webp",
        }
    }

    /// The format a file name asks for, by its extension.
    pub fn of_path(path: &Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "webm" => Some(Format::Webm),
            "tgs" => Some(Format::Tgs),
            "webp" => Some(Format::Image),
            _ => None,
        }
    }

    pub fn default_preset(self) -> &'static str {
        match self {
            Format::Webm | Format::Image => DEFAULT_PRESET,
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
    /// WebM's options, for an image.
    Image(Options),
}

impl FormatOptions {
    pub fn format(&self) -> Format {
        match self {
            FormatOptions::Webm(_) => Format::Webm,
            FormatOptions::Tgs(_) => Format::Tgs,
            FormatOptions::Image(_) => Format::Image,
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
    toml::Table::try_from(options).expect("options serialise to a table")
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
    /// Each preset's options with what it extends applied, worked out
    /// once as presets load: the window asks for them many times a frame,
    /// and each would otherwise parse the TOML of the whole chain again.
    resolved: BTreeMap<String, Resolved>,
}

/// A preset's options for either kind of result, or why they can't be
/// had.
#[derive(Debug, Clone)]
struct Resolved {
    webm: std::result::Result<Options, String>,
    tgs: std::result::Result<TgsOptions, String>,
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
            Error::InvalidOptions(message) => message,
            other => other.to_string(),
        };
        self.resolved = self
            .entries
            .keys()
            .map(|name| {
                let webm =
                    self.fold(name, |preset| &preset.webm, Options::default(), Options::merged);
                let tgs = self.fold(
                    name,
                    |preset| &preset.tgs,
                    TgsOptions::default(),
                    TgsOptions::merged,
                );
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
        Ok(presets.resolved())
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
            Format::Image => FormatOptions::Image(self.webm(name)?),
        })
    }

    /// Options of `name` for WebM results.
    pub fn webm(&self, name: &str) -> Result<Options> {
        match self.resolved.get(name) {
            Some(resolved) => resolved.webm.clone().map_err(Error::InvalidOptions),
            None => Err(self.get(name).expect_err("every preset is resolved")),
        }
    }

    /// Options of `name` for `.tgs` results.
    pub fn tgs(&self, name: &str) -> Result<TgsOptions> {
        match self.resolved.get(name) {
            Some(resolved) => resolved.tgs.clone().map_err(Error::InvalidOptions),
            None => Err(self.get(name).expect_err("every preset is resolved")),
        }
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

/// Longest name a preset saved here can have.
pub const LONGEST_NAME: usize = 64;

/// Whether `name` can name a preset file: letters, digits, spaces, `-`
/// and `_`, not starting with a space, at most 64 of them.
pub fn check_name(name: &str) -> Result<()> {
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_');
    if name.trim().is_empty() || name.starts_with(' ') || name.chars().count() > LONGEST_NAME {
        return Err(Error::InvalidOptions(format!(
            "a preset's name has 1 to {LONGEST_NAME} characters, not starting with a space"
        )));
    }
    if !name.chars().all(allowed) {
        return Err(Error::InvalidOptions(
            "a preset's name has only letters, digits, spaces, - and _".into(),
        ));
    }
    Ok(())
}

/// The options of `options` a preset keeps: none that only make sense for
/// one file, as what to make, its crop, its time, its tag and layers, its
/// title.
pub(crate) fn shareable(options: &Options) -> Options {
    Options {
        target: None,
        tag: None,
        show_layers: None,
        hide_layers: None,
        crop: None,
        start: None,
        length: None,
        title: None,
        ..options.clone()
    }
}

/// [`shareable`] for `.tgs` options, which also leave out a sprite sheet's
/// layout; the size of the art's pixels stays, as files from one place
/// often share it.
pub(crate) fn shareable_tgs(options: &TgsOptions) -> TgsOptions {
    TgsOptions {
        target: None,
        tag: None,
        show_layers: None,
        hide_layers: None,
        crop: None,
        start: None,
        length: None,
        title: None,
        sheet: None,
        sheet_frames: None,
        ..options.clone()
    }
}

/// Writes a preset named `name` into `dir`, said to be `description`, of
/// what [`shareable`] keeps of `webm` and `tgs`, and returns its file.
/// One there already is replaced only with `replace`.
pub fn save(
    dir: &Path,
    name: &str,
    description: &str,
    (webm, tgs): (&Options, &TgsOptions),
    replace: bool,
) -> Result<PathBuf> {
    check_name(name)?;
    let mut file = toml::Table::new();
    if !description.is_empty() {
        file.insert("description".into(), description.into());
    }
    file.insert("webm".into(), table(&shareable(webm)).into());
    file.insert("tgs".into(), table(&shareable_tgs(tgs)).into());
    let text = toml::to_string(&file).map_err(|err| Error::InvalidOptions(err.to_string()))?;
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.toml"));
    crate::fsutil::write_output(&path, text.as_bytes(), replace)?;
    Ok(path)
}

/// Renames the preset `from` in `dir` to `to`, a name no other preset
/// has, built in or not. One another preset extends keeps its name, as
/// that one would break.
pub fn rename(dir: &Path, from: &str, to: &str, presets: &Presets) -> Result<PathBuf> {
    check_name(to)?;
    if to != from && presets.iter().any(|(name, _)| name == to) {
        return Err(Error::InvalidOptions(format!("another preset is named {to:?}")));
    }
    let extending = presets
        .iter()
        .filter_map(|(_, preset)| preset.ok())
        .find(|preset| preset.extends.as_deref() == Some(from));
    if let Some(other) = extending {
        return Err(Error::InvalidOptions(format!(
            "{:?} extends it: rename it there first",
            other.name
        )));
    }
    let (old, new) = (dir.join(format!("{from}.toml")), dir.join(format!("{to}.toml")));
    // a name changed only in case names the same file where case doesn't
    // matter; where it does, the other file is another preset's
    if new.exists() && !same_file(&old, &new) {
        return Err(Error::OutputExists(new));
    }
    std::fs::rename(&old, &new)?;
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
pub fn describe(path: &Path, description: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut file: toml::Table = toml::from_str(&text)
        .map_err(|err| Error::InvalidOptions(format!("{}: {err}", path.display())))?;
    file.remove("description");
    if !description.is_empty() {
        file.insert("description".into(), description.into());
    }
    let text = toml::to_string(&file).map_err(|err| Error::InvalidOptions(err.to_string()))?;
    crate::fsutil::write_output(path, text.as_bytes(), true)
}

/// Deletes the preset at `path`, and returns what it held, which
/// [`restore`] puts back.
pub fn remove(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)?;
    std::fs::remove_file(path)?;
    Ok(text)
}

/// Puts back as `name` in `dir` a preset [`remove`] deleted, which held
/// `text`, unless another of its name is there now.
pub fn restore(dir: &Path, name: &str, text: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.toml"));
    crate::fsutil::write_output(&path, text.as_bytes(), false)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::Target;

    #[test]
    fn renames_describes_and_removes_presets() {
        let dir = tempfile::tempdir().unwrap();
        let webm = Options { crf: Some(28), ..Options::default() };
        let tgs = TgsOptions::default();
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
        describe(&path, "Tiny ones").unwrap();
        assert_eq!(load().get("tiny").unwrap().description, "Tiny ones");
        describe(&path, "").unwrap();
        assert_eq!(load().get("tiny").unwrap().description, "");
        assert_eq!(load().webm("tiny").unwrap().crf, Some(28));

        // gone, and back as it was
        let text = remove(&path).unwrap();
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
                remove(&capital).unwrap();
            }
        }

        // one another extends keeps its name
        write(dir.path(), "child", "extends = 'tiny'\n");
        assert!(rename(dir.path(), "tiny", "small", &load()).is_err());
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
        let tgs = TgsOptions {
            speed: Some(Speed::Fast),
            lossless: Some(false),
            pixel_scale: Some(2),
            // one file's: left out
            sheet: Some("4x2".into()),
            ..TgsOptions::default()
        };
        let path = save(dir.path(), "my small", "Small ones", (&webm, &tgs), false).unwrap();
        assert_eq!(path, dir.path().join("my small.toml"));
        let presets = Presets::load(Some(dir.path())).unwrap();
        assert_eq!(presets.get("my small").unwrap().description, "Small ones");
        assert_eq!(presets.webm("my small").unwrap(), shareable(&webm));
        assert_eq!(presets.tgs("my small").unwrap(), shareable_tgs(&tgs));
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

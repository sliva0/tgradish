//! The window's settings: core's `config.toml`, with the `[gui]` table core
//! keeps for the window read here. They are read leniently, unlike the
//! rest of the file: a key a newer window wrote, or one renamed since, is
//! left as it is, and a value that can't be read gives that setting its
//! default rather than failing the file, which the command line reads too.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tgradish_core::telegram::Target;

/// Core's config, and the window's own settings from its `[gui]` table.
/// It stands for core's where that is asked for, as presets are chosen by
/// it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Config {
    /// Core's, its `[gui]` holding only what the window doesn't know of.
    core: tgradish_core::config::Config,
    pub(crate) gui: GuiConfig,
}

impl Deref for Config {
    type Target = tgradish_core::config::Config;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.core
    }
}

impl From<tgradish_core::config::Config> for Config {
    fn from(mut core: tgradish_core::config::Config) -> Config {
        let gui = GuiConfig::read(&core.gui);
        core.gui.retain(|key, _| !KEYS.contains(&key));
        Config { core, gui }
    }
}

impl Config {
    /// Default location of `config.toml`.
    pub(crate) fn default_path() -> Option<PathBuf> {
        tgradish_core::config::Config::default_path()
    }

    /// Reads `path`, as core does. A missing file gives the defaults.
    pub(crate) fn load(path: &Path) -> tgradish_core::Result<Config> {
        tgradish_core::config::Config::load(path).map(Config::from)
    }

    /// Writes the settings to `path`, with what the window doesn't know of
    /// in `[gui]` kept as it was read.
    pub(crate) fn save(&self, path: &Path) -> tgradish_core::Result<()> {
        let mut core = self.core.clone();
        self.gui.write(&mut core.gui);
        core.save(path)
    }
}

/// Settings of the tgradish window.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct GuiConfig {
    /// Directory for results, instead of next to each input. Never empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output_dir: Option<PathBuf>,
    /// Replace existing results.
    pub(crate) overwrite: bool,
    /// What new files become: stickers or emoji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<Target>,
    /// Ease scrolling over a few frames instead of following the wheel.
    pub(crate) smooth_scrolling: bool,
    /// The window's language, as a tag like `de`; the system's when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    /// Light or dark, or as the system has it.
    pub(crate) theme: Theme,
    /// Start with the files open when the window last closed.
    pub(crate) reopen_files: bool,
    /// Show the timeline's cuts and times in frames rather than seconds.
    pub(crate) timeline_frames: bool,
    pub(crate) result_preview: ResultPreview,
}

impl Default for GuiConfig {
    fn default() -> GuiConfig {
        GuiConfig {
            output_dir: None,
            overwrite: false,
            target: None,
            smooth_scrolling: false,
            language: None,
            theme: Theme::default(),
            reopen_files: true,
            timeline_frames: false,
            result_preview: ResultPreview::default(),
        }
    }
}

/// The keys of `[gui]` the window reads and writes; others are kept.
const KEYS: [&str; 9] = [
    "output-dir",
    "overwrite",
    "target",
    "smooth-scrolling",
    "language",
    "theme",
    "reopen-files",
    "timeline-frames",
    "result-preview",
];

/// The value at `key` in `table`, if it is there and reads as a `T`.
fn get<T: DeserializeOwned>(table: &toml::Table, key: &str) -> Option<T> {
    table.get(key)?.clone().try_into().ok()
}

impl GuiConfig {
    /// The settings in `table`, each the default where it is missing or
    /// can't be read.
    pub(crate) fn read(table: &toml::Table) -> GuiConfig {
        let default = GuiConfig::default();
        GuiConfig {
            output_dir: get::<PathBuf>(table, "output-dir")
                .filter(|dir| !dir.as_os_str().is_empty()),
            overwrite: get(table, "overwrite").unwrap_or(default.overwrite),
            target: get(table, "target"),
            smooth_scrolling: get(table, "smooth-scrolling").unwrap_or(default.smooth_scrolling),
            language: get(table, "language"),
            theme: get(table, "theme").unwrap_or(default.theme),
            reopen_files: get(table, "reopen-files").unwrap_or(default.reopen_files),
            timeline_frames: get(table, "timeline-frames").unwrap_or(default.timeline_frames),
            result_preview: match table.get("result-preview") {
                Some(toml::Value::Table(preview)) => ResultPreview::read(preview),
                _ => default.result_preview,
            },
        }
    }

    /// Writes the settings into `table`, keeping what else it holds.
    fn write(&self, table: &mut toml::Table) {
        for key in KEYS {
            table.remove(key);
        }
        let written = toml::Table::try_from(self).expect("the window's settings are a table");
        table.extend(written);
    }
}

/// The result's small picture beside the input in the window: how large
/// it is, and in which corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ResultPreview {
    /// From 1, the smallest, to [`ResultPreview::LARGEST`].
    pub(crate) size: u8,
    pub(crate) corner: Corner,
}

impl ResultPreview {
    pub(crate) const LARGEST: u8 = 5;

    fn read(table: &toml::Table) -> ResultPreview {
        let default = ResultPreview::default();
        ResultPreview {
            size: get(table, "size")
                .filter(|size| (1..=ResultPreview::LARGEST).contains(size))
                .unwrap_or(default.size),
            corner: get(table, "corner").unwrap_or(default.corner),
        }
    }
}

impl Default for ResultPreview {
    fn default() -> ResultPreview {
        ResultPreview { size: 3, corner: Corner::default() }
    }
}

/// A corner of the preview.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

impl Corner {
    pub(crate) fn left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }

    pub(crate) fn top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }

    /// The corner on the `left` or right, at the `top` or bottom.
    pub(crate) fn at(left: bool, top: bool) -> Corner {
        match (left, top) {
            (true, true) => Corner::TopLeft,
            (false, true) => Corner::TopRight,
            (true, false) => Corner::BottomLeft,
            (false, false) => Corner::BottomRight,
        }
    }
}

/// The window's colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Theme {
    /// As the system has it.
    #[default]
    System,
    Light,
    Dark,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> GuiConfig {
        let core: tgradish_core::config::Config = toml::from_str(text).unwrap();
        Config::from(core).gui
    }

    #[test]
    fn reads_what_it_can_and_defaults_the_rest() {
        let gui = read(
            "[gui]\noverwrite = true\ntheme = 'sepia'\nnewer-setting = 1\nreopen-files = false\n\
             [gui.result-preview]\nsize = 9\ncorner = 'top-left'\n",
        );
        assert!(gui.overwrite && !gui.reopen_files);
        // a theme it doesn't know, and a size out of range: the defaults
        assert_eq!(gui.theme, Theme::System);
        assert_eq!(gui.result_preview, ResultPreview { size: 3, corner: Corner::TopLeft });
        // a table where a setting is expected is no setting
        assert_eq!(read("[gui]\nresult-preview = 4\n").result_preview, ResultPreview::default());
    }

    #[test]
    fn never_reads_an_empty_folder() {
        assert_eq!(read("[gui]\noutput-dir = ''\n").output_dir, None);
        assert_eq!(read("[gui]\noutput-dir = 'out'\n").output_dir, Some(PathBuf::from("out")));
    }

    #[test]
    fn keeps_what_it_doesnt_know_on_saving() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[gui]\nnewer-setting = 'kept'\noutput-dir = 'old'\n").unwrap();
        let mut config = Config::load(&path).unwrap();
        config.gui.output_dir = None;
        config.gui.result_preview.size = 5;
        config.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("newer-setting = \"kept\"") && !text.contains("old"), "{text}");
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn writes_only_its_own_keys() {
        let gui = GuiConfig {
            output_dir: Some("out".into()),
            target: Some(Target::Emoji),
            language: Some("de".into()),
            ..GuiConfig::default()
        };
        let written = toml::Table::try_from(&gui).unwrap();
        let mut keys: Vec<&str> = written.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut known = KEYS.to_vec();
        known.sort_unstable();
        assert_eq!(keys, known);
    }
}

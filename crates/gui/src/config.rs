//! The window's settings: core's `config.toml`, with the `[gui]` table core
//! keeps for the window read here. They are read leniently, unlike the
//! rest of the file: a key a newer window wrote, or one renamed since, is
//! left as it is, and a value that can't be read gives that setting its
//! default rather than failing the file, which the command line reads too,
//! and is left as it is until that setting is changed.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use eframe::egui::Color32;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tgradish_core::telegram::Target;

use crate::theme;

/// Core's config, and the window's own settings from its `[gui]` table.
/// It stands for core's where that is asked for, as presets are chosen by
/// it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Config {
    /// Core's, its `[gui]` holding only what the window doesn't know of:
    /// keys, and values of its own keys it can't read.
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
        let read = toml::Table::try_from(&gui).expect("the window's settings are a table");
        core.gui.retain(|key, value| !KEYS.contains(&key) || read.get(key) != Some(value));
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
    pub(crate) fn save(&mut self, path: &Path) -> tgradish_core::Result<()> {
        self.forget_overridden();
        let mut core = self.core.clone();
        self.gui.write(&mut core.gui);
        core.save(path)
    }

    /// Forgets each value of `[gui]` this version couldn't read whose
    /// setting is no longer what that value was read as, so that changing
    /// the setting back writes what was chosen rather than bringing the
    /// value back. The window does this at every frame, as Settings
    /// changes them without saving.
    pub(crate) fn forget_overridden(&mut self) {
        // most files have none, which costs nothing to tell
        if !KEYS.iter().any(|key| self.core.gui.contains_key(*key)) {
            return;
        }
        let chosen = toml::Table::try_from(&self.gui).expect("the window's settings are a table");
        let unread = GuiConfig::read(&self.core.gui);
        let unread = toml::Table::try_from(&unread).expect("the window's settings are a table");
        self.core.gui.retain(|key, _| !KEYS.contains(&key) || chosen.get(key) == unread.get(key));
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
    /// The colour that marks what is chosen, hovered and focused.
    pub(crate) accent: Accent,
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
            accent: Accent::default(),
            reopen_files: true,
            timeline_frames: false,
            result_preview: ResultPreview::default(),
        }
    }
}

/// The keys of `[gui]` the window reads and writes; others are kept.
const KEYS: [&str; 10] = [
    "output-dir",
    "overwrite",
    "target",
    "smooth-scrolling",
    "language",
    "theme",
    "accent",
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
            accent: get(table, "accent").unwrap_or(default.accent),
            reopen_files: get(table, "reopen-files").unwrap_or(default.reopen_files),
            timeline_frames: get(table, "timeline-frames").unwrap_or(default.timeline_frames),
            result_preview: match table.get("result-preview") {
                Some(toml::Value::Table(preview)) => ResultPreview::read(preview),
                _ => default.result_preview,
            },
        }
    }

    /// Writes the settings into `table`, keeping what else it holds: a
    /// value of a setting this version couldn't read stays, as one is
    /// there only while its setting is still what it read as
    /// ([`Config::forget_overridden`]).
    fn write(&self, table: &mut toml::Table) {
        let written = toml::Table::try_from(self).expect("the window's settings are a table");
        for key in KEYS {
            if !table.contains_key(key)
                && let Some(value) = written.get(key)
            {
                table.insert(key.to_owned(), value.clone());
            }
        }
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

/// The window's accent colour: the system's, one of Plasma's, or one of
/// the user's own. Written as `system`, a name of [`theme::ACCENTS`], or
/// `#rrggbb`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub(crate) enum Accent {
    /// As the system has it, or Breeze's where it has none.
    #[default]
    System,
    /// One of [`theme::ACCENTS`], by its name.
    Named(&'static str),
    Own(Color32),
}

impl Accent {
    /// The colour, unless it is the system's.
    pub(crate) fn colour(self) -> Option<Color32> {
        match self {
            Accent::System => None,
            Accent::Named(name) => {
                theme::ACCENTS.iter().find(|(each, _)| *each == name).map(|(_, colour)| *colour)
            }
            Accent::Own(colour) => Some(colour),
        }
    }

    /// `#rrggbb`, in either case, as a colour.
    pub(crate) fn hex(text: &str) -> Option<Color32> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
        Some(Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?))
    }
}

/// `colour` as `#rrggbb`.
pub(crate) fn hex(colour: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b())
}

impl TryFrom<String> for Accent {
    type Error = String;

    fn try_from(text: String) -> Result<Accent, String> {
        if text == "system" {
            return Ok(Accent::System);
        }
        if let Some((name, _)) = theme::ACCENTS.iter().find(|(name, _)| *name == text) {
            return Ok(Accent::Named(name));
        }
        Accent::hex(&text).map(Accent::Own).ok_or_else(|| format!("no accent colour: {text}"))
    }
}

impl<'de> Deserialize<'de> for Accent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Accent, D::Error> {
        Accent::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl From<Accent> for String {
    fn from(accent: Accent) -> String {
        match accent {
            Accent::System => "system".to_owned(),
            Accent::Named(name) => name.to_owned(),
            Accent::Own(colour) => hex(colour),
        }
    }
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
    fn reads_every_key_the_example_documents() {
        let example = include_str!("../../core/src/config.example.toml");
        let config =
            Config::from(toml::from_str::<tgradish_core::config::Config>(example).unwrap());
        // none left over as unknown, or as a value it couldn't read
        assert!(config.core.gui.is_empty(), "{:?}", config.core.gui);
        assert_eq!(config.gui.accent, Accent::System);
    }

    #[test]
    fn reads_accents_by_name_or_colour() {
        let accent = |value: &str| read(&format!("[gui]\naccent = {value}\n")).accent;
        assert_eq!(accent("'system'"), Accent::System);
        assert_eq!(accent("'teal'"), Accent::Named("teal"));
        assert_eq!(accent("'#3DAEe9'"), Accent::Own(theme::BREEZE_ACCENT));
        // what it can't read: the system's
        for unread in ["'mauve'", "'#3dae'", "'3daee9'", "'#3daeeg'", "7", "'#ffffffff'"] {
            assert_eq!(accent(unread), Accent::System, "{unread}");
        }
        assert_eq!(Accent::Named("grey").colour(), Some(Color32::from_rgb(104, 107, 111)));
        // written as it is read
        for accent in
            [Accent::System, Accent::Named("pink"), Accent::Own(Color32::from_rgb(1, 2, 254))]
        {
            let gui = GuiConfig { accent, ..GuiConfig::default() };
            let written = toml::Table::try_from(&gui).unwrap();
            assert_eq!(GuiConfig::read(&written).accent, accent);
        }
        let own =
            GuiConfig { accent: Accent::Own(Color32::from_rgb(1, 2, 254)), ..Default::default() };
        assert_eq!(toml::Table::try_from(&own).unwrap()["accent"].as_str(), Some("#0102fe"));
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
    fn keeps_values_it_cannot_read_until_their_setting_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let text = "[gui]\ntheme = 'high-contrast'\nlanguage = 7\n\
                    [gui.result-preview]\nsize = 9\ncorner = 'top-left'\n";
        std::fs::write(&path, text).unwrap();
        let mut app = crate::app::App::with(
            Config::load(&path).unwrap(),
            Some(path.clone()),
            tgradish_core::presets::Presets::builtin(),
        );
        assert_eq!(app.config.gui.theme, Theme::System);
        // another setting changed, and the whole of them saved
        app.save_setting(|gui| gui.timeline_frames = true);
        app.config.save(&path).unwrap();
        let saved = || std::fs::read_to_string(&path).unwrap();
        for kept in ["theme = \"high-contrast\"", "language = 7", "size = 9"] {
            assert!(saved().contains(kept), "{kept} in {}", saved());
        }
        assert!(saved().contains("timeline-frames = true"), "{}", saved());
        // the setting itself changed
        app.save_setting(|gui| gui.theme = Theme::Dark);
        app.save_setting(|gui| gui.result_preview.corner = Corner::TopRight);
        assert!(
            saved().contains("theme = \"dark\"") && saved().contains("size = 3"),
            "{}",
            saved()
        );
        assert!(saved().contains("language = 7"), "{}", saved());
        // and changed back to what those values read as: they are gone for
        // good, whether one setting or the whole of them is saved
        app.save_setting(|gui| gui.theme = Theme::System);
        app.save_setting(|gui| gui.result_preview.corner = Corner::TopLeft);
        let chosen = ["theme = \"system\"", "size = 3", "corner = \"top-left\""];
        for chosen in chosen {
            assert!(saved().contains(chosen), "{chosen} in {}", saved());
        }
        app.config.save(&path).unwrap();
        for chosen in chosen {
            assert!(saved().contains(chosen), "{chosen} in {}", saved());
        }
    }

    #[test]
    fn forgets_values_it_cannot_read_once_settings_changes_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[gui]\ntheme = 'high-contrast'\n").unwrap();
        let mut harness = crate::testing::harness(Config::load(&path).unwrap());
        // changed and changed back in Settings, then saved
        harness.state_mut().config.gui.theme = Theme::Dark;
        harness.step();
        harness.state_mut().config.gui.theme = Theme::System;
        harness.step();
        harness.state_mut().config.save(&path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("theme = \"system\""), "{saved}");
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

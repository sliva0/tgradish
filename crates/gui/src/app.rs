//! The window's state and its frame: the files open, what waits to be
//! converted, the dialogs and the theme. The other modules add their parts
//! to `App`; this holds what they share, and draws them in turn.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::config::Config;
use eframe::egui::{self, Color32};
use tgradish_core::backend::Backend;
use tgradish_core::clipboard::Pasted;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::paths;
use tgradish_core::presets::Presets;

use crate::Launch;
use crate::about::about;
use crate::canvas::Screen;
use crate::fonts::fonts;
use crate::inset::Inset;
use crate::item::{Conversion, Item};
use crate::jobs::Job;
use crate::keys::direct_scrolling;
use crate::l10n::fl;
use crate::outputs::{FileVersion, Written};
use crate::widgets::toast;
use crate::work::release;
use crate::{accent, fallback, inspect, preset_dialogs, session, settings_dialog, system, theme};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::{dnd, toplevel_icon};

/// How many items keep their big frames: the selected one and the last few.
const FRAMES_HELD: usize = 3;
/// How wide the list of files starts.
const FILES_WIDTH: f32 = 410.0;

pub(crate) struct App {
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) config: Config,
    pub(crate) presets: Presets,
    /// Where the user's presets are kept, and saved to: none in tests,
    /// unless one gives a folder.
    pub(crate) presets_dir: Option<PathBuf>,
    pub(crate) items: Vec<Item>,
    pub(crate) next_id: u64,
    pub(crate) selected: Option<u64>,
    /// Items selected lately, the latest first, which keep their frames.
    pub(crate) recent: VecDeque<u64>,
    /// Items waiting to be converted, in order.
    pub(crate) queue: VecDeque<u64>,
    /// Results this window wrote, which it replaces when making them again.
    pub(crate) written: HashMap<PathBuf, Written>,
    /// Files found in the way, by their size and time: whether tgradish
    /// made them, so they may be replaced. Working out where results go
    /// fills it in, though it only borrows the window, hence the cell.
    pub(crate) made_by_tgradish: std::cell::RefCell<HashMap<PathBuf, (FileVersion, bool)>>,
    /// Where each item's result goes, worked out once a frame: it looks
    /// at the other items and at files on disk, and is asked for several
    /// times a frame. Cleared as each frame starts.
    pub(crate) outputs: std::cell::RefCell<HashMap<u64, PathBuf>>,
    /// The ffmpeg found for the settings, and when: looking for it
    /// searches the PATH, so it is looked for again only every few
    /// seconds, to notice one installed meanwhile, or when the settings
    /// change.
    pub(crate) found_backend: std::cell::RefCell<Option<FoundBackend>>,
    /// The ffmpeg the command line chose, over the config's for this run
    /// alone, so Settings doesn't save it.
    pub(crate) ffmpeg_override: (Option<FfmpegChoice>, Option<PathBuf>),
    /// The ffmpeg inputs were last read with, or why there was none: when
    /// it changes, reads that failed are tried again.
    pub(crate) read_with: Option<Result<Backend, String>>,
    pub(crate) screen: Screen,
    pub(crate) inset: Inset,
    pub(crate) settings_dialog: settings_dialog::SettingsDialog,
    /// The file dialog open, if one is, on a thread of its own.
    pub(crate) picking: Option<crate::media::Task<Option<crate::files::Picked>>>,
    /// A file dialog parented to the window, to ask with.
    pub(crate) file_dialog: rfd::FileDialog,
    /// A dialog of the user's presets, open over the window.
    pub(crate) preset_dialog: Option<preset_dialogs::Dialog>,
    pub(crate) inspection: Option<inspect::Inspection>,
    /// The theme Settings showed last, to apply a change made there.
    pub(crate) theme_shown: Option<crate::config::Theme>,
    /// Where the open files are kept between runs; `None` where they
    /// aren't.
    pub(crate) session: Option<session::Store>,
    /// When to see whether the session changed.
    pub(crate) next_save: f64,
    /// The system's fonts for what the window's own lack.
    pub(crate) fallbacks: fallback::Fallbacks,
    pub(crate) message: Option<String>,
    /// What Ctrl+V has done so far.
    pub(crate) paste_key: crate::keys::PasteKey,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) drops: Option<dnd::Drops>,
    /// The window's icon on Wayland, kept while the window shows it.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) _toplevel_icon: Option<toplevel_icon::ToplevelIcon>,
    /// Files are being dragged over the window, as egui or, on Wayland,
    /// the window's own listener reports it.
    pub(crate) hovering: bool,
    /// Share of the main panel's height the preview takes.
    pub(crate) preview_share: f32,
    /// The system's accent colour, as it changes.
    pub(crate) accent: Option<accent::Accent>,
    /// The accent the window's look was made with last.
    pub(crate) applied_accent: Option<Color32>,
}

impl App {
    pub(crate) fn new(launch: Launch) -> App {
        let config_path = launch.config_path.or_else(Config::default_path);
        let (config, unreadable) = match config_path.as_deref() {
            Some(path) => load_config(path),
            None => (Config::default(), None),
        };
        // before anything is said
        crate::l10n::choose(config.gui.language.as_deref());
        let mut message = unreadable.map(|(err, kept)| match kept {
            Some(kept) => {
                let kept = fl!("settings-kept", path = kept.display().to_string());
                format!("{}\n{kept}", fl!("settings-unreadable", error = err.to_string()))
            }
            None => fl!("settings-unreadable", error = err.to_string()),
        });
        let presets = Presets::load_user().unwrap_or_else(|err| {
            message = Some(fl!("presets-unreadable", error = err.to_string()));
            Presets::builtin()
        });
        let mut app = App::with(config, config_path, presets, message);
        app.ffmpeg_override = (launch.ffmpeg_choice, launch.ffmpeg_path);
        app.presets_dir = tgradish_core::presets::user_dir();
        if let Some(dir) = paths::state_dir() {
            app.reopen(dir.join("session.json"));
        }
        app
    }

    /// Opens the files of last time from the session at `path`, unless the
    /// settings say not to, and keeps the session there from now on. A
    /// window opened while another has the session starts empty and keeps
    /// none.
    pub(crate) fn reopen(&mut self, path: PathBuf) {
        let Some(mut store) = session::Store::open(path) else { return };
        // with the setting off, the first save empties what is there
        if self.config.gui.reopen_files {
            let loaded = match store.read() {
                Ok(loaded) => loaded,
                // not to be written over, then
                Err(err) => return self.say(fl!("session-unsaved", error = err.to_string())),
            };
            if let Some(unreadable) = loaded.unreadable {
                let aside = unreadable.aside.display().to_string();
                self.say(match unreadable.files {
                    Some(count) => fl!("files-unreadable", count = count, path = aside),
                    None => fl!("session-unreadable", path = aside),
                });
            }
            let restored = loaded.session.restore(store.path(), self.next_id);
            self.next_id += restored.items.len() as u64;
            self.items = restored.items;
            self.written.extend(restored.written);
            if let Some(id) = restored.selected.or(self.items.last().map(|item| item.id)) {
                self.select(id);
            }
            if restored.gone > 0 {
                self.say(fl!("files-gone", count = restored.gone));
            }
        }
        self.session = Some(store);
    }

    /// Shows `message` over the window, under what it already says there.
    fn say(&mut self, message: String) {
        self.message = Some(match self.message.take() {
            Some(said) => format!("{said}\n{message}"),
            None => message,
        });
    }

    /// Copies pasted images that have no copy yet next to the session, so
    /// they can be opened again. Where one can't be copied the session
    /// can't be kept: that is said once, as when writing it fails.
    fn copy_pasted(&mut self) {
        let Some(store) = &mut self.session else { return };
        if !self.config.gui.reopen_files {
            return;
        }
        for item in self.items.iter_mut().filter(|item| item.pasted_copy.is_none()) {
            let Some(image) = item.paste_dir.as_ref().and(item.inputs.first()) else { continue };
            match store.copy_pasted(item.id, image) {
                Ok(name) => item.pasted_copy = Some(name),
                Err(err) => {
                    self.message = Some(fl!("session-unsaved", error = err.to_string()));
                    self.session = None;
                    return;
                }
            }
        }
    }

    /// Keeps the open files and their settings, if they changed.
    pub(crate) fn save_session(&mut self) {
        // those pasted while the setting was on have none yet
        self.copy_pasted();
        let Some(store) = &mut self.session else { return };
        // with the setting off, nothing is kept: an empty session, whose
        // saving removes the copies of pasted images, so they are copied
        // again once it is back on
        let items: &[Item] = if self.config.gui.reopen_files {
            &self.items
        } else {
            self.items.iter_mut().for_each(|item| item.pasted_copy = None);
            &[]
        };
        let session = session::Session::of(items, self.selected, &self.written);
        if let Err(err) = store.save(&session) {
            self.message = Some(fl!("session-unsaved", error = err.to_string()));
            // once is enough
            self.session = None;
        }
    }

    pub(crate) fn with(
        config: Config,
        config_path: Option<PathBuf>,
        presets: Presets,
        message: Option<String>,
    ) -> App {
        App {
            config_path,
            config,
            presets,
            presets_dir: None,
            items: Vec::new(),
            next_id: 0,
            selected: None,
            recent: VecDeque::new(),
            queue: VecDeque::new(),
            written: HashMap::new(),
            made_by_tgradish: Default::default(),
            outputs: Default::default(),
            found_backend: Default::default(),
            ffmpeg_override: (None, None),
            read_with: None,
            screen: Screen::default(),
            inset: Inset::default(),
            settings_dialog: settings_dialog::SettingsDialog::default(),
            picking: None,
            file_dialog: rfd::FileDialog::new(),
            preset_dialog: None,
            inspection: None,
            theme_shown: None,
            session: None,
            next_save: 0.0,
            fallbacks: fallback::Fallbacks::new(fonts()),
            message,
            paste_key: crate::keys::PasteKey::default(),
            #[cfg(all(unix, not(target_os = "macos")))]
            drops: None,
            #[cfg(all(unix, not(target_os = "macos")))]
            _toplevel_icon: None,
            hovering: false,
            preview_share: 0.56,
            accent: None,
            applied_accent: None,
        }
    }

    pub(crate) fn style(&self, ctx: &egui::Context) {
        ctx.set_fonts(fonts());
        set_theme(ctx, self.config.gui.theme);
        theme::apply(ctx, self.applied_accent);
        ctx.all_styles_mut(|style| {
            use egui::{FontFamily, FontId, TextStyle};
            style.text_styles = [
                (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            ]
            .into();
        });
    }

    /// Makes the window's look again when the system's accent changed.
    fn follow_accent(&mut self, ctx: &egui::Context) {
        let accent = self.accent.as_ref().and_then(accent::Accent::get);
        if accent != self.applied_accent {
            self.applied_accent = accent;
            theme::apply(ctx, accent);
        }
    }

    /// Which ffmpeg to use and where, as the settings and the command line
    /// say: as on the command line, a path beats a choice.
    pub(crate) fn ffmpeg_settings(&self) -> (FfmpegChoice, Option<PathBuf>) {
        let ffmpeg = &self.config.ffmpeg;
        match &self.ffmpeg_override {
            (choice, Some(path)) => (choice.unwrap_or(ffmpeg.choice), Some(path.clone())),
            (Some(choice), None) => (*choice, None),
            (None, None) => (ffmpeg.choice, ffmpeg.path.clone()),
        }
    }

    pub(crate) fn backend(&self) -> Result<Backend, String> {
        let settings = self.ffmpeg_settings();
        let mut found = self.found_backend.borrow_mut();
        match &*found {
            Some(seen) if seen.settings == settings && seen.when.elapsed() < LOOK_AGAIN => {
                seen.backend.clone()
            }
            _ => {
                let backend = Backend::select(settings.0, settings.1.as_deref())
                    .map_err(|err| crate::jobs::failure(&err));
                let when = std::time::Instant::now();
                *found = Some(FoundBackend { settings, when, backend: backend.clone() });
                backend
            }
        }
    }

    pub(crate) fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    pub(crate) fn item_mut(&mut self, id: u64) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    pub(crate) fn select(&mut self, id: u64) {
        self.selected = Some(id);
        self.recent.retain(|&other| other != id);
        self.recent.push_front(id);
        // the oldest give their frames back
        while self.recent.len() > FRAMES_HELD {
            let old = self.recent.pop_back().expect("longer than FRAMES_HELD");
            if let Some(item) = self.item_mut(old) {
                release(item);
            }
        }
    }

    /// Adds files: each its own item. Directories are the frames of one
    /// animation.
    pub(crate) fn add(&mut self, paths: Vec<PathBuf>) {
        let mut last = None;
        for path in paths {
            let sequence = path.is_dir();
            last = Some(self.push(Item::new(
                self.next_id,
                vec![path],
                sequence,
                self.config.gui.target.unwrap_or_default(),
            )));
        }
        if let Some(id) = last {
            self.select(id);
        }
    }

    /// Adds images as the frames of one animation.
    pub(crate) fn add_frames(&mut self, files: Vec<PathBuf>) {
        if files.is_empty() {
            return;
        }
        let item = Item::new(self.next_id, files, true, self.config.gui.target.unwrap_or_default());
        let id = self.push(item);
        self.select(id);
    }

    fn push(&mut self, item: Item) -> u64 {
        let id = item.id;
        self.items.push(item);
        self.next_id += 1;
        id
    }

    /// Adds what is on the clipboard: copied files, paths or an image.
    pub(crate) fn paste(&mut self) {
        match tgradish_core::clipboard::paste() {
            Ok(Pasted { files, image_dir: None }) => self.add(files),
            Ok(Pasted { files, image_dir: Some(dir) }) => self.add_pasted(files, dir),
            Err(err) => self.message = Some(crate::jobs::failure(&err)),
        }
    }

    /// Adds a pasted image, the one file in `dir`, which is kept as long
    /// as the item is, and copied next to the session at once: `dir` goes
    /// when the window closes.
    pub(crate) fn add_pasted(&mut self, files: Vec<PathBuf>, dir: tempfile::TempDir) {
        let mut item =
            Item::new(self.next_id, files, false, self.config.gui.target.unwrap_or_default());
        item.paste_dir = Some(dir);
        let id = self.push(item);
        self.select(id);
        self.copy_pasted();
    }

    pub(crate) fn remove(&mut self, id: u64) {
        if let Some(item) = self.item(id)
            && let Some(conversion) = &item.job
        {
            conversion.job.cancel();
        }
        self.items.retain(|item| item.id != id);
        self.queue.retain(|&other| other != id);
        self.recent.retain(|&other| other != id);
        if self.selected == Some(id) {
            self.selected = self.recent.front().copied().or(self.items.last().map(|item| item.id));
        }
    }

    /// Queues the item for converting with its settings as they are now.
    pub(crate) fn enqueue(&mut self, id: u64) {
        let Some(item) = self.item(id) else { return };
        // as things are now, not as they were when the frame started
        let output = self.work_out_output(item);
        let item = self.item_mut(id).expect("found above");
        if let Some(conversion) = &item.job {
            conversion.job.cancel();
        }
        item.job = Some(Conversion::of(Job::waiting(output), item.format, item.choices.clone()));
        self.queue.retain(|&other| other != id);
        self.queue.push_back(id);
    }

    /// Changes a setting of the window, and keeps that change in the config
    /// file at once, without the rest of what Settings changed.
    pub(crate) fn save_setting(&mut self, change: impl Fn(&mut crate::config::GuiConfig)) {
        self.save_config_setting(|config| change(&mut config.gui));
    }

    /// [`App::save_setting`] for any of the settings.
    pub(crate) fn save_config_setting(&mut self, change: impl Fn(&mut Config)) {
        change(&mut self.config);
        if let Some(path) = &self.config_path {
            let saved = Config::load(path).and_then(|mut saved| {
                change(&mut saved);
                saved.save(path)
            });
            if let Err(err) = saved {
                self.message = Some(fl!("setting-unsaved", error = err.to_string()));
            }
        }
    }

    /// Whether a dialog is open over the window.
    pub(crate) fn dialog_open(&self) -> bool {
        self.picking.is_some()
            || self.settings_dialog.is_open()
            || self.settings_dialog.about
            || self.inspection.is_some()
            || self.preset_dialog.is_some()
    }
}

/// The settings at `path`, or the defaults where they can't be read, with
/// why and where the file is kept as it was: copied aside first, as saving
/// Settings writes over it.
fn load_config(path: &Path) -> (Config, Option<(tgradish_core::Error, Option<PathBuf>)>) {
    match Config::load(path) {
        Ok(config) => (config, None),
        Err(err) => {
            let aside = path.with_extension("toml.unreadable");
            let kept = std::fs::copy(path, &aside).is_ok().then_some(aside);
            (Config::default(), Some((err, kept)))
        }
    }
}

/// How long a closing window waits for conversions to stop, so they take
/// their temporary files and ffmpeg's processes with them.
const STOPPING: std::time::Duration = std::time::Duration::from_secs(3);

impl App {
    /// Stops every conversion, and waits a little for them to clean up.
    fn stop_converting(&mut self) {
        let jobs = || self.items.iter().filter_map(|item| item.job.as_ref().map(|c| &c.job));
        jobs().for_each(Job::cancel);
        let deadline = std::time::Instant::now() + STOPPING;
        for item in &mut self.items {
            if let Some(conversion) = &mut item.job {
                conversion.job.wait_until(deadline);
            }
        }
    }
}

/// Makes the window light or dark.
pub(crate) fn set_theme(ctx: &egui::Context, theme: crate::config::Theme) {
    use crate::config::Theme;
    ctx.set_theme(match theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
}

/// How long an ffmpeg found, or not found, is taken as it is.
const LOOK_AGAIN: std::time::Duration = std::time::Duration::from_secs(3);

/// The ffmpeg found for the settings `settings`, at `when`.
pub(crate) struct FoundBackend {
    settings: (FfmpegChoice, Option<PathBuf>),
    when: std::time::Instant,
    backend: Result<Backend, String>,
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
        self.stop_converting();
        // the listener shares winit's Wayland connection, which closes next
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(drops) = &mut self.drops {
            drops.stop();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // file dialogs stay over the window, which they are modal for
        self.file_dialog = rfd::FileDialog::new().set_parent(frame);
        self.take_picked();
        self.outputs.borrow_mut().clear();
        self.follow_accent(&ctx);
        if !self.config.gui.smooth_scrolling {
            direct_scrolling(&ctx);
        }
        self.take_input(&ctx);
        self.pump(&ctx);
        self.find_fonts(&ctx);
        // as wide as half the window, leaving the rest the room it needs
        let window = ctx.content_rect().width();
        let widest = (window * 0.5).min(window - crate::preview::NARROWEST_MAIN).max(260.0);
        // apart from the width it has in a small window, or that would stay
        // once the window grows again
        let id = if widest < FILES_WIDTH { "files-small" } else { "files" };
        egui::Panel::left(id)
            .resizable(true)
            .default_size(FILES_WIDTH)
            .size_range(260.0..=widest)
            .show(ui, |ui| {
                self.files_panel(ui);
            });
        egui::CentralPanel::default().show(ui, |ui| self.main_panel(ui));
        let ffmpeg_given = self.ffmpeg_override != (None, None);
        settings_dialog::show(
            &ctx,
            &mut self.settings_dialog,
            &mut self.config,
            ffmpeg_given,
            self.config_path.as_deref(),
            &self.presets,
        );
        if let Some(pick) = self.settings_dialog.pick.take() {
            self.pick(&ctx, pick);
        }
        if let Some((format, name)) = self.settings_dialog.default_preset.take() {
            self.set_default_preset(format, name);
        }
        // as Settings or the theme button changes it
        if self.theme_shown != Some(self.config.gui.theme) {
            set_theme(&ctx, self.config.gui.theme);
            self.theme_shown = Some(self.config.gui.theme);
        }
        self.inspection_dialog(&ctx);
        self.preset_dialogs(&ctx);
        if self.settings_dialog.about {
            let modal = egui::Modal::new(egui::Id::new("about")).show(&ctx, |ui| {
                ui.set_width(600.0);
                about(ui);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.button(fl!("close")).clicked()
                })
                .inner
            });
            if modal.inner || modal.should_close() {
                self.settings_dialog.about = false;
            }
        }
        if let Some(message) = &self.message
            && !toast(&ctx, message)
        {
            self.message = None;
        }
        if self.hovering {
            let painter =
                ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, "drop".into()));
            let screen = ctx.content_rect();
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                fl!("drop-to-add"),
                egui::FontId::proportional(30.0),
                Color32::WHITE,
            );
            // egui says each frame whether files hover; Wayland says when they leave
            #[cfg(all(unix, not(target_os = "macos")))]
            let reported = self.drops.is_some();
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            let reported = false;
            if !reported {
                self.hovering = false;
            }
        }
        // links clicked, opened as files are shown in their folder: eframe
        // opens them only with a dependency much larger than this
        let mut links = Vec::new();
        ctx.output_mut(|output| {
            output.commands.retain(|command| match command {
                egui::OutputCommand::OpenUrl(open) => {
                    links.push(open.url.clone());
                    false
                }
                _ => true,
            })
        });
        links.iter().for_each(|url| system::browse(url));
        // every few seconds, so a crash loses little; what changed is kept
        // even if nothing draws the window again before then
        let now = ctx.input(|input| input.time);
        if now >= self.next_save {
            self.next_save = now + 2.0;
            self.save_session();
        } else if self.session.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.next_save - now));
        }
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tgradish_core::presets::Format;
    use tgradish_core::telegram;

    use super::*;
    use crate::testing::{convert, finish, harness, pasted_image, square};

    #[test]
    fn copies_pasted_images_as_they_are_pasted() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let copies = dir.path().join("state").join("pasted");
        let mut harness = harness(Config::default());
        harness.state_mut().reopen(state.clone());
        let (files, folder) = pasted_image();
        harness.state_mut().add_pasted(files, folder);
        // there before the session is next written
        assert_eq!(std::fs::read_dir(&copies).unwrap().count(), 1);
        assert!(harness.state().items[0].pasted_copy.is_some() && !state.exists());

        // where copies can't be made, that is said once and not tried again
        std::fs::remove_dir_all(&copies).unwrap();
        std::fs::write(&copies, b"in the way").unwrap();
        for _ in 0..2 {
            let (files, folder) = pasted_image();
            harness.state_mut().add_pasted(files, folder);
            let app = harness.state_mut();
            let said = app.message.take().is_some_and(|said| said.contains("kept for next time"));
            assert_eq!(said, app.items.len() == 2);
            assert!(app.session.is_none() && app.items.last().unwrap().pasted_copy.is_none());
        }
        harness.state_mut().save_session();
        assert_eq!(std::fs::read(&copies).unwrap(), b"in the way");
    }

    #[test]
    fn copies_pasted_images_again_once_the_files_are_kept_again() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let mut first = harness(Config::default());
        first.state_mut().reopen(state.clone());
        let (files, folder) = pasted_image();
        first.state_mut().add_pasted(files, folder);
        first.state_mut().save_session();
        // turned off, and on again before the window closes
        first.state_mut().config.gui.reopen_files = false;
        first.state_mut().save_session();
        first.state_mut().config.gui.reopen_files = true;
        first.state_mut().save_session();
        drop(first);

        let mut second = harness(Config::default());
        second.state_mut().reopen(state);
        assert_eq!(second.state().items.len(), 1);
        assert!(second.state().message.is_none(), "{:?}", second.state().message);
    }

    #[test]
    fn leaves_the_session_to_the_window_that_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let mut first = harness(Config::default());
        first.state_mut().reopen(state.clone());
        let (files, folder) = pasted_image();
        first.state_mut().add_pasted(files, folder);
        first.state_mut().save_session();
        let written = std::fs::read_to_string(&state).unwrap();

        // a second window meanwhile, whose own files are kept nowhere
        let mut second = harness(Config::default());
        second.state_mut().reopen(state.clone());
        assert!(second.state().session.is_none() && second.state().items.is_empty());
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        second.state_mut().add(vec![art]);
        second.state_mut().save_session();
        assert!(second.state().message.is_none());
        assert_eq!(std::fs::read_to_string(&state).unwrap(), written);
        assert_eq!(std::fs::read_dir(state.with_file_name("pasted")).unwrap().count(), 1);

        // the next one, once the first has closed, has its files
        drop(first);
        let mut third = harness(Config::default());
        third.state_mut().reopen(state);
        assert_eq!(third.state().items.len(), 1);
    }

    #[test]
    fn makes_a_pasted_image_again_where_it_made_it_last_time() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let output = dir.path().join("out").join("clipboard.sticker.tgs");
        let mut first = harness(config.clone());
        first.state_mut().reopen(state.clone());
        let (files, folder) = pasted_image();
        first.state_mut().add_pasted(files, folder);
        finish(&mut first);
        convert(&first);
        first.run_steps(2);
        finish(&mut first);
        assert_eq!(crate::testing::made_job(&first.state().items[0]).output, output);
        first.state_mut().save_session();
        drop(first);

        // the next window names the result the same, and knows the file
        // there for the image's own
        let mut second = harness(config);
        second.state_mut().reopen(state);
        finish(&mut second);
        let app = second.state();
        assert_eq!(app.work_out_output(&app.items[0]), output);
        second.state_mut().items[0].choices.tgs.keep_canvas = Some(true);
        second.run_steps(2);
        convert(&second);
        second.run_steps(2);
        finish(&mut second);
        assert_eq!(crate::testing::made_job(&second.state().items[0]).output, output);
        assert_eq!(std::fs::read_dir(dir.path().join("out")).unwrap().count(), 1);
    }

    #[test]
    fn shows_a_result_of_last_time_as_it_was_made() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let state = dir.path().join("state").join("session.json");
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut first = harness(config.clone());
        first.state_mut().reopen(state.clone());
        first.state_mut().add(vec![art]);
        finish(&mut first);
        {
            let item = &mut first.state_mut().items[0];
            (item.format, item.format_guessed) = (Format::Webp, false);
            item.choices.target = telegram::Target::Emoji;
        }
        first.run_steps(2);
        convert(&first);
        first.run_steps(2);
        finish(&mut first);
        first.state_mut().save_session();
        drop(first);

        let mut second = harness(config);
        second.state_mut().reopen(state);
        finish(&mut second);
        let item = &second.state().items[0];
        assert!(item.result_is_current());
        assert_eq!(item.made.as_ref().map(|made| made.format), Some(Format::Webp));
        // checked as an emoji, which a sticker's size isn't
        let Some(crate::jobs::Status::Done(done)) = item.made.as_ref().map(|made| &made.job.status)
        else {
            panic!("not made");
        };
        assert!(done.problems.is_empty(), "{:?}", done.problems);
        second.get_by_label_contains("Telegram should accept it");
        // and shown
        assert!(item.result_clip.ready().is_some() && item.result_thumb.is_some());
    }

    #[test]
    fn stops_converting_when_the_window_closes() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = crate::testing::video_of(dir.path(), "1280x720", 8) else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        let running = |app: &App| app.items[0].job.as_ref().is_some_and(|c| c.job.is_running());
        assert!(running(harness.state()));
        eframe::App::on_exit(harness.state_mut(), None);
        // its worker is done, having said it stopped
        let job = &mut harness.state_mut().items[0].job.as_mut().unwrap().job;
        assert!(job.wait_until(std::time::Instant::now()));
        job.poll();
        assert!(matches!(job.status, crate::jobs::Status::Cancelled), "{:?}", job.status);
    }

    #[test]
    fn saves_no_ffmpeg_the_command_line_chose() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut app = App::with(Config::default(), Some(path.clone()), Presets::builtin(), None);
        app.ffmpeg_override = (None, Some(dir.path().join("no-ffmpeg-here")));
        // used, though nothing is found there
        assert!(app.backend().is_err());
        app.save_setting(|gui| gui.overwrite = true);
        app.config.save(&path).unwrap();
        let saved = Config::load(&path).unwrap();
        assert!(saved.gui.overwrite && saved.ffmpeg == Config::default().ffmpeg);
    }

    #[test]
    fn keeps_settings_it_cannot_read_before_they_are_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "made-up = 1\n").unwrap();
        let (config, unreadable) = load_config(&path);
        let (_, kept) = unreadable.expect("unreadable");
        assert_eq!(config, Config::default());
        assert_eq!(kept, Some(dir.path().join("config.toml.unreadable")));
        assert_eq!(std::fs::read_to_string(kept.unwrap()).unwrap(), "made-up = 1\n");
        // a file that is fine is left alone
        std::fs::write(&path, "").unwrap();
        assert!(load_config(&path).1.is_none());
    }

    #[test]
    fn keeps_a_session_it_cannot_read_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("session.json");
        let aside = dir.path().join("session.json.unreadable");
        std::fs::write(&state, r#"{"version":99,"items":[{"of":"a later tgradish"}]}"#).unwrap();
        let mut harness = harness(Config::default());
        harness.state_mut().reopen(state.clone());
        harness.run_steps(2);
        harness.get_by_label_contains("The files of last time couldn't be read");
        // written anew by this window, the other kept beside it
        harness.state_mut().save_session();
        assert!(std::fs::read_to_string(&aside).unwrap().contains("a later tgradish"));
        assert!(std::fs::read_to_string(&state).unwrap().contains(r#""version":1"#));

        // told to forget the files, the next window reads none and keeps none
        drop(harness);
        std::fs::write(&state, "anything").unwrap();
        let mut config = Config::default();
        config.gui.reopen_files = false;
        let mut harness = self::harness(config);
        harness.state_mut().reopen(state.clone());
        harness.state_mut().save_session();
        assert!(harness.state().message.is_none());
        assert!(std::fs::read_to_string(&state).unwrap().contains(r#""items":[]"#));
        assert!(std::fs::read_to_string(&aside).unwrap().contains("a later tgradish"));
    }

    #[test]
    fn reopens_the_files_of_last_time() {
        let dir = tempfile::tempdir().unwrap();
        let (art, gone) = (dir.path().join("art.png"), dir.path().join("gone.png"));
        square(&art, [255, 0, 0, 255]);
        square(&gone, [0, 0, 255, 255]);
        let state = dir.path().join("state").join("session.json");
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut first = harness(config.clone());
        first.state_mut().reopen(state.clone());
        first.state_mut().add(vec![gone.clone(), art.clone()]);
        finish(&mut first);
        {
            let item = &mut first.state_mut().items[1];
            item.format = Format::Tgs;
            item.format_guessed = false;
            item.choices.target = telegram::Target::Emoji;
            item.choices.tgs.keep_canvas = Some(true);
        }
        first.run_steps(2);
        convert(&first);
        first.run_steps(2);
        finish(&mut first);
        first.state_mut().save_session();
        assert!(state.exists());
        drop(first);

        // a file goes before the window opens again
        std::fs::remove_file(&gone).unwrap();
        let mut second = harness(config);
        second.state_mut().reopen(state);
        // its result is read as the window draws, not before it opens
        assert!(second.state().items[0].made.is_none());
        assert!(second.state().items[0].earlier.is_some());
        finish(&mut second);
        let app = second.state();
        assert_eq!(app.items.len(), 1);
        let item = &app.items[0];
        assert_eq!((item.inputs.as_slice(), item.format), (&[art][..], Format::Tgs));
        assert_eq!(item.choices.target, telegram::Target::Emoji);
        assert_eq!(item.choices.tgs.keep_canvas, Some(true));
        assert_eq!(app.selected, Some(item.id));
        // the result made with these settings is still the current one
        assert!(item.result_is_current());
        assert!(app.message.as_deref().is_some_and(|m| m.contains("1 file is gone")));
    }
}

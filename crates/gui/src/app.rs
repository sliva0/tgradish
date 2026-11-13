//! The window's state and its frame: the files open, what waits to be
//! converted, the dialogs and the theme. The other modules add their parts
//! to `App`; this holds what they share, and draws them in turn.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use eframe::egui::{self, Color32};
use tgradish_core::backend::Backend;
use tgradish_core::clipboard::Pasted;
use tgradish_core::config::Config;
use tgradish_core::paths;
use tgradish_core::presets::Presets;

use crate::Launch;
use crate::about::about;
use crate::canvas::Screen;
use crate::fonts::fonts;
use crate::inset::Inset;
use crate::item::Item;
use crate::jobs::Job;
use crate::keys::direct_scrolling;
use crate::l10n::fl;
use crate::outputs::{FileVersion, Written};
use crate::widgets::toast;
use crate::work::release;
use crate::{accent, fallback, inspect, prefs, preset_dialogs, session, system, theme};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::{dnd, toplevel_icon};

/// How many items keep their big frames: the selected one and the last few.
const KEPT: usize = 3;
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
    pub(crate) screen: Screen,
    pub(crate) inset: Inset,
    pub(crate) prefs: prefs::Prefs,
    /// A dialog of the user's presets, open over the window.
    pub(crate) preset_dialog: Option<preset_dialogs::Dialog>,
    pub(crate) inspection: Option<inspect::Inspection>,
    /// The theme Settings showed last, to apply a change made there.
    pub(crate) prefs_theme: Option<tgradish_core::config::Theme>,
    /// Where the open files are kept between runs; `None` where they
    /// aren't.
    pub(crate) session: Option<session::Store>,
    /// When to see whether the session changed.
    pub(crate) next_save: f64,
    /// The system's fonts for what the window's own lack.
    pub(crate) fallbacks: fallback::Fallbacks,
    pub(crate) message: Option<String>,
    /// The clipboard was read for the Ctrl+V being held.
    pub(crate) pasted: bool,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) drops: Option<dnd::Drops>,
    /// The window's icon on Wayland, kept while the window shows it.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) _toplevel_icon: Option<toplevel_icon::ToplevelIcon>,
    /// Files are being dragged over the window, as Wayland reports it.
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
        let (mut config, unreadable) = match config_path.as_deref().map(Config::load) {
            Some(Ok(config)) => (config, None),
            Some(Err(err)) => (Config::default(), Some(err)),
            None => (Config::default(), None),
        };
        // before anything is said
        crate::l10n::choose(config.gui.language.as_deref());
        let mut message = unreadable.map(|err| fl!("settings-unreadable", error = err.to_string()));
        // as on the command line: a path beats a choice
        if let Some(choice) = launch.ffmpeg_choice {
            config.ffmpeg.choice = choice;
            config.ffmpeg.path = None;
        }
        if let Some(path) = launch.ffmpeg_path {
            config.ffmpeg.path = Some(path);
        }
        let presets = Presets::load_user().unwrap_or_else(|err| {
            message = Some(fl!("presets-unreadable", error = err.to_string()));
            Presets::builtin()
        });
        let mut app = App::with(config, config_path, presets, message);
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
    fn reopen(&mut self, path: PathBuf) {
        let Some(mut store) = session::Store::open(path) else { return };
        // with the setting off, the first save empties what is there
        if !self.config.gui.forget_files {
            let loaded = match store.read() {
                Ok(loaded) => loaded,
                // not to be written over, then
                Err(err) => return self.say(fl!("session-unsaved", error = err.to_string())),
            };
            if let Some(unreadable) = loaded.unreadable {
                let kept = unreadable.kept.display().to_string();
                self.say(match unreadable.files {
                    Some(count) => fl!("files-unreadable", count = count, path = kept),
                    None => fl!("session-unreadable", path = kept),
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
    fn keep_pasted(&mut self) {
        let Some(store) = &mut self.session else { return };
        if self.config.gui.forget_files {
            return;
        }
        for item in self.items.iter_mut().filter(|item| item.kept.is_none()) {
            let Some(image) = item.pasted.as_ref().and(item.inputs.first()) else { continue };
            match store.keep_pasted(item.id, image) {
                Ok(name) => item.kept = Some(name),
                Err(err) => {
                    self.message = Some(fl!("session-unsaved", error = err.to_string()));
                    self.session = None;
                    return;
                }
            }
        }
    }

    /// Keeps the open files and their settings, if they changed.
    fn save_session(&mut self) {
        // those pasted while the setting was on have none yet
        self.keep_pasted();
        let Some(store) = &mut self.session else { return };
        // with the setting off, nothing is kept: an empty session
        let items: &[Item] = if self.config.gui.forget_files { &[] } else { &self.items };
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
            screen: Screen::default(),
            inset: Inset::default(),
            prefs: prefs::Prefs::default(),
            preset_dialog: None,
            inspection: None,
            prefs_theme: None,
            session: None,
            next_save: 0.0,
            fallbacks: fallback::Fallbacks::new(fonts()),
            message,
            pasted: false,
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

    pub(crate) fn backend(&self) -> Result<Backend, String> {
        let ffmpeg = &self.config.ffmpeg;
        let settings = (ffmpeg.choice, ffmpeg.path.clone());
        let mut found = self.found_backend.borrow_mut();
        match &*found {
            Some(seen) if seen.settings == settings && seen.when.elapsed() < LOOK_AGAIN => {
                seen.backend.clone()
            }
            _ => {
                let backend = Backend::select(ffmpeg.choice, ffmpeg.path.as_deref())
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
        while self.recent.len() > KEPT {
            let old = self.recent.pop_back().expect("longer than KEPT");
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
        item.pasted = Some(dir);
        let id = self.push(item);
        self.select(id);
        self.keep_pasted();
    }

    pub(crate) fn remove(&mut self, id: u64) {
        if let Some(item) = self.item(id)
            && let Some((job, ..)) = &item.job
        {
            job.cancel();
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
        if let Some((job, ..)) = &item.job {
            job.cancel();
        }
        item.job = Some((Job::waiting(output), item.format, item.choices.clone()));
        self.queue.retain(|&other| other != id);
        self.queue.push_back(id);
    }

    /// Changes a setting of the window, and keeps that change in the config
    /// file at once, without the rest of what Settings changed.
    pub(crate) fn keep(&mut self, change: impl Fn(&mut tgradish_core::config::GuiConfig)) {
        self.keep_config(|config| change(&mut config.gui));
    }

    /// [`App::keep`] for any of the settings.
    pub(crate) fn keep_config(&mut self, change: impl Fn(&mut Config)) {
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
        self.prefs.is_open()
            || self.prefs.about
            || self.inspection.is_some()
            || self.preset_dialog.is_some()
    }
}

/// Makes the window light or dark.
pub(crate) fn set_theme(ctx: &egui::Context, theme: tgradish_core::config::Theme) {
    use tgradish_core::config::Theme;
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
    settings: (tgradish_core::ffmpeg::FfmpegChoice, Option<PathBuf>),
    when: std::time::Instant,
    backend: Result<Backend, String>,
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
        // the listener shares winit's Wayland connection, which closes next
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(drops) = &mut self.drops {
            drops.stop();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
        prefs::show(
            &ctx,
            &mut self.prefs,
            &mut self.config,
            self.config_path.as_deref(),
            &self.presets,
        );
        // as Settings or the theme button changes it
        if self.prefs_theme != Some(self.config.gui.theme) {
            set_theme(&ctx, self.config.gui.theme);
            self.prefs_theme = Some(self.config.gui.theme);
        }
        self.inspection_dialog(&ctx);
        self.preset_dialogs(&ctx);
        if self.prefs.about {
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
                self.prefs.about = false;
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
        assert!(harness.state().items[0].kept.is_some() && !state.exists());

        // where copies can't be made, that is said once and not tried again
        std::fs::remove_dir_all(&copies).unwrap();
        std::fs::write(&copies, b"in the way").unwrap();
        for _ in 0..2 {
            let (files, folder) = pasted_image();
            harness.state_mut().add_pasted(files, folder);
            let app = harness.state_mut();
            let said = app.message.take().is_some_and(|said| said.contains("kept for next time"));
            assert_eq!(said, app.items.len() == 2);
            assert!(app.session.is_none() && app.items.last().unwrap().kept.is_none());
        }
        harness.state_mut().save_session();
        assert_eq!(std::fs::read(&copies).unwrap(), b"in the way");
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
        config.gui.forget_files = true;
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
        second.run_steps(2);
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

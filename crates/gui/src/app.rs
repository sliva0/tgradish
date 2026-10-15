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
    /// Where the open files are kept between runs, and what was written
    /// there last.
    pub(crate) session: Option<(PathBuf, String)>,
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
    /// settings say not to, and keeps the session there from now on.
    fn reopen(&mut self, path: PathBuf) {
        let session = session::Session::load(&path).filter(|_| !self.config.gui.forget_files);
        let saved = session.as_ref().and_then(|session| session.to_json().ok()).unwrap_or_default();
        if let Some(session) = session {
            let (items, selected, gone) = session.restore(&path, self.next_id);
            self.next_id += items.len() as u64;
            self.items = items;
            if let Some(id) = selected.or(self.items.last().map(|item| item.id)) {
                self.select(id);
            }
            if gone > 0 {
                self.message = Some(fl!("files-gone", count = gone));
            }
        }
        self.session = Some((path, saved));
    }

    /// Keeps the open files and their settings, if they changed.
    fn save_session(&mut self) {
        let Some((path, saved)) = &self.session else { return };
        // with the setting off, nothing is kept: an empty session
        let items: &mut [Item] =
            if self.config.gui.forget_files { &mut [] } else { &mut self.items };
        let session = session::Session::of(items, self.selected, path);
        let saved_now = session.to_json().map_err(std::io::Error::other).and_then(|json| {
            if json != *saved {
                session.save(&json, path)?;
            }
            Ok(json)
        });
        match saved_now {
            Ok(json) => self.session = Some((path.clone(), json)),
            Err(err) => {
                self.message = Some(fl!("session-unsaved", error = err.to_string()));
                // once is enough
                self.session = None;
            }
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
            Ok(Pasted { files, image_dir: Some(dir) }) => {
                let mut item = Item::new(
                    self.next_id,
                    files,
                    false,
                    self.config.gui.target.unwrap_or_default(),
                );
                item.pasted = Some(dir);
                let id = self.push(item);
                self.select(id);
            }
            Err(err) => self.message = Some(crate::jobs::failure(&err)),
        }
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
        // as wide as half the window
        let widest = (ctx.content_rect().width() * 0.5).max(260.0);
        egui::Panel::left("files")
            .resizable(true)
            .default_size(410.0)
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
    use tgradish_core::presets::Format;
    use tgradish_core::telegram;

    use super::*;
    use crate::testing::{convert, finish, harness, square};

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

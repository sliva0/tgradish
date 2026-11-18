//! The GUI's state and its frame: the files open, what waits to be
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
use crate::banner::{Action, Banner, Message, Subject};
use crate::canvas::Screen;
use crate::fonts::fonts;
use crate::inset::Inset;
use crate::item::{Conversion, Item};
use crate::jobs::{Job, failure};
use crate::keys::direct_scrolling;
use crate::l10n::fl;
use crate::outputs::{FileVersion, Written};
use crate::work::release;
use crate::{
    accent, fallback, inspect, instance, preset_dialogs, session, settings_dialog, system, theme,
    widgets,
};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::{dnd, toplevel_icon};

/// How many items keep their big frames: the selected one and the last few.
const FRAMES_HELD: usize = 3;
/// How wide the list of files starts, and the least it narrows to.
const FILES_WIDTH: f32 = 410.0;
pub(crate) const NARROWEST_LIST: f32 = 260.0;
/// The share of the height beside the list the preview starts with.
pub(crate) const PREVIEW_SHARE: f32 = 0.56;

/// Where the session is kept, if anywhere.
pub(crate) fn session_path() -> Option<PathBuf> {
    paths::state_dir().map(|dir| dir.join("session.json"))
}

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
    /// How many were asked to be converted since nothing was.
    pub(crate) batch: usize,
    /// Results the GUI wrote, which it replaces when making them again.
    pub(crate) written: HashMap<PathBuf, Written>,
    /// Files found in the way, by their size and time: whether tgradish
    /// made them, so they may be replaced. Working out where results go
    /// fills it in, though it only borrows `App`, hence the cell.
    pub(crate) made_by_tgradish: std::cell::RefCell<HashMap<PathBuf, (FileVersion, bool)>>,
    /// What was seen of files where results go, looked at again only now
    /// and then rather than on every frame.
    pub(crate) probes: crate::outputs::Probes,
    /// Where each item's result goes, worked out once a frame: it looks
    /// at the other items and at files on disk, and is asked for several
    /// times a frame. Cleared as each frame starts.
    pub(crate) outputs: std::cell::RefCell<HashMap<u64, PathBuf>>,
    /// The ffmpeg found for the settings, and when: looking for it
    /// searches the PATH, so it is looked for again only every few
    /// seconds, to notice one installed meanwhile, or when the settings
    /// change.
    pub(crate) found_backend: std::cell::RefCell<Option<FoundBackend>>,
    /// The ffmpeg given as the GUI was started, by the command line or
    /// `TGRADISH_FFMPEG`, over the config's for this run alone, so
    /// Settings doesn't save it.
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
    /// A dialog of the user's presets, open over the rest.
    pub(crate) preset_dialog: Option<preset_dialogs::Dialog>,
    pub(crate) inspection: Option<inspect::Inspection>,
    /// Whether the dialog of tgradish's version and licences is open.
    pub(crate) about: bool,
    /// How many times About was closed, which it starts afresh by: how
    /// far it is scrolled and which licences are open are forgotten.
    pub(crate) about_visit: u64,
    /// Whether the dialog that lists the keyboard's shortcuts is open.
    pub(crate) shortcuts: bool,
    /// The theme Settings showed last, to apply a change made there.
    pub(crate) theme_shown: Option<crate::config::Theme>,
    /// The scale shown last, to apply a change made in Settings
    /// and save one made by egui's keys.
    pub(crate) scale_shown: Option<f32>,
    /// The scale asked of egui, and the pointer kept where it is on the
    /// screen as it changes.
    pub(crate) scaling: crate::scaling::Scaling,
    /// Where the open files are kept between runs; `None` where they
    /// aren't.
    pub(crate) session: Option<session::Store>,
    /// Where GUIs started while this one runs hand it their files.
    pub(crate) instance: Option<instance::Listener>,
    /// The window asked for attention, for files handed over, until it
    /// has the focus.
    pub(crate) attention: bool,
    /// When to see whether the session changed.
    pub(crate) next_save: f64,
    /// The system's fonts for what tgradish's own lack.
    pub(crate) fallbacks: fallback::Fallbacks,
    /// What the texts were when fonts were last looked for them.
    pub(crate) fonts_checked: Option<u64>,
    /// What is said along the bottom.
    pub(crate) banner: Banner,
    /// Files removed a moment ago, which may be put back.
    pub(crate) removed: Option<Removed>,
    /// What the last frames were, for which changes to files' settings
    /// are steps to undo.
    pub(crate) edit_frames: crate::history::Frames,
    /// The file whose settings were drawn last, which is the one picked
    /// but for the frame a number typed in another's is taken.
    pub(crate) drawn: Option<u64>,
    /// The file with a number typed in its settings, not yet taken, as the
    /// frame began.
    pub(crate) typed: Option<u64>,
    /// Files to remove once the number typed in one of them is taken.
    pub(crate) removing: Vec<u64>,
    /// The settings as they were before Settings put them back to their
    /// defaults a moment ago, which may be put back.
    pub(crate) settings_reset: Option<Config>,
    /// The clipboard results are copied to, as files.
    pub(crate) clipboard: system::Clipboard,
    /// What Ctrl+V has done so far.
    pub(crate) paste_key: crate::keys::PasteKey,
    /// What keys asked of the bar under the preview this frame.
    pub(crate) keyed: crate::keys::Keyed,
    /// The file picked by its key, which the list scrolls to.
    pub(crate) reveal_selected: bool,
    /// What file dialogs were asked for, which tests don't open.
    #[cfg(test)]
    pub(crate) asked_to_pick: Vec<crate::files::Pick>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) drops: Option<dnd::Drops>,
    /// The window's icon on Wayland, kept while the window shows it.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) _toplevel_icon: Option<toplevel_icon::ToplevelIcon>,
    /// Files are being dragged over the window, as egui or, on Wayland,
    /// tgradish's own listener reports it.
    pub(crate) hovering: bool,
    /// Share of the main panel's height the preview takes.
    pub(crate) preview_share: f32,
    /// How the window is laid out, kept in the session: as it was last
    /// time until it is drawn.
    pub(crate) window: Option<session::Window>,
    /// The system's accent colour, as it changes.
    pub(crate) accent: Option<accent::Accent>,
    /// The accent the look was made with last: the one chosen, or
    /// else the system's.
    pub(crate) applied_accent: Option<Color32>,
    /// Whether the system asks for less motion, as it changes.
    pub(crate) system_motion: Option<crate::motion::System>,
    /// How the list's rows change, as they fade in and close up.
    pub(crate) rows: crate::files::Rows,
}

impl App {
    /// The GUI as `launch` starts it, keeping the session in `session`
    /// where it is the one to, and hearing from the GUIs started after it
    /// on `listener`.
    pub(crate) fn new(
        launch: Launch,
        session: Option<session::Store>,
        listener: Option<instance::Listener>,
    ) -> App {
        let config_path = launch.config_path.or_else(Config::default_path);
        let (config, unreadable) = match config_path.as_deref() {
            Some(path) => load_config(path),
            None => (Config::default(), None),
        };
        // before anything is said
        crate::l10n::choose(config.gui.language.as_deref());
        let mut app = App::with(config, config_path, Presets::builtin());
        app.ffmpeg_override = (launch.ffmpeg_choice, launch.ffmpeg_path);
        app.start(unreadable, tgradish_core::presets::user_dir());
        if let Some(store) = session {
            app.resume(store);
        }
        app.instance = listener;
        app.add(launch.files);
        app
    }

    /// Says why the settings couldn't be read, where `unreadable` gives
    /// why, and reads the presets in `presets_dir`, as the GUI starts.
    fn start(&mut self, unreadable: Option<tgradish_core::Error>, presets_dir: Option<PathBuf>) {
        if let Some(err) = unreadable {
            self.say(settings_unreadable(fl!("settings-unreadable"), &err));
        }
        self.presets_dir = presets_dir;
        self.reload_presets();
    }

    /// Opens the files of last time from the session at `path`, unless the
    /// settings say not to, and keeps the session there from now on. A
    /// second instance started while another has the session, which it
    /// couldn't hand its files to, starts empty and keeps none.
    #[cfg(test)]
    pub(crate) fn reopen(&mut self, path: PathBuf) {
        if let Some(store) = session::Store::open(path) {
            self.resume(store);
        }
    }

    /// Opens the files of last time from the session `store` holds,
    /// unless the settings say not to, and keeps the session there from
    /// now on.
    pub(crate) fn resume(&mut self, mut store: session::Store) {
        // laid out as last time, whether or not its files are opened again
        if let Some(window) = session::window(store.path()) {
            self.preview_share = window.preview;
            self.window = Some(window);
        }
        // with the setting off, the first save empties what is there
        if self.config.gui.reopen_files {
            let loaded = match store.read() {
                Ok(loaded) => loaded,
                // not to be written over, then
                Err(err) => return self.say(session_unsaved(store.path(), &err)),
            };
            if let Some(unreadable) = loaded.unreadable {
                let message = Message::info(match unreadable.files {
                    Some(count) => fl!("files-unreadable", count = count),
                    None => fl!("session-unreadable"),
                });
                self.say(message.file(unreadable.aside).actions(&[Action::ShowInFolder]));
            }
            let restored = loaded.session.restore(store.path(), self.next_id);
            self.next_id += restored.items.len() as u64;
            self.items = restored.items;
            self.written.extend(restored.written);
            if let Some(id) = restored.selected.or(self.items.last().map(|item| item.id)) {
                self.select(id);
            }
            if restored.gone > 0 {
                self.say(Message::info(fl!("files-gone", count = restored.gone)));
            }
        }
        self.session = Some(store);
    }

    /// Shows `message` under what is said already, unless it is among it:
    /// in the corner over the list where it passes, in the banner where it
    /// stays.
    pub(crate) fn say(&mut self, message: Message) {
        self.banner.say(message);
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
                    let message = session_unsaved(store.path(), &err);
                    self.session = None;
                    return self.say(message);
                }
            }
        }
    }

    /// Adds the files GUIs started since handed over, as dropped files
    /// are, whatever dialog is open, and brings the window forward for
    /// them: where it was raised with a token already, as on Wayland, by
    /// egui's means too, and where it wasn't, asking for attention as
    /// well, as a system may not let a window take the focus itself.
    fn take_handed(&mut self, ctx: &egui::Context) {
        let (minimized, focused) = ctx.input(|input| {
            let viewport = input.viewport();
            (viewport.minimized == Some(true), viewport.focused == Some(true))
        });
        if self.attention && focused {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Reset,
            ));
            self.attention = false;
        }
        let handed = self.instance.as_ref().map(instance::Listener::poll).unwrap_or_default();
        if handed.is_empty() {
            return;
        }
        let raised = handed.iter().all(|handed| handed.raised);
        for handed in handed {
            self.add(handed.files);
        }
        // restored only from being minimised, as Windows would restore a
        // maximised window too
        if minimized {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        if !raised && !focused {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
            self.attention = true;
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
        let removed = self.removed.iter_mut().flat_map(Removed::items_mut);
        // the copies of those removed a moment ago are kept while they may
        // be put back, as they come back with them
        let (items, held): (&[Item], Vec<String>) = if self.config.gui.reopen_files {
            (&self.items, removed.filter_map(|item| item.pasted_copy.clone()).collect())
        } else {
            self.items.iter_mut().chain(removed).for_each(|item| item.pasted_copy = None);
            (&[], Vec::new())
        };
        let session = session::Session::of(items, self.selected, &self.written);
        let session = session.with_window(self.window);
        if let Err(err) = store.save(&session, held) {
            let message = session_unsaved(store.path(), &err);
            // once is enough
            self.session = None;
            self.say(message);
        }
    }

    pub(crate) fn with(config: Config, config_path: Option<PathBuf>, presets: Presets) -> App {
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
            batch: 0,
            written: HashMap::new(),
            made_by_tgradish: Default::default(),
            probes: Default::default(),
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
            about: false,
            about_visit: 0,
            shortcuts: false,
            theme_shown: None,
            scale_shown: None,
            scaling: Default::default(),
            session: None,
            instance: None,
            attention: false,
            next_save: 0.0,
            fallbacks: fallback::Fallbacks::new(fonts()),
            fonts_checked: None,
            banner: Banner::default(),
            removed: None,
            edit_frames: Default::default(),
            drawn: None,
            typed: None,
            removing: Vec::new(),
            settings_reset: None,
            clipboard: system::Clipboard::default(),
            paste_key: crate::keys::PasteKey::default(),
            keyed: crate::keys::Keyed::default(),
            reveal_selected: false,
            #[cfg(test)]
            asked_to_pick: Vec::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            drops: None,
            #[cfg(all(unix, not(target_os = "macos")))]
            _toplevel_icon: None,
            hovering: false,
            preview_share: PREVIEW_SHARE,
            window: None,
            accent: None,
            applied_accent: None,
            system_motion: None,
            rows: crate::files::Rows::default(),
        }
    }

    /// Keeps the scale and its setting together: the setting is applied
    /// as the GUI starts, and as Settings changes it, which
    /// keeps it once saved, as it does the theme; a change by egui's keys,
    /// Ctrl++ (or Ctrl+=) and Ctrl+- and Ctrl+0, is saved at once, as the theme's button
    /// saves its change. egui applies a scale asked of it from the next
    /// frame, which `scale_shown`, what was last asked or seen, awaits.
    fn follow_scale(&mut self, ctx: &egui::Context) {
        let zoom = ctx.zoom_factor();
        let set = self.config.gui.scale as f32;
        let alike = |a: f32, b: f32| (a - b).abs() < 0.005;
        match self.scale_shown {
            // the setting changed, by Settings, or as the GUI starts
            Some(shown) if !alike(set, shown) => self.scaling.set(ctx, set),
            None => self.scaling.set(ctx, set),
            // what was asked, now applied
            Some(shown) if alike(zoom, shown) => return,
            // by the keys, Settings open or not
            Some(_) => {
                let scale = crate::config::rounded_scale(f64::from(zoom));
                self.save_setting(|gui| gui.scale = scale);
                self.scale_shown = Some(zoom);
                return;
            }
        }
        self.scale_shown = Some(set);
    }

    pub(crate) fn style(&self, ctx: &egui::Context) {
        ctx.set_fonts(fonts());
        set_theme(ctx, self.config.gui.theme);
        theme::apply(ctx, self.applied_accent);
        ctx.all_styles_mut(|style| {
            use egui::{FontFamily, FontId, TextStyle};
            // headings stand out by their weight more than their size
            style.text_styles = [
                (TextStyle::Heading, FontId::new(widgets::TITLE, crate::fonts::medium())),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            ]
            .into();
        });
    }

    /// Keeps how the window is laid out, for the session: its size and
    /// place while it is neither maximised nor full screen, and the width
    /// of the list, `list`, where it isn't narrowed by a small window.
    fn follow_window(&mut self, ctx: &egui::Context, list: Option<f32>) {
        let zoom = ctx.zoom_factor();
        let (inner, outer, maximized, other) = ctx.input(|input| {
            let viewport = input.viewport();
            let other = viewport.minimized == Some(true) || viewport.fullscreen == Some(true);
            (viewport.inner_rect, viewport.outer_rect, viewport.maximized == Some(true), other)
        });
        let window = self.window.get_or_insert(session::Window {
            size: [1280.0, 820.0],
            position: None,
            maximized: false,
            list: FILES_WIDTH,
            preview: PREVIEW_SHARE,
        });
        // in the system's points, whole, so it stays the same as the
        // window does
        let whole = |value: egui::Vec2| [(value.x * zoom).round(), (value.y * zoom).round()];
        if !maximized && !other {
            if let Some(inner) = inner {
                window.size = whole(inner.size());
            }
            window.position = outer.map(|outer| whole(outer.min.to_vec2()));
        }
        if !other {
            window.maximized = maximized;
        }
        if let Some(list) = list {
            window.list = list.round();
        }
        window.preview = self.preview_share;
    }

    /// Makes the look again when the accent chosen, or the
    /// system's, changed.
    fn follow_accent(&mut self, ctx: &egui::Context) {
        let system = || self.accent.as_ref().and_then(accent::Accent::get);
        let accent = self.config.gui.accent.colour().or_else(system);
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
            Err(err) => self.say(not_pasted(&err)),
        }
    }

    /// Adds a pasted image, the one file in `dir`, which is kept as long
    /// as the item is, and copied next to the session at once: `dir` goes
    /// when the GUI closes.
    pub(crate) fn add_pasted(&mut self, files: Vec<PathBuf>, dir: tempfile::TempDir) {
        let mut item =
            Item::new(self.next_id, files, false, self.config.gui.target.unwrap_or_default());
        item.paste_dir = Some(dir);
        let id = self.push(item);
        self.select(id);
        self.copy_pasted();
    }

    /// Takes the file `id` out of the list, which can be undone a while.
    pub(crate) fn remove(&mut self, id: u64) {
        self.remove_all(&[id]);
    }

    /// Takes the files `ids` out of the list, stopping their conversions;
    /// they are kept, with their settings and results, while a notice
    /// offers to put them back, with those removed just before.
    pub(crate) fn remove_all(&mut self, ids: &[u64]) {
        // a number typed in one of them taken first, in the next frame,
        // which shows that file a frame more for it
        if self.typed.is_some_and(|file| ids.contains(&file)) {
            self.removing.extend_from_slice(ids);
            return;
        }
        let selected = self.selected.filter(|id| ids.contains(id));
        let mut taken = Vec::new();
        let mut index = 0;
        while index < self.items.len() {
            if !ids.contains(&self.items[index].id) {
                index += 1;
                continue;
            }
            let mut item = self.items.remove(index);
            if let Some(conversion) = item.job.take() {
                conversion.job.cancel();
            }
            release(&mut item);
            taken.push((index + taken.len(), item));
        }
        if taken.is_empty() {
            return;
        }
        self.queue.retain(|id| !ids.contains(id));
        self.recent.retain(|id| !ids.contains(id));
        if selected.is_some() {
            self.selected = self.recent.front().copied().or(self.items.last().map(|item| item.id));
        }
        // with those removed a moment ago, which were put back before these
        let removed = self.removed.get_or_insert_with(Removed::default);
        removed.batches.push(taken);
        removed.selected = removed.selected.or(selected);
        let count = removed.batches.iter().map(Vec::len).sum::<usize>();
        let message = Message::info(fl!("files-removed", count = count));
        self.say(message.actions(&[Action::Undo]).about(Subject::Removed));
    }

    /// Puts the files removed a moment ago back where they were, with
    /// their settings and results.
    pub(crate) fn undo_removing(&mut self) {
        let Some(removed) = self.removed.take() else { return };
        self.banner.forget(Subject::Removed);
        // the latest out first back, each where it was among the rest
        for batch in removed.batches.into_iter().rev() {
            for (index, item) in batch {
                let index = index.min(self.items.len());
                self.items.insert(index, item);
            }
        }
        if let Some(id) = removed.selected {
            self.select(id);
        }
    }

    /// Ctrl+Z: puts back the files just removed while a notice offers
    /// to, else undoes the last change to the selected file's settings.
    pub(crate) fn undo(&mut self, ctx: &egui::Context) {
        if self.removed.is_some() {
            return self.undo_removing();
        }
        self.step_history(ctx, true);
    }

    /// Ctrl+Shift+Z: makes the last change undone to the selected file's
    /// settings again.
    pub(crate) fn redo(&mut self, ctx: &egui::Context) {
        self.step_history(ctx, false);
    }

    /// Whether [`App::undo`] has anything to do.
    pub(crate) fn can_undo(&self) -> bool {
        let selected = self.selected.and_then(|id| self.item(id));
        self.removed.is_some() || selected.is_some_and(|item| item.history.can_undo())
    }

    pub(crate) fn can_redo(&self) -> bool {
        let selected = self.selected.and_then(|id| self.item(id));
        selected.is_some_and(|item| item.history.can_redo())
    }

    fn step_history(&mut self, ctx: &egui::Context, back: bool) {
        // not halfway through a drag, whose step isn't over
        if ctx.input(|input| input.pointer.any_down()) {
            return;
        }
        let Some(item) = self.selected.and_then(|id| self.item_mut(id)) else { return };
        if crate::history::step(item, back) {
            ctx.request_repaint();
        }
    }

    /// Makes what the user changed of each file's settings so far a step,
    /// before the GUI changes them by itself, as a dialog changes
    /// presets: such a change is no step of theirs, but what the settings
    /// are from then on, which [`App::take_in_edits`] takes in after it.
    pub(crate) fn settle_edits(&mut self) {
        self.items.iter_mut().for_each(crate::history::flush);
    }

    pub(crate) fn take_in_edits(&mut self) {
        self.items.iter_mut().for_each(crate::history::take_in);
    }

    /// Takes in the changes to files' settings this frame made, which are
    /// steps once at rest; another frame is drawn while they aren't.
    fn follow_edits(&mut self, ctx: &egui::Context) {
        let frame = self.edit_frames.end(ctx, self.drawn);
        let selected = self.drawn;
        // only the file whose settings were drawn is changed by hand
        let other = crate::history::Frame { input: false, ..frame };
        let items = self.items.iter_mut().map(|item| match Some(item.id) == selected {
            true => (item, frame),
            false => (item, other),
        });
        if crate::history::follow(items) {
            ctx.request_repaint();
        }
    }

    /// Puts every setting of the Settings dialog back to its default, with
    /// a message that offers to undo it. Settings, which is open, keeps
    /// them as it keeps any change.
    pub(crate) fn reset_settings(&mut self) {
        let before = self.config.clone();
        self.take_dialog_settings(&Config::default());
        self.settings_dialog.forget_typed();
        self.settings_reset = Some(before);
        let message = Message::info(fl!("settings-reset"));
        self.say(message.actions(&[Action::UndoReset]).about(Subject::SettingsReset));
    }

    /// Puts back the settings as they were before they were reset, and
    /// keeps them, at once where Settings is closed by now.
    pub(crate) fn undo_reset(&mut self) {
        let Some(before) = self.settings_reset.take() else { return };
        self.banner.forget(Subject::SettingsReset);
        self.take_dialog_settings(&before);
        self.settings_dialog.forget_typed();
        if !self.settings_dialog.is_open() {
            self.save_config_setting(|config| dialog_settings(&before, config));
        }
    }

    /// Takes the settings the Settings dialog shows from `from`: the
    /// default presets as Settings picks them, so the files that follow
    /// them may fit another way, and the GUI's language as it changes.
    fn take_dialog_settings(&mut self, from: &Config) {
        use tgradish_core::presets::Format;
        for format in [Format::Webm, Format::Tgs] {
            let name = from.preset_for(format).to_owned();
            if name != self.config.preset_for(format) {
                self.set_default_preset(format, name);
            }
        }
        let language = self.config.gui.language.clone();
        dialog_settings(from, &mut self.config);
        if self.config.gui.language != language {
            crate::l10n::choose(self.config.gui.language.as_deref());
        }
    }

    /// Lets go of files removed once the offer to put them back is gone.
    pub(crate) fn forget_removed(&mut self) {
        let offered =
            self.banner.rows.iter().any(|said| said.message.subject == Some(Subject::Removed));
        if !offered {
            self.removed = None;
        }
    }

    /// Queues the item for converting with its settings as they are now.
    pub(crate) fn enqueue(&mut self, id: u64) {
        // not while something keeps it from being converted as it is
        let Some(item) = self.item(id).filter(|item| !item.blocked()) else { return };
        // as things are now, not as they were when the frame started, nor
        // as files were seen a moment ago
        self.probes.forget();
        let output = self.work_out_output(item);
        // counted from the first while nothing else converts
        if self.items.iter().all(|item| item.job.is_none()) {
            self.batch = 0;
        }
        let item = self.item_mut(id).expect("found above");
        let again = item.job.as_ref().inspect(|conversion| conversion.job.cancel()).is_some();
        item.job = Some(Conversion::of(Job::waiting(output), item.format, item.choices.clone()));
        if !again {
            self.batch += 1;
        }
        self.queue.retain(|&other| other != id);
        self.queue.push_back(id);
    }

    /// Stops every conversion, and takes what waits out of the queue.
    pub(crate) fn stop_all(&mut self) {
        for item in &mut self.items {
            let Some(conversion) = &item.job else { continue };
            conversion.job.cancel();
            // waiting, it is as if never asked for; running, it stops and
            // says so
            if conversion.job.is_waiting() {
                item.job = None;
            }
        }
        self.queue.clear();
    }

    /// Changes a setting of the GUI, and keeps that change in the config
    /// file at once, without the rest of what Settings changed.
    pub(crate) fn save_setting(&mut self, change: impl Fn(&mut crate::config::GuiConfig)) {
        self.save_config_setting(|config| change(&mut config.gui));
    }

    /// [`App::save_setting`] for any of the settings.
    pub(crate) fn save_config_setting(&mut self, change: impl Fn(&mut Config)) {
        change(&mut self.config);
        self.config.forget_overridden();
        let Some(path) = &self.config_path else { return };
        let saved = Config::load(path).and_then(|mut saved| {
            change(&mut saved);
            saved.save(path)
        });
        match saved {
            // whatever was said of the file is past
            Ok(()) => self.banner.forget(Subject::SettingsFile),
            Err(err @ tgradish_core::Error::Config { .. }) => {
                self.say(settings_unreadable(fl!("setting-unsaved", "unreadable"), &err));
            }
            Err(err) => self.say(told(Message::warning(fl!("setting-unsaved")), &err)),
        }
    }

    /// Whether a dialog is open over the rest.
    pub(crate) fn dialog_open(&self) -> bool {
        self.picking.is_some()
            || self.settings_dialog.is_open()
            || self.about
            || self.shortcuts
            || self.inspection.is_some()
            || self.preset_dialog.is_some()
    }
}

/// Files taken out of the list, each batch as it was removed, with where
/// each was, kept while they may be put back.
#[derive(Default)]
pub(crate) struct Removed {
    batches: Vec<Vec<(usize, Item)>>,
    /// The file selected, if it was removed.
    selected: Option<u64>,
}

impl Removed {
    pub(crate) fn items_mut(&mut self) -> impl Iterator<Item = &mut Item> {
        self.batches.iter_mut().flatten().map(|(_, item)| item)
    }
}

/// Copies the settings the Settings dialog shows from `from` to `to`.
fn dialog_settings(from: &Config, to: &mut Config) {
    (to.ffmpeg.choice, to.ffmpeg.path) = (from.ffmpeg.choice, from.ffmpeg.path.clone());
    to.threads = from.threads;
    (to.preset, to.tgs_preset) = (from.preset.clone(), from.tgs_preset.clone());
    let (gui, from) = (&mut to.gui, &from.gui);
    (gui.output_dir, gui.overwrite, gui.target) =
        (from.output_dir.clone(), from.overwrite, from.target);
    (gui.language, gui.theme, gui.accent) = (from.language.clone(), from.theme, from.accent);
    (gui.scale, gui.reopen_files, gui.motion) = (from.scale, from.reopen_files, from.motion);
}

/// The settings at `path`, or the defaults where they can't be read, with
/// why. Such a file is copied aside at once, while it is as it was.
fn load_config(path: &Path) -> (Config, Option<tgradish_core::Error>) {
    match Config::load(path) {
        Ok(config) => (config, None),
        Err(err) => {
            // whether it could be is told when it is written over
            let _ = keep_aside(path);
            (Config::default(), Some(err))
        }
    }
}

/// What to say of the settings file that can't be read, as `err` says,
/// `headline` first. Where it isn't settings, as when it was changed by
/// hand or by another version, it can be replaced; read again, once it is
/// put right; and found, by those who would put it right.
pub(crate) fn settings_unreadable(headline: String, err: &tgradish_core::Error) -> Message {
    let mut message = told(Message::warning(headline), err).about(Subject::SettingsFile);
    message.file = message.file.map(|file| std::path::absolute(&file).unwrap_or(file));
    match err {
        tgradish_core::Error::Config { .. } => message.note(fl!("settings-file-note")).actions(&[
            Action::FreshSettings,
            Action::ReadSettingsAgain,
            Action::CopyPath,
            Action::ShowInFolder,
        ]),
        _ => message.actions(&[Action::ReadSettingsAgain, Action::CopyPath, Action::ShowInFolder]),
    }
}

/// `message` with what `err` says: the file it is about, where it names
/// one; why, in the GUI's words; and what a program or the system said,
/// as it said it, as what is wrong in a settings file.
pub(crate) fn told(message: Message, err: &tgradish_core::Error) -> Message {
    use tgradish_core::Error;
    match err {
        Error::Config { path, message: said } | Error::Unreadable { path, message: said } => {
            message.file(path).literal(said)
        }
        Error::Io { path, error, .. } => message.file(path).literal(error.to_string()),
        Error::Ffmpeg { program, status, stderr } => {
            let program = program.to_string();
            let failed = fl!("program-failed", program = program, status = status.as_str());
            message.reason(failed).literal(stderr)
        }
        err => message.reason(failure(err)),
    }
}

/// What to say of the open files that couldn't be kept at `path` for
/// next time, as the system said why.
pub(crate) fn session_unsaved(path: &Path, err: &std::io::Error) -> Message {
    Message::warning(fl!("session-unsaved")).file(path).literal(err.to_string())
}

/// What to say when nothing was pasted, as `err` says why: the clipboard
/// held nothing to add, or it, or the image on it, couldn't be read.
pub(crate) fn not_pasted(err: &tgradish_core::Error) -> Message {
    use tgradish_core::{ClipboardError, Error};
    match err {
        Error::Clipboard(ClipboardError::Empty) => {
            Message::warning(fl!("nothing-to-paste")).reason(fl!("nothing-to-paste", "why"))
        }
        Error::Clipboard(ClipboardError::Unreadable(said)) => {
            Message::warning(fl!("paste-failed")).literal(said)
        }
        Error::Clipboard(ClipboardError::ImageUnsaved(said)) => {
            Message::warning(fl!("paste-failed", "image")).literal(said)
        }
        // making the image's file
        err => told(Message::warning(fl!("paste-failed", "image")), err),
    }
}

/// Copies the settings file at `path`, which can't be read, to
/// `config.toml.unreadable` beside it, before it is written over; gives
/// where. A copy kept earlier of something else is never written over:
/// this one goes to `config.toml.unreadable-2` and on, unless one of them
/// holds the same already.
pub(crate) fn keep_aside(path: &Path) -> tgradish_core::Result<PathBuf> {
    use std::io::{ErrorKind, Write};
    use tgradish_core::{Error, FileAction};
    let failed = |action: FileAction, path: &Path| {
        let path = path.to_path_buf();
        move |error| Error::Io { action, path, error }
    };
    let bytes = std::fs::read(path).map_err(failed(FileAction::Read, path))?;
    for number in 1..=999 {
        let extension = match number {
            1 => "toml.unreadable".to_owned(),
            _ => format!("toml.unreadable-{number}"),
        };
        let aside = path.with_extension(extension);
        match std::fs::read(&aside) {
            Ok(kept) if kept == bytes => return Ok(aside),
            Ok(_) => continue,
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(failed(FileAction::Read, &aside)(err)),
        }
        // not even a copy another instance made meanwhile is written over
        match std::fs::File::create_new(&aside) {
            Ok(mut file) => {
                file.write_all(&bytes).map_err(failed(FileAction::Write, &aside))?;
                return Ok(aside);
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(failed(FileAction::Create, &aside)(err)),
        }
    }
    let many = std::io::Error::other("too many copies of unreadable settings are kept");
    Err(failed(FileAction::Create, &path.with_extension("toml.unreadable"))(many))
}

/// How long the GUI waits as it closes for conversions to stop, so they take
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

/// Makes the GUI light or dark.
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
        // the window is let go of next
        if let Some(instance) = &self.instance {
            instance.detach();
        }
        self.save_session();
        self.stop_converting();
        // the listener shares winit's Wayland connection, which closes next
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(drops) = &mut self.drops {
            drops.stop();
        }
    }

    // before every frame, and while the window is minimised and draws none
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.take_handed(ctx);
    }

    /// Says the frame's input at the scale egui applies to it, with the
    /// pointer where it is on the screen as the scale changes.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.scaling.follow(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.scaling.begun();
        // file dialogs stay over the window, which they are modal for
        self.file_dialog = rfd::FileDialog::new().set_parent(frame);
        self.take_picked();
        self.outputs.borrow_mut().clear();
        // converting, or done this frame
        let working = self.items.iter().any(|item| item.job.is_some());
        self.probes.follow(&ctx, working);
        self.follow_accent(&ctx);
        let less = self.system_motion.as_ref().is_some_and(crate::motion::System::reduced);
        let motion = self.config.gui.motion && !less;
        crate::motion::set(&ctx, motion);
        if !motion {
            direct_scrolling(&ctx);
        }
        // a number being typed is taken as the window closes, as its field
        // is drawn this last frame, so that it is saved with the rest
        if ctx.input(|input| input.viewport().close_requested()) {
            widgets::finish_typed_number(&ctx);
        }
        self.typed = widgets::typed_number(&ctx);
        if !self.removing.is_empty() && self.typed.is_none_or(|file| !self.removing.contains(&file))
        {
            let removing = std::mem::take(&mut self.removing);
            self.remove_all(&removing);
        }
        self.take_input(&ctx);
        self.pump(&ctx);
        self.find_fonts(&ctx);
        // as wide as half the window, leaving the rest the room it needs
        let window = ctx.content_rect().width();
        let widest = (window * 0.5).min(window - crate::preview::NARROWEST_MAIN).max(260.0);
        // apart from the width it has in a small window, or that would stay
        // once the window grows again
        let small = widest < FILES_WIDTH;
        let id = if small { "files-small" } else { "files" };
        // as wide as last time
        let width = match self.window {
            Some(window) if !small => window.list,
            _ => FILES_WIDTH,
        };
        // without files, the GUI is where they are dropped, until one is
        // added; the list is then as wide as it was
        if self.items.is_empty() {
            self.follow_window(&ctx, None);
            let frame = egui::Frame::side_top_panel(&ctx.global_style());
            let width = egui::PanelState::load(&ctx, egui::Id::new(id))
                .map_or(width, |state| state.size().x)
                .clamp(NARROWEST_LIST, widest)
                - frame.inner_margin.sum().x;
            // its icon where the list's is
            egui::CentralPanel::default().frame(frame).show(ui, |ui| self.welcome(ui, width));
        } else {
            let list = egui::Panel::left(id)
                .resizable(true)
                .default_size(width)
                .size_range(NARROWEST_LIST..=widest)
                .show(ui, |ui| {
                    self.files_panel(ui);
                });
            self.follow_window(&ctx, (!small).then(|| list.response.rect.width()));
            egui::CentralPanel::default().show(ui, |ui| self.main_panel(ui));
        }
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
        if std::mem::take(&mut self.settings_dialog.reset) {
            self.reset_settings();
        }
        if std::mem::take(&mut self.settings_dialog.wrote) {
            self.banner.forget(Subject::SettingsFile);
        }
        if let Some((format, name)) = self.settings_dialog.default_preset.take() {
            self.set_default_preset(format, name);
        }
        // as Settings changes them, without saving
        self.config.forget_overridden();
        // as Settings or the theme button changes it
        if self.theme_shown != Some(self.config.gui.theme) {
            set_theme(&ctx, self.config.gui.theme);
            self.theme_shown = Some(self.config.gui.theme);
        }
        self.follow_scale(&ctx);
        self.inspection_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        self.preset_dialogs(&ctx);
        if self.about {
            let modal = widgets::dialog(&ctx, "about").show(&ctx, |ui| {
                ui.set_width(crate::about::WIDTH);
                let crossed =
                    widgets::dialog_title(ui, widgets::title_text(ui, fl!("about-title")));
                // afresh each time it opens
                ui.push_id(self.about_visit, about);
                let closed = widgets::dialog_buttons(ui, |ui| {
                    ui.add(widgets::dialog_button(fl!("close"))).clicked()
                });
                crossed || closed
            });
            if modal.inner || modal.should_close() {
                self.about = false;
                self.about_visit += 1;
            }
        }
        for (message, action) in self.banner.show(&ctx) {
            self.act(&ctx, &message, action);
        }
        self.forget_removed();
        if !self.banner.rows.iter().any(|said| said.message.subject == Some(Subject::SettingsReset))
        {
            self.settings_reset = None;
        }
        if let Some(action) = crate::banner::asked(&ctx) {
            self.act(&ctx, &Message::info(""), action);
        }
        theme::focus_ring(&ctx);
        // files dropped are shown where they go: on the list, or the drop
        // zone without one, never over the preview
        if self.hovering {
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
        self.follow_edits(&ctx);
        // last, as egui would take them
        self.scaling.keys(&ctx);
        // every few seconds, so a crash loses little; what changed is kept
        // even if nothing is drawn again before then
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
    use crate::testing::{convert, finish, harness, has_ffmpeg, pasted_image, square};

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
            let said = crate::testing::said(app).contains("kept for next time");
            app.banner.rows.clear();
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
        // turned off, and on again before the GUI closes
        first.state_mut().config.gui.reopen_files = false;
        first.state_mut().save_session();
        first.state_mut().config.gui.reopen_files = true;
        first.state_mut().save_session();
        drop(first);

        let mut second = harness(Config::default());
        second.state_mut().reopen(state);
        assert_eq!(second.state().items.len(), 1);
        assert!(second.state().banner.rows.is_empty(), "{}", crate::testing::said(second.state()));
    }

    #[test]
    fn keeps_the_copy_of_a_pasted_image_while_it_may_be_put_back() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let copies = state.with_file_name("pasted");
        let mut first = harness(Config::default());
        first.state_mut().reopen(state.clone());
        let (files, folder) = pasted_image();
        first.state_mut().add_pasted(files, folder);
        first.state_mut().save_session();
        let id = first.state().items[0].id;
        first.state_mut().remove(id);
        first.state_mut().save_session();
        assert_eq!(std::fs::read_dir(&copies).unwrap().count(), 1);
        first.state_mut().undo_removing();
        first.state_mut().save_session();
        drop(first);

        let mut second = harness(Config::default());
        second.state_mut().reopen(state.clone());
        assert_eq!(second.state().items.len(), 1);
        assert!(second.state().banner.rows.is_empty(), "{}", crate::testing::said(second.state()));

        // and lets go of it once it can't be
        let id = second.state().items[0].id;
        second.state_mut().remove(id);
        second.state_mut().save_session();
        assert_eq!(std::fs::read_dir(&copies).unwrap().count(), 1);
        second.state_mut().banner.rows.clear();
        second.state_mut().forget_removed();
        second.state_mut().save_session();
        assert_eq!(std::fs::read_dir(&copies).unwrap().count(), 0);
    }

    #[test]
    fn leaves_the_session_to_the_instance_that_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let mut first = harness(Config::default());
        first.state_mut().reopen(state.clone());
        let (files, folder) = pasted_image();
        first.state_mut().add_pasted(files, folder);
        first.state_mut().save_session();
        let written = std::fs::read_to_string(&state).unwrap();

        // a second instance meanwhile, whose own files are kept nowhere
        let mut second = harness(Config::default());
        second.state_mut().reopen(state.clone());
        assert!(second.state().session.is_none() && second.state().items.is_empty());
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        second.state_mut().add(vec![art]);
        second.state_mut().save_session();
        assert!(second.state().banner.rows.is_empty());
        assert_eq!(std::fs::read_to_string(&state).unwrap(), written);
        assert_eq!(std::fs::read_dir(state.with_file_name("pasted")).unwrap().count(), 1);

        // the next one, once the first has closed, has its files
        drop(first);
        let mut third = harness(Config::default());
        third.state_mut().reopen(state);
        assert_eq!(third.state().items.len(), 1);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn adds_the_files_a_gui_started_later_hands_over_as_dropped_ones() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let instance::Start::First { store, listener: Some(listener) } =
            instance::start(state.clone(), &instance::Request::default())
        else {
            panic!("not the first, listening");
        };
        let mut first = harness(Config::default());
        listener.attach(&first.ctx);
        first.state_mut().resume(store);
        first.state_mut().instance = Some(listener);
        // behind a dialog, as a drop is
        first.state_mut().about = true;
        first.run_steps(2);

        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let frames = dir.path().join("frames");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [0, 0, 255, 255]);
        let request =
            instance::Request { files: vec![art.clone(), frames.clone()], ..Default::default() };
        assert!(matches!(instance::start(state, &request), instance::Start::HandedOver));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while first.state().items.is_empty() && std::time::Instant::now() < deadline {
            first.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let app = first.state();
        let added: Vec<_> =
            app.items.iter().map(|item| (item.inputs.clone(), item.sequence)).collect();
        assert_eq!(added, [(vec![art], false), (vec![frames], true)]);
        assert_eq!(app.selected, Some(app.items[1].id));
        assert!(app.about);
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

        // the next run names the result the same, and knows the file
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
        if !has_ffmpeg() {
            return;
        }
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
        second.get_by_label_contains(&fl!("telegram-accepts"));
        // and shown
        assert!(item.result_clip.ready().is_some() && item.result_thumb.is_some());
    }

    #[test]
    fn shows_a_tgs_result_of_last_time_as_it_was_made() {
        use crate::jobs::Status;
        let dir = tempfile::tempdir().unwrap();
        let art = crate::testing::corpus(dir.path(), "walker_2x.gif", "walker.gif");
        let state = dir.path().join("state").join("session.json");
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut first = harness(config.clone());
        first.state_mut().reopen(state.clone());
        first.state_mut().add(vec![art]);
        finish(&mut first);
        let id = first.state().items[0].id;
        first.state_mut().select(id);
        assert_eq!(first.state().items[0].format, Format::Tgs);
        convert(&first);
        first.run_steps(2);
        finish(&mut first);
        let shown = |item: &Item| {
            let clip = item.result_clip.ready()?;
            Some((clip.width, clip.height, clip.frames.clone(), clip.times.clone()))
        };
        let item = &first.state().items[0];
        let fresh = shown(item).expect("drawn as it was made");
        let output = crate::testing::made_job(item).output.clone();
        // its frames are kept once, where they show, and its art no more
        let Some(Status::Done(done)) = item.made.as_ref().map(|made| &made.job.status) else {
            panic!("not made");
        };
        assert!(done.drawing.is_none());
        // let go of, as when other files are looked at, and read again
        crate::work::release(&mut first.state_mut().items[0]);
        finish(&mut first);
        assert_eq!(shown(&first.state().items[0]).as_ref(), Some(&fresh));
        // and taken out of the list and put back
        first.state_mut().remove(id);
        first.state_mut().undo_removing();
        finish(&mut first);
        assert_eq!(shown(&first.state().items[0]).as_ref(), Some(&fresh));
        first.state_mut().save_session();
        drop(first);

        // and the next time the GUI starts, drawn from the file
        let mut second = harness(config);
        second.state_mut().reopen(state);
        finish(&mut second);
        let item = &second.state().items[0];
        assert!(item.result_is_current() && item.result_thumb.is_some());
        assert_eq!(shown(item), Some(fresh));
        // replaced by a sticker tgradish can't draw, a circle, which it says
        let circle = concat!(
            r#"{"v":"5.7.2","fr":60,"ip":0,"op":60,"w":512,"h":512,"layers":[{"ty":4,"ks":{},"#,
            r#""ip":0,"op":60,"shapes":[{"ty":"gr","it":[{"ty":"el","p":{"k":[256,256]},"#,
            r#""s":{"k":[200,200]}},{"ty":"fl","c":{"k":[1,0,0]}},{"ty":"tr"}]}]}]}"#,
        );
        std::fs::write(&output, circle).unwrap();
        let id = second.state().items[0].id;
        second.state_mut().select(id);
        crate::work::release(&mut second.state_mut().items[0]);
        finish(&mut second);
        let item = &second.state().items[0];
        let unseen = fl!("tgs-unseen");
        assert!(matches!(&item.result_clip, crate::media::Load::Failed(why) if *why == unseen));
    }

    #[test]
    fn stops_converting_on_exit() {
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
        let mut app = App::with(Config::default(), Some(path.clone()), Presets::builtin());
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
        assert!(unreadable.is_some());
        assert_eq!(config, Config::default());
        let kept = dir.path().join("config.toml.unreadable");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), "made-up = 1\n");
        // a file that is fine is left alone
        std::fs::write(&path, "").unwrap();
        assert!(load_config(&path).1.is_none());
    }

    #[test]
    fn never_writes_over_a_copy_of_settings_kept_before() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let aside = |name: &str| dir.path().join(name);
        std::fs::write(&path, "first = 1\n").unwrap();
        assert_eq!(keep_aside(&path).unwrap(), aside("config.toml.unreadable"));
        // the same again is kept once; something else beside it
        assert_eq!(keep_aside(&path).unwrap(), aside("config.toml.unreadable"));
        std::fs::write(&path, "second = 2\n").unwrap();
        assert_eq!(keep_aside(&path).unwrap(), aside("config.toml.unreadable-2"));
        let read = |name: &str| std::fs::read_to_string(aside(name)).unwrap();
        assert_eq!(read("config.toml.unreadable"), "first = 1\n");
        assert_eq!(read("config.toml.unreadable-2"), "second = 2\n");
    }

    #[cfg(unix)]
    #[test]
    fn says_everything_it_cannot_read_as_it_opens() {
        let dir = tempfile::tempdir().unwrap();
        let Some(presets) = crate::testing::unlistable(dir.path()) else { return };
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "made-up = 1\n").unwrap();
        let mut app = App::with(Config::default(), Some(path.clone()), Presets::builtin());
        app.start(load_config(&path).1, Some(presets.clone()));
        let said = crate::testing::said(&app);
        for saying in ["settings file can't be read", "config.toml", "presets couldn't"] {
            assert!(said.contains(saying), "{saying} in {said}");
        }
        crate::testing::listable(&presets);
    }

    #[test]
    fn says_the_session_is_unsaved_under_what_it_said_before() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let mut app = App::with(Config::default(), None, Presets::builtin());
        app.reopen(state.join("session.json"));
        // a folder in the session's place, so saving it fails; not a file
        // in place of its folder, which can't be removed everywhere while
        // the session's lock in it is open
        std::fs::create_dir(state.join("session.json")).unwrap();
        app.say(Message::info("Something said before"));
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        app.add(vec![art]);
        app.save_session();
        let said = crate::testing::said(&app);
        assert!(said.starts_with("Something said before\n"), "{said}");
        assert!(said.contains("kept for next time"), "{said}");
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
        harness.get_by_label(&fl!("session-unreadable"));
        // written anew by this run, the other kept beside it
        harness.state_mut().save_session();
        assert!(std::fs::read_to_string(&aside).unwrap().contains("a later tgradish"));
        assert!(std::fs::read_to_string(&state).unwrap().contains(r#""version":1"#));

        // told to forget the files, the next run reads none and keeps none
        drop(harness);
        std::fs::write(&state, "anything").unwrap();
        let mut config = Config::default();
        config.gui.reopen_files = false;
        let mut harness = self::harness(config);
        harness.state_mut().reopen(state.clone());
        harness.state_mut().save_session();
        assert!(harness.state().banner.rows.is_empty());
        assert!(std::fs::read_to_string(&state).unwrap().contains(r#""items":[]"#));
        assert!(std::fs::read_to_string(&aside).unwrap().contains("a later tgradish"));
    }

    #[test]
    fn lays_the_window_out_as_last_time() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state").join("session.json");
        let mut first = harness(Config::default());
        first.state_mut().reopen(state.clone());
        first.state_mut().preview_share = 0.4;
        // the list, which shows with files
        let art = dir.path().join("art.png");
        crate::testing::square(&art, [255, 0, 0, 255]);
        first.state_mut().add(vec![art]);
        first.run_steps(2);
        // the list made wider, by its edge
        let edge = first.state().window.unwrap().list;
        let drag = |pos| egui::Event::PointerMoved(pos);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let (from, to) = (egui::pos2(edge, 300.0), egui::pos2(edge + 60.0, 300.0));
        for event in [drag(from), press(from, true), drag(to), press(to, false)] {
            first.event(event);
            first.step();
        }
        first.run_steps(2);
        let window = first.state().window.unwrap();
        assert!((window.list - (edge + 60.0)).abs() <= 2.0, "{} from {edge}", window.list);
        first.state_mut().save_session();
        drop(first);

        // as it was, whatever the setting of opening files again
        let mut config = Config::default();
        config.gui.reopen_files = false;
        // read before the first frame is drawn, as the GUI starts
        let session = state.clone();
        let mut second =
            egui_kittest::Harness::builder().with_size([1280.0, 820.0]).build_eframe(move |_| {
                let mut app = App::with(config.clone(), None, Presets::builtin());
                app.reopen(session.clone());
                app
            });
        second.run_steps(2);
        assert_eq!(second.state().preview_share, 0.4);
        assert_eq!(second.state().window.unwrap().list, window.list);
        assert_eq!(session::window(&state).map(|window| window.list), Some(window.list));
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

        // a file goes before the next run
        std::fs::remove_file(&gone).unwrap();
        let mut second = harness(config);
        second.state_mut().reopen(state);
        // its result is read as frames are drawn, not as the GUI starts
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
        assert!(crate::testing::said(app).contains("1 file is gone"));
    }
}

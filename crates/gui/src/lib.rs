//! The tgradish window: a list of files, each with its own settings, a
//! preview to crop and trim it, and its result. It converts with
//! `tgradish-core` directly, like the CLI.

mod canvas;
#[cfg(all(unix, not(target_os = "macos")))]
mod dnd;
mod fallback;
mod inspect;
mod item;
mod jobs;
mod media;
mod output;
mod prefs;
mod settings;
mod timeline;
mod widgets;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText, vec2};
use tgradish_core::backend::Backend;
use tgradish_core::clipboard::Pasted;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{Resize, Scaling};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::{convert, paths, telegram, tgs};

use crate::canvas::{Cropping, Inset, Picture, Screen};
use crate::item::{Framing, Item, Kind, Made, Show, Zoom};
use crate::jobs::{Job, Plan, Status};
use crate::media::{Art, Clip, Load, Video};

/// How the window starts, from the command line.
#[derive(Debug, Default)]
pub struct Launch {
    /// The `config.toml` to read and save; the user's own when `None`.
    pub config_path: Option<PathBuf>,
    /// Which ffmpeg to use instead of the config's.
    pub ffmpeg_choice: Option<FfmpegChoice>,
    /// An ffmpeg executable or directory to use instead of the config's.
    pub ffmpeg_path: Option<PathBuf>,
}

/// Opens the window and runs until it is closed.
pub fn run(launch: Launch) -> eframe::Result<()> {
    tgradish_core::mark::set_client(tgradish_core::mark::Client::Window);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tgradish")
            .with_app_id("tgradish")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "tgradish",
        options,
        Box::new(|cc| {
            let app = App::new(launch);
            app.style(&cc.egui_ctx);
            #[cfg(all(unix, not(target_os = "macos")))]
            let app = App { drops: dnd::Drops::start(cc, &cc.egui_ctx), ..app };
            Ok(Box::new(app))
        }),
    )
}

/// The window's fonts: Ubuntu Light for text, JetBrains Mono NL for
/// commands, and a few symbols; small subsets, see `assets/fonts`.
fn fonts() -> egui::FontDefinitions {
    use egui::{FontData, FontFamily};
    let mut fonts = egui::FontDefinitions::empty();
    for (name, bytes) in [
        ("ubuntu", &include_bytes!("../assets/fonts/Ubuntu-Light-tgradish.ttf")[..]),
        ("mono", &include_bytes!("../assets/fonts/JetBrainsMonoNL-Regular-tgradish.ttf")[..]),
        ("icons", &include_bytes!("../assets/fonts/emoji-icon-font-tgradish.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), std::sync::Arc::new(FontData::from_static(bytes)));
    }
    // each falls back on the others for what it lacks
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["ubuntu".into(), "icons".into(), "mono".into()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["mono".into(), "icons".into(), "ubuntu".into()]);
    fonts
}

/// The licence notices of this build: tgradish's, its fonts', and those
/// [`tgradish_core::licenses`] has.
pub fn notices() -> Vec<tgradish_core::licenses::Notice> {
    use tgradish_core::licenses::Notice;
    let fonts = [
        ("Font: Ubuntu Light, a subset", include_str!("../assets/fonts/Ubuntu-LICENCE.txt")),
        (
            "Font: JetBrains Mono NL, a subset",
            include_str!("../assets/fonts/JetBrainsMono-OFL.txt"),
        ),
        (
            "Font: emoji-icon-font, a subset",
            include_str!("../assets/fonts/emoji-icon-font-LICENSE.txt"),
        ),
    ];
    let mut notices = tgradish_core::licenses::notices();
    // before the long list of crates
    let at =
        notices.iter().position(|notice| notice.title == "Rust crates").unwrap_or(notices.len());
    for (offset, (title, text)) in fonts.into_iter().enumerate() {
        notices.insert(at + offset, Notice { title: title.into(), text: text.into() });
    }
    notices
}

/// Shows an error in a dialog, for when there is no terminal to print to.
pub fn show_error(message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("tgradish")
        .set_description(message)
        .show();
}

/// A result this window wrote: for which item, and the file as written,
/// so a file put there since isn't taken for it.
struct Written {
    item: u64,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl Written {
    fn of(item: u64, path: &Path) -> Option<Written> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Written { item, len: metadata.len(), modified: metadata.modified().ok() })
    }
}

/// A file's size and modification time, which change when it is written.
type FileVersion = (u64, Option<std::time::SystemTime>);

/// How many items keep their big frames: the selected one and the last few.
const KEPT: usize = 3;
/// How many items read their inputs at once, besides the selected one.
const READING: usize = 2;
/// Side of the thumbnails in the list.
const THUMB: f32 = 52.0;

struct App {
    config_path: Option<PathBuf>,
    config: Config,
    presets: Presets,
    items: Vec<Item>,
    next_id: u64,
    selected: Option<u64>,
    /// Items selected lately, the latest first, which keep their frames.
    recent: VecDeque<u64>,
    /// Items waiting to be converted, in order.
    queue: VecDeque<u64>,
    /// Results this window wrote, which it replaces when making them again.
    written: HashMap<PathBuf, Written>,
    /// Files found in the way, by their size and time: whether tgradish
    /// made them, so they may be replaced.
    marked: std::cell::RefCell<HashMap<PathBuf, (FileVersion, bool)>>,
    screen: Screen,
    inset: Inset,
    prefs: prefs::Prefs,
    inspection: Option<inspect::Inspection>,
    /// The About and licenses window is open.
    about: bool,
    /// The system's fonts for what the window's own lack.
    fallbacks: fallback::Fallbacks,
    message: Option<String>,
    /// The clipboard was read for the Ctrl+V being held.
    pasted: bool,
    #[cfg(all(unix, not(target_os = "macos")))]
    drops: Option<dnd::Drops>,
    /// Files are being dragged over the window, as Wayland reports it.
    hovering: bool,
    /// Share of the main panel's height the preview takes.
    preview_share: f32,
}

impl App {
    fn new(launch: Launch) -> App {
        let config_path = launch.config_path.or_else(Config::default_path);
        let mut message = None;
        let mut config = match config_path.as_deref().map(Config::load) {
            Some(Ok(config)) => config,
            Some(Err(err)) => {
                message =
                    Some(format!("The settings couldn't be read, so defaults are used: {err}"));
                Config::default()
            }
            None => Config::default(),
        };
        // as on the command line: a path beats a choice
        if let Some(choice) = launch.ffmpeg_choice {
            config.ffmpeg.choice = choice;
            config.ffmpeg.path = None;
        }
        if let Some(path) = launch.ffmpeg_path {
            config.ffmpeg.path = Some(path);
        }
        let presets = Presets::load_user().unwrap_or_else(|err| {
            message = Some(format!("Your presets couldn't be read: {err}"));
            Presets::builtin()
        });
        App::with(config, config_path, presets, message)
    }

    fn with(
        config: Config,
        config_path: Option<PathBuf>,
        presets: Presets,
        message: Option<String>,
    ) -> App {
        App {
            config_path,
            config,
            presets,
            items: Vec::new(),
            next_id: 0,
            selected: None,
            recent: VecDeque::new(),
            queue: VecDeque::new(),
            written: HashMap::new(),
            marked: Default::default(),
            screen: Screen::default(),
            inset: Inset::default(),
            prefs: prefs::Prefs::default(),
            inspection: None,
            about: false,
            fallbacks: fallback::Fallbacks::new(fonts()),
            message,
            pasted: false,
            #[cfg(all(unix, not(target_os = "macos")))]
            drops: None,
            hovering: false,
            preview_share: 0.56,
        }
    }

    fn style(&self, ctx: &egui::Context) {
        ctx.set_fonts(fonts());
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
            style.spacing.item_spacing = vec2(8.0, 6.0);
            style.spacing.button_padding = vec2(9.0, 4.0);
            style.spacing.interact_size.y = 24.0;
        });
    }

    fn backend(&self) -> Result<Backend, String> {
        let ffmpeg = &self.config.ffmpeg;
        Backend::select(ffmpeg.choice, ffmpeg.path.as_deref()).map_err(|err| err.to_string())
    }

    fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    fn item_mut(&mut self, id: u64) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    fn select(&mut self, id: u64) {
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
    fn add(&mut self, paths: Vec<PathBuf>) {
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
    fn add_frames(&mut self, files: Vec<PathBuf>) {
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
    fn paste(&mut self) {
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
            Err(err) => self.message = Some(err.to_string()),
        }
    }

    fn remove(&mut self, id: u64) {
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

    /// Where an item's result goes: next to its input or in the folder
    /// for results, numbered if another file in the window has the name.
    fn output_for(&self, item: &Item) -> PathBuf {
        let dir = self.config.gui.output_dir.clone().filter(|dir| !dir.as_os_str().is_empty());
        let input = &item.inputs[0];
        let next_to_input = match item.format {
            Format::Webm => convert::default_output(input, item.choices.target),
            Format::Tgs => tgs::default_output(input, item.choices.target),
        };
        let name = next_to_input.file_name().unwrap_or_default();
        let first = match (&item.pasted, dir) {
            // not next to the image, which is in a temporary directory
            (Some(_), dir) => dir.or_else(paths::pictures_dir).unwrap_or_default().join(name),
            (None, Some(dir)) => dir.join(name),
            (None, None) => next_to_input.clone(),
        };
        let claimed: HashSet<&Path> = self
            .items
            .iter()
            .filter(|other| other.id != item.id)
            .flat_map(|other| {
                let waiting = other.job.as_ref().map(|(job, ..)| job.output.as_path());
                waiting.into_iter().chain(other.made.as_ref().map(|made| made.job.output.as_path()))
            })
            .collect();
        let suffix = format!(".{}.{}", item.choices.target.name(), item.format.extension());
        let own = item.made.as_ref().map(|made| made.job.output.as_path());
        // pasted images all have one name: theirs never replace a file
        let blocked = |path: &Path| {
            path.exists() && (item.pasted.is_some() || !self.may_replace(item.id, path))
        };
        free_name(&first, &suffix, &claimed, own, &blocked)
    }

    /// Whether the item may replace the file at `path`: one it wrote and
    /// nobody changed since, one tgradish made, or any with the setting
    /// that allows it.
    fn may_replace(&self, item: u64, path: &Path) -> bool {
        self.config.gui.overwrite || self.wrote(item, path) || self.made_here(path)
    }

    fn wrote(&self, item: u64, path: &Path) -> bool {
        self.written.get(path).is_some_and(|written| {
            Written::of(item, path).is_some_and(|now| {
                now.item == written.item
                    && now.len == written.len
                    && now.modified == written.modified
            })
        })
    }

    /// Whether tgradish made the file at `path` for this user, by its hidden
    /// mark: read once for each version of the file.
    fn made_here(&self, path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else { return false };
        let version = (metadata.len(), metadata.modified().ok());
        let mut marked = self.marked.borrow_mut();
        match marked.get(path) {
            Some(&(seen, made)) if seen == version => made,
            _ => {
                let made = tgradish_core::mark::made_here(path);
                marked.insert(path.to_path_buf(), (version, made));
                made
            }
        }
    }

    /// Queues the item for converting with its settings as they are now.
    fn enqueue(&mut self, id: u64) {
        let Some(item) = self.item(id) else { return };
        let output = self.output_for(item);
        let item = self.item_mut(id).expect("found above");
        if let Some((job, ..)) = &item.job {
            job.cancel();
        }
        item.job = Some((Job::waiting(output), item.format, item.choices.clone()));
        self.queue.retain(|&other| other != id);
        self.queue.push_back(id);
    }

    /// Takes in loads and conversions, and starts what is due.
    fn pump(&mut self, ctx: &egui::Context) {
        let backend = self.backend();
        let selected = self.selected;
        let mut reading = self
            .items
            .iter()
            .filter(|item| item.video.is_loading() || item.art.is_loading())
            .count();
        for item in &mut self.items {
            let is_selected = selected == Some(item.id);
            let kept = is_selected || self.recent.contains(&item.id);
            let finished = item.video.poll() | item.art.poll();
            if let Some((_, part)) = &mut item.part {
                part.poll();
            }
            if let Some((_, still)) = &mut item.still {
                still.poll();
            }
            item.result_clip.poll();
            if finished {
                settle(ctx, item);
                if !kept {
                    release(item);
                }
            }
            if item.result_thumb.is_none()
                && let Some(clip) = item.result_clip.ready()
            {
                item.result_thumb = Some(ctx.load_texture(
                    format!("result-{}", item.id),
                    clip.thumbnail(THUMB as u32 * 2),
                    texture_options(clip),
                ));
                if !kept {
                    item.result_clip = Load::Idle;
                }
            }
            // read what the item needs
            let wanted = is_selected || item.input_thumb.is_none();
            if !wanted || (!is_selected && reading >= READING) {
                continue;
            }
            let reads_art =
                item.format == Format::Tgs || (item.format_guessed && item.kind == Kind::Image);
            let read_with = item.choices.reading(&self.presets, &self.config);
            if reads_art
                && (item.art.is_idle() || (item.art_reading != read_with && !item.art.is_loading()))
            {
                item.art_reading = read_with;
                item.art = Load::Loading(Art::load(
                    ctx,
                    item.inputs.clone(),
                    item.sequence,
                    item.art_reading.clone(),
                ));
                reading += 1;
            } else if item.format == Format::Webm
                && item.video.is_idle()
                && !(item.format_guessed && item.art.is_loading())
            {
                item.video = match &backend {
                    Ok(backend) => {
                        Load::Loading(Video::load(ctx, backend.clone(), item.inputs[0].clone()))
                    }
                    Err(message) => Load::Failed(message.clone()),
                };
                reading += 1;
            }
            if is_selected && item.result_clip.is_idle() && item.result_thumb.is_some() {
                load_result_clip(ctx, item, &backend);
            }
        }
        self.load_detail(ctx, &backend);
        self.convert_next(ctx, &backend);
    }

    /// Reads more of the selected video once what is looked at stays put
    /// for a moment: the used part at full rate, and the frame shown while
    /// paused at full size.
    fn load_detail(&mut self, ctx: &egui::Context, backend: &Result<Backend, String>) {
        let Some(id) = self.selected else { return };
        let Ok(backend) = backend else { return };
        let Some(item) = self.item_mut(id) else { return };
        if item.format != Format::Webm {
            return;
        }
        let range = item.range().map(|(start, end)| (start, end - start));
        let Some(video) = item.video.ready() else { return };
        let path = item.inputs[0].clone();
        let still = video.probe.still_image;
        if !item.view.playing || still {
            let time = if still { 0.0 } else { item.view.time };
            let loaded = item.still.as_ref().is_some_and(|(at, _)| *at == time);
            if !loaded && settled(ctx, ("still", id), (time, 0.0), 0.25) {
                let task = video.load_still(ctx, backend.clone(), path.clone(), time);
                item.still = Some((time, Load::Loading(task)));
            }
        } else {
            item.still = None;
        }
        let Some(range) = range else { return };
        if !item.part.as_ref().is_some_and(|(loaded, _)| *loaded == range)
            && settled(ctx, ("range", id), range, 0.4)
        {
            let task = video.load_part(ctx, backend.clone(), path, range);
            item.part = Some((range, Load::Loading(task)));
        }
    }

    fn convert_next(&mut self, ctx: &egui::Context, backend: &Result<Backend, String>) {
        // finished jobs become results
        let mut finished = Vec::new();
        for item in &mut self.items {
            if let Some((job, ..)) = &mut item.job
                && (job.poll() || job.is_finished())
                && !job.is_waiting()
            {
                finished.push(item.id);
            }
        }
        for id in finished {
            let selected = self.selected == Some(id);
            let item = self.item_mut(id).expect("just found");
            let (job, format, choices) = item.job.take().expect("just found");
            let made = Made { job, format, choices };
            let Status::Done(done) = &made.job.status else {
                item.failed = Some(made);
                continue;
            };
            let output = done.output.clone();
            item.result_thumb = None;
            item.result_clip = match &done.preview {
                Some(preview) => Load::Ready(Clip::from_preview(preview.clone(), true)),
                None => Load::Idle,
            };
            if selected {
                item.view.show = Show::Result;
            }
            item.made = Some(made);
            item.failed = None;
            if item.result_clip.is_idle() {
                load_result_clip(ctx, item, backend);
            }
            if let Some(written) = Written::of(id, &output) {
                self.written.insert(output, written);
            }
        }
        if self.items.iter().any(|item| item.job.as_ref().is_some_and(|(job, ..)| job.is_running()))
        {
            return;
        }
        while let Some(id) = self.queue.pop_front() {
            let Some(item) = self.item(id) else { continue };
            let Some((job, format, choices)) = &item.job else { continue };
            let plan = match format {
                Format::Webm => {
                    choices.webm_options(&self.presets, &self.config).and_then(|options| {
                        backend.clone().map(|backend| Plan::Webm { options, backend })
                    })
                }
                Format::Tgs => choices
                    .tgs_options(&self.presets, &self.config)
                    .map(|options| Plan::Tgs { options }),
            };
            let overwrite = self.may_replace(id, &job.output);
            // a result of another time, rather than this item's own
            let replaces = job.output.exists() && !self.wrote(id, &job.output);
            let (inputs, sequence) = (item.inputs.clone(), item.sequence);
            let item = self.item_mut(id).expect("found above");
            let (job, ..) = item.job.as_mut().expect("found above");
            match plan {
                Ok(plan) => {
                    job.replaces = replaces;
                    job.start(plan, inputs, sequence, overwrite, ctx);
                    return;
                }
                Err(message) => {
                    job.status = Status::Failed(message);
                }
            }
        }
    }

    /// Looks for the system's fonts for characters in names the window's
    /// own fonts lack.
    fn find_fonts(&mut self, ctx: &egui::Context) {
        let mut texts: Vec<String> = Vec::new();
        for item in &self.items {
            texts.push(item.name());
            texts.extend(item.inputs.iter().map(|path| path.display().to_string()));
            if let Some(made) = &item.made {
                texts.push(made.job.output.display().to_string());
            }
        }
        if let Some(item) = self.selected.and_then(|id| self.item(id)) {
            texts.push(self.output_for(item).display().to_string());
        }
        texts.extend(self.message.clone());
        texts.extend(
            self.inspection.as_ref().map(|inspection| inspection.path.display().to_string()),
        );
        texts.extend(self.config.gui.output_dir.as_ref().map(|dir| dir.display().to_string()));
        self.fallbacks.check(ctx, texts.iter().map(String::as_str));
    }

    /// Files dropped on the window, pasted, and keys.
    fn take_input(&mut self, ctx: &egui::Context) {
        let (dropped, hovered, text_pasted, v_released, space, delete) = ctx.input(|input| {
            let dropped: Vec<PathBuf> = input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect();
            let text_pasted =
                input.events.iter().any(|event| matches!(event, egui::Event::Paste(_)));
            let v_released = input.events.iter().find_map(|event| match event {
                egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. } => {
                    Some(modifiers.command)
                }
                _ => None,
            });
            (
                dropped,
                !input.raw.hovered_files.is_empty(),
                text_pasted,
                v_released,
                input.key_pressed(egui::Key::Space),
                input.key_pressed(egui::Key::Delete),
            )
        });
        self.hovering = hovered || self.hovering;
        if !dropped.is_empty() {
            self.add(dropped);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(drops) = &self.drops {
            for dropped in drops.poll() {
                match dropped {
                    dnd::Dropped::Hovering(over) => self.hovering = over,
                    dnd::Dropped::Files(files) => self.add(files),
                }
            }
        }
        // typing into a text field is just typing
        if ctx.memory(|memory| memory.focused().is_some()) {
            self.pasted = false;
            return;
        }
        if space
            && let Some(id) = self.selected
            && let Some(item) = self.item_mut(id)
        {
            item.view.playing = !item.view.playing;
        }
        if delete && let Some(id) = self.selected {
            self.remove(id);
        }
        // egui only says when text is pasted, so files and images are
        // pasted when Ctrl+V is let go
        if text_pasted {
            self.paste();
            self.pasted = true;
        }
        if let Some(command) = v_released {
            if command && !self.pasted {
                self.paste();
            }
            self.pasted = false;
        }
    }

    fn add_menu(&mut self, ui: &mut egui::Ui) {
        if ui
            .button("A folder of frames…")
            .on_hover_text("Its images, in name order, as one animation")
            .clicked()
        {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.add(vec![dir]);
            }
            ui.close();
        }
        if ui
            .button("Images as one animation…")
            .on_hover_text("The images picked, in name order")
            .clicked()
        {
            if let Some(files) = rfd::FileDialog::new()
                .add_filter(
                    "images",
                    &["png", "gif", "webp", "jpg", "jpeg", "bmp", "ase", "aseprite"],
                )
                .pick_files()
            {
                self.add_frames(files);
            }
            ui.close();
        }
        if ui
            .button("From the clipboard")
            .on_hover_text("Copied files, paths or an image (Ctrl+V)")
            .clicked()
        {
            self.paste();
            ui.close();
        }
    }

    fn pick_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new().pick_files() {
            self.add(files);
        }
    }

    fn files_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("tgradish").heading().strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⚙", |ui| {
                    if ui.button("Settings…").clicked() {
                        self.prefs.open = true;
                        ui.close();
                    }
                    if ui.button("About and licenses…").clicked() {
                        self.about = true;
                        ui.close();
                    }
                    if ui
                        .button("Inspect a sticker…")
                        .on_hover_text("Check a sticker file against Telegram's rules")
                        .clicked()
                    {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("stickers", &["webm", "tgs"])
                            .pick_file()
                        {
                            self.inspection = Some(inspect::Inspection::of(&file));
                        }
                        ui.close();
                    }
                });
                // a split button: files, or the rest from its arrow
                ui.spacing_mut().item_spacing.x = 1.0;
                ui.menu_button("⏷", |ui| self.add_menu(ui));
                if ui.button("Add files…").clicked() {
                    self.pick_files();
                }
            });
        });
        ui.add_space(4.0);
        ui.separator();

        let changed =
            self.items.iter().filter(|item| !item.is_busy() && !item.result_is_current()).count();
        egui::Panel::bottom("files-footer").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let text = if changed > 0 {
                    format!("Convert all ({changed})")
                } else {
                    "Convert all".into()
                };
                if ui
                    .add_enabled(changed > 0, egui::Button::new(text))
                    .on_hover_text("Every file without an up-to-date result")
                    .clicked()
                {
                    let ids: Vec<u64> = self
                        .items
                        .iter()
                        .filter(|item| !item.is_busy() && !item.result_is_current())
                        .map(|item| item.id)
                        .collect();
                    for id in ids {
                        self.enqueue(id);
                    }
                }
                if !self.items.is_empty() && ui.button("Remove all").clicked() {
                    let ids: Vec<u64> = self.items.iter().map(|item| item.id).collect();
                    for id in ids {
                        self.remove(id);
                    }
                }
            });
            ui.add_space(4.0);
        });

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.items.is_empty() {
                ui.add_space(20.0);
                widgets::note(ui, "Files you add show up here.");
            }
            let mut clicked = None;
            let mut removed = None;
            let mut again = None;
            for item in &self.items {
                let selected = self.selected == Some(item.id);
                let queued = self.queue.iter().position(|&id| id == item.id);
                let (response, action) = row(ui, item, selected, queued, &self.fallbacks);
                match action {
                    Some(RowAction::Remove) => removed = Some(item.id),
                    Some(RowAction::Convert) => again = Some(item.id),
                    None if response.clicked() => clicked = Some(item.id),
                    None => {}
                }
                response.context_menu(|ui| {
                    if ui.button("Remove").clicked() {
                        removed = Some(item.id);
                        ui.close();
                    }
                });
            }
            if let Some(id) = clicked {
                self.select(id);
            }
            if let Some(id) = removed {
                self.remove(id);
            }
            if let Some(id) = again {
                self.enqueue(id);
            }
        });
    }

    fn empty(&mut self, ui: &mut egui::Ui) {
        let zone = ui.max_rect().shrink(28.0);
        let colour = if self.hovering {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        };
        let dashes = egui::Shape::dashed_line(
            &[
                zone.left_top(),
                zone.right_top(),
                zone.right_bottom(),
                zone.left_bottom(),
                zone.left_top(),
            ],
            egui::Stroke::new(1.5, colour),
            8.0,
            6.0,
        );
        ui.painter().extend(dashes);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 110.0).max(40.0));
            ui.label(RichText::new("Drop videos, GIFs, images or Aseprite files here").size(22.0));
            ui.add_space(6.0);
            widgets::note(
                ui,
                "Videos become WebM stickers; pixel art can also become a crisp TGS animation.",
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let width = 400.0;
                ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
                let add = egui::Button::new(RichText::new("Add files…").strong())
                    .min_size(vec2(130.0, 34.0))
                    .fill(ui.visuals().selection.bg_fill);
                if ui.add(add).clicked() {
                    self.pick_files();
                }
                if ui
                    .add(egui::Button::new("A folder of frames…").min_size(vec2(150.0, 34.0)))
                    .clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.add(vec![dir]);
                }
                if ui
                    .add(egui::Button::new("Paste").min_size(vec2(90.0, 34.0)))
                    .on_hover_text("Ctrl+V")
                    .clicked()
                {
                    self.paste();
                }
            });
        });
    }

    fn main_panel(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.filter(|&id| self.item(id).is_some()) else {
            self.empty(ui);
            return;
        };
        let extra_args = self.backend().as_ref().is_ok_and(Backend::supports_extra_args);
        let queued = self.queue.iter().position(|&other| other == id);
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(8.0);
            self.actions(ui, id);
            ui.add_space(6.0);
        });
        let output = self.item(id).map(|item| self.output_for(item));
        let context =
            settings::Context { presets: &self.presets, config: &self.config, extra_args, output };
        let height = ui.available_height();
        let preview_height =
            (height * self.preview_share).clamp(240.0, (height - 140.0).max(240.0));
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        egui::Panel::top("preview").exact_size(preview_height).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let side = SIDE;
                let width = (ui.available_width() - side - 16.0).max(200.0);
                ui.allocate_ui_with_layout(
                    vec2(width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        preview(ui, &mut self.screen, &mut self.inset, item);
                    },
                );
                ui.add_space(8.0);
                ui.separator();
                ui.allocate_ui_with_layout(
                    vec2(side, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(side);
                        egui::ScrollArea::vertical()
                            .id_salt("side")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_max_width(side - 12.0);
                                ui.add_space(6.0);
                                settings::make(ui, item, &context);
                                summary(ui, item, queued);
                            });
                    },
                );
            });
        });
        // the line under the preview moves it
        let (line, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 8.0), egui::Sense::drag());
        let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if response.dragged() {
            self.preview_share =
                ((preview_height + response.drag_delta().y) / height).clamp(0.2, 0.85);
        }
        let colour = if response.hovered() || response.dragged() {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        };
        ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, colour));
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                settings::show(ui, item, &context);
                details(ui, item);
                ui.add_space(12.0);
            });
        });
    }

    fn actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let output = self.item(id).map(|item| self.output_for(item));
        let queued = self.queue.iter().position(|&other| other == id);
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        let mut convert = false;
        let mut replace = None;
        ui.horizontal(|ui| {
            match &item.job {
                Some((job, ..)) if job.is_running() => {
                    if ui.add(egui::Button::new("Stop").min_size(vec2(140.0, 32.0))).clicked() {
                        job.cancel();
                    }
                    widgets::note(ui, "Converting: progress is beside the preview");
                }
                Some((job, ..)) => {
                    if ui
                        .add(egui::Button::new("Don't convert").min_size(vec2(140.0, 32.0)))
                        .clicked()
                    {
                        job.cancel();
                        item.job = None;
                        self.queue.retain(|&other| other != id);
                    } else {
                        let ahead = queued.unwrap_or(0);
                        widgets::note(
                            ui,
                            if ahead == 0 {
                                "Next to convert".to_owned()
                            } else {
                                format!("Waiting for {ahead} more")
                            },
                        );
                    }
                }
                None => {
                    let current = item.result_is_current();
                    let text = if item.made.is_some() { "Convert again" } else { "Convert" };
                    let mut button = egui::Button::new(RichText::new(text).strong().size(15.0))
                        .min_size(vec2(140.0, 32.0))
                        .shortcut_text("Ctrl+Enter");
                    if !current {
                        button = button.fill(ui.visuals().selection.bg_fill);
                    }
                    let hint = if current {
                        "Makes it again with the same settings"
                    } else {
                        "Makes it with these settings"
                    };
                    if ui.add(button).on_hover_text(hint).clicked() {
                        convert = true;
                    }
                    if let Some(failed) = &item.failed {
                        match &failed.job.status {
                            Status::Failed(message) => {
                                ui.colored_label(widgets::BAD, format!("Failed: {message}"));
                            }
                            Status::Exists(path) => {
                                ui.colored_label(
                                    widgets::WARN,
                                    "A file not made here is in the way:",
                                );
                                if ui.button("Replace it").clicked() {
                                    replace = Some(path.clone());
                                }
                            }
                            Status::Cancelled => widgets::note(ui, "Stopped"),
                            Status::Done(_) | Status::Waiting | Status::Running => {}
                        }
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let Some(output) = &output else { return };
                if output.exists() && ui.button("Show in folder").clicked() {
                    jobs::reveal(output);
                }
                let name = output
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let folder = output.parent().map(|p| p.display().to_string()).unwrap_or_default();
                let fallbacks = &self.fallbacks;
                let strong = text_format(ui, ui.visuals().strong_text_color());
                let (name, missing) = drawable(ui, fallbacks, &name, strong);
                ui.label(name).on_hover_ui(|ui| {
                    let plain = text_format(ui, ui.visuals().text_color());
                    ui.label(drawable(ui, fallbacks, &output.display().to_string(), plain).0);
                    if let Some(note) = missing_note(&missing) {
                        widgets::note(ui, note);
                    }
                });
                let weak = text_format(ui, ui.visuals().weak_text_color());
                let saves = format!("Saves to {folder}{}", std::path::MAIN_SEPARATOR);
                ui.add(egui::Label::new(drawable(ui, fallbacks, &saves, weak).0).truncate())
                    .on_hover_text("Settings chooses where results go");
            });
        });
        let shortcut =
            ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));
        // the user lets this item replace the file that is there now
        if let Some(path) = replace {
            if let Some(written) = Written::of(id, &path) {
                self.written.insert(path, written);
            }
            convert = true;
        }
        if convert || (shortcut && self.item(id).is_some_and(|item| item.job.is_none())) {
            self.enqueue(id);
        }
    }
}

/// Width of the column beside the preview.
const SIDE: f32 = 300.0;

/// The big preview of an item, its header and timeline.
fn preview(ui: &mut egui::Ui, screen: &mut Screen, inset: &mut Inset, item: &mut Item) {
    let has_result = item.result_clip.ready().is_some();
    // a change to what is used or how it is fitted shows on the input
    let framing = Framing::of(item);
    if item.view.framing.as_ref().is_some_and(|seen| *seen != framing) {
        item.view.show = Show::Input;
    }
    item.view.framing = Some(framing);
    if !has_result {
        item.view.show = Show::Input;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mut show = item.view.show;
        let choices = [(Show::Input, "Input", ""), (Show::Result, "Result", "")];
        widgets::segments(ui, &mut show, &choices, |show| {
            if show == Show::Result && !has_result { Err("Convert it first") } else { Ok(()) }
        });
        item.view.show = show;
        if has_result && !item.result_is_current() {
            ui.colored_label(widgets::WARN, "made with earlier settings");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if item.view.show == Show::Input {
                ui.toggle_value(&mut item.view.inset, "Result preview")
                    .on_hover_text("What the result will look like, small, over the input");
            }
            let zoom = item.view.zoom_mut();
            if zoom.level > 1.0 {
                if ui.small_button("Fit").clicked() {
                    *zoom = Zoom::default();
                }
                ui.label(format!("{:.0}%", zoom.level * 100.0));
            } else {
                widgets::note(
                    ui,
                    match (item.view.show, item.choices.crop.is_some()) {
                        (Show::Input, false) => {
                            "Drag to crop · wheel to zoom · right button to pan"
                        }
                        (Show::Input, true) => "Drag the crop or its edges · double-click for all",
                        (Show::Result, _) => "Wheel to zoom · right button to pan",
                    },
                );
            }
        });
    });

    let length = item.input_length();
    let result_length =
        item.result_clip.ready().filter(|clip| clip.is_animated()).map(|clip| clip.end());
    let timeline_height = match item.view.show {
        Show::Input if length.is_some() => 38.0,
        Show::Result if result_length.is_some() => 38.0,
        _ => 0.0,
    };
    let size = ui.available_size() - vec2(0.0, timeline_height + 6.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let dt = ui.input(|input| f64::from(input.stable_dt.min(0.1)));
    let range = item.range();
    let input_size = item.input_size();
    let error = item.input_error().map(str::to_owned);
    // fields rather than methods, so the view and crop can change
    let (overview, part, still) = match item.format {
        Format::Webm => (
            item.video.ready().map(|video| &video.overview),
            item.part.as_ref().and_then(|(_, part)| part.ready()),
            item.still.as_ref().and_then(|(at, still)| Some((*at, still.ready()?))),
        ),
        Format::Tgs => (item.art.ready().map(|art| &art.clip), None, None),
    };
    let target = item.choices.target;
    let resize = settings::resize_of(item);
    let filling = (item.format == Format::Webm && resize == Resize::Crop).then(|| {
        let (width, height) = target.box_size();
        f64::from(width) / f64::from(height)
    });

    match item.view.show {
        Show::Input => {
            if let Some(message) = error {
                message_in(ui, rect, &format!("Couldn't read it: {message}"), widgets::BAD);
            } else if let (Some(clip), Some(size)) = (overview, input_size) {
                // play within the used part
                if let Some((start, end)) = range {
                    if item.view.playing {
                        item.view.time += dt;
                        ui.ctx().request_repaint();
                    }
                    if item.view.time < start || item.view.time >= end {
                        item.view.time = start;
                    }
                }
                let time = item.view.time;
                let part = part.filter(|part| part.start() <= time && time < part.end() + 0.05);
                // paused: the sharp frame, once it is there
                // and still images, which don't play, always
                let still =
                    still.filter(|(at, _)| range.is_none() || (!item.view.playing && *at == time));
                let clip = still.map(|(_, still)| still).or(part).unwrap_or(clip);
                let frame = clip.index_at(time);
                let picture = Picture { clip, frame, size, square: false };
                let ratio = settings::ratio(item.view.aspect, size);
                let exact = settings::exact_of(item);
                let cropping = Cropping { crop: &mut item.choices.crop, ratio, filling, exact };
                canvas::show(ui, screen, rect, &picture, &mut item.view.input_zoom, Some(cropping));
                if item.view.inset {
                    let crop = item.choices.crop;
                    let where_ = output::Part::of(crop, size, (clip.width, clip.height));
                    match item.format {
                        Format::Webm => {
                            let used = crop.map_or(size, |crop| (crop.width, crop.height));
                            let scaling = item.choices.webm.scaling.unwrap_or(Scaling::Auto);
                            // a crop that doesn't suit the exact scale yet is made to
                            // fit on the next frame
                            let sizes = match item.choices.webm.exact_scale {
                                Some(scale) => convert::exact_sizes(target, used, scale).ok(),
                                None => Some(convert::sizes(target, resize, scaling, used)),
                            };
                            if let Some(mut sizes) = sizes {
                                let art_key = inset_key((clip.id, where_));
                                if scaling == Scaling::Auto
                                    && item.choices.webm.exact_scale.is_none()
                                    && sizes.enlarges(used) >= 2.0
                                    && inset.looks_like_art(art_key, || {
                                        output::looks_like_art(clip, where_)
                                    })
                                {
                                    sizes.scaling = Scaling::Sharp;
                                }
                                let key = inset_key((clip.id, frame, where_, sizes));
                                let label =
                                    format!("Result: {} × {} px", sizes.width, sizes.height);
                                inset.show(ui, rect, key, &label, || {
                                    output::webm(&clip.frames[frame], clip.width, where_, &sizes)
                                });
                            }
                        }
                        Format::Tgs => {
                            let keep = item.choices.tgs.keep_canvas.unwrap_or(false);
                            let bounds = inset
                                .tgs_bounds(inset_key((clip.id, where_, keep)), || {
                                    output::tgs_bounds(&clip.frames, clip.width, where_, keep)
                                });
                            let key = inset_key((clip.id, frame, bounds));
                            inset.show(ui, rect, key, "Result: 512 × 512 canvas", || {
                                output::tgs(&clip.frames[frame], clip.width, bounds, 256)
                            });
                        }
                    }
                }
            } else {
                message_in(ui, rect, "Reading…", ui.visuals().weak_text_color());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        Show::Result => {
            if let Some(clip) = item.result_clip.ready() {
                let length = clip.end().max(1e-3);
                if item.view.playing && clip.is_animated() {
                    item.view.result_time = (item.view.result_time + dt).rem_euclid(length);
                    ui.ctx().request_repaint();
                }
                let square = item.made.as_ref().is_some_and(|made| made.format == Format::Tgs);
                let picture = Picture {
                    clip,
                    frame: clip.index_at(item.view.result_time.rem_euclid(length)),
                    size: (clip.width, clip.height),
                    square,
                };
                canvas::show(ui, screen, rect, &picture, &mut item.view.result_zoom, None);
            }
        }
    }

    if timeline_height == 0.0 {
        return;
    }
    ui.add_space(6.0);
    match (item.view.show, length, result_length) {
        (Show::Input, Some(length), _) => timeline::show(
            ui,
            timeline::Timeline {
                length,
                start: &mut item.choices.start,
                used: &mut item.choices.length,
                time: &mut item.view.time,
                playing: &mut item.view.playing,
                limit: telegram::MAX_SECONDS,
            },
        ),
        (Show::Result, _, Some(length)) => {
            timeline::player(ui, length, &mut item.view.result_time, &mut item.view.playing);
        }
        _ => {}
    }
}

/// A key that changes when any of `parts` does.
fn inset_key(parts: impl std::fmt::Debug) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    format!("{parts:?}").hash(&mut hasher);
    hasher.finish()
}

/// tgradish's version and the licences of what it is made of.
fn about(ui: &mut egui::Ui) {
    ui.label(RichText::new(format!("tgradish {}", env!("CARGO_PKG_VERSION"))).heading());
    ui.horizontal(|ui| {
        ui.label("Telegram stickers from videos and pixel art. MIT licence;");
        ui.hyperlink_to("source code", "https://github.com/sliva0/tgradish");
    });
    ui.add_space(6.0);
    widgets::note(
        ui,
        "tgradish is built from other people's work too. Their licences ask for these notices \
         to go with every copy, so the program carries them:",
    );
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let notices = ui.ctx().memory_mut(|memory| {
            memory
                .data
                .get_temp_mut_or_insert_with(egui::Id::new("notices"), || {
                    std::sync::Arc::new(notices())
                })
                .clone()
        });
        for notice in notices.iter() {
            egui::CollapsingHeader::new(&notice.title).id_salt(("notice", &notice.title)).show(
                ui,
                |ui| {
                    if notice.title.starts_with("ffmpeg: what") {
                        ui.hyperlink_to(
                            "The exact sources of the ffmpeg built in, with this release",
                            format!(
                                "https://github.com/sliva0/tgradish/releases/tag/v{}",
                                env!("CARGO_PKG_VERSION")
                            ),
                        );
                    }
                    // the crates' list is long: a section for each licence
                    let rule = format!("\n{}\n", "-".repeat(80));
                    let blocks: Vec<&str> = notice.text.split(rule.as_str()).collect();
                    if blocks.len() > 1 {
                        widgets::note(ui, blocks[0].trim());
                        for block in &blocks[1..] {
                            let (head, body) = block.split_once("\n\n").unwrap_or((block, ""));
                            let crates = head.lines().count().saturating_sub(1);
                            let name =
                                head.lines().next().unwrap_or("").trim_end_matches(", used by:");
                            let title = format!(
                                "{name}, {crates} crate{}",
                                if crates == 1 { "" } else { "s" }
                            );
                            egui::CollapsingHeader::new(title).id_salt(("crates", *block)).show(
                                ui,
                                |ui| {
                                    widgets::note(
                                        ui,
                                        head.lines()
                                            .skip(1)
                                            .map(str::trim)
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                    );
                                    ui.label(RichText::new(body.trim()).monospace().size(11.5));
                                },
                            );
                        }
                    } else {
                        ui.label(RichText::new(notice.text.trim()).monospace().size(11.5));
                    }
                },
            );
        }
    });
}

/// Scrolls by what the wheel turned this frame, instead of egui's easing
/// over the next few frames.
fn direct_scrolling(ctx: &egui::Context) {
    let (line, page) = (
        ctx.options(|options| options.input_options.line_scroll_speed),
        ctx.content_rect().height(),
    );
    ctx.input_mut(|input| {
        // the command key turns the wheel into zooming
        if input.modifiers.command {
            return;
        }
        let mut delta = egui::Vec2::ZERO;
        for event in &input.raw.events {
            if let egui::Event::MouseWheel { unit, delta: turned, .. } = event {
                delta += match unit {
                    egui::MouseWheelUnit::Point => *turned,
                    egui::MouseWheelUnit::Line => *turned * line,
                    egui::MouseWheelUnit::Page => *turned * page,
                };
            }
        }
        if input.modifiers.shift {
            delta = vec2(delta.x + delta.y, 0.0);
        }
        input.smooth_scroll_delta = delta;
    });
}

/// A message over the bottom of the window, gone when dismissed or after a
/// while. False once it is gone.
fn toast(ctx: &egui::Context, message: &str) -> bool {
    const SHOWN: f64 = 12.0;
    let now = ctx.input(|input| input.time);
    let id = egui::Id::new(("toast", message));
    let since = ctx.data_mut(|data| *data.get_temp_mut_or_insert_with(id, || now));
    let mut open = true;
    let area = egui::Area::new(egui::Id::new("toast"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -56.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(560.0);
                ui.horizontal(|ui| {
                    ui.colored_label(widgets::WARN, "⚠");
                    ui.label(message);
                    if ui.small_button("OK").clicked() {
                        open = false;
                    }
                });
            });
        });
    // hovering keeps it
    if area.response.contains_pointer() {
        ctx.data_mut(|data| data.insert_temp(id, now));
    } else if now - since > SHOWN {
        open = false;
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
    if !open {
        ctx.data_mut(|data| data.remove::<f64>(id));
    }
    open
}

/// Whether `value`, kept under `key`, has stayed the same for `wait`
/// seconds; asks for a repaint to look again when it hasn't.
fn settled(
    ctx: &egui::Context,
    key: impl std::hash::Hash + std::fmt::Debug,
    value: (f64, f64),
    wait: f64,
) -> bool {
    let now = ctx.input(|input| input.time);
    let since = ctx.data_mut(|data| {
        let entry = data.get_temp_mut_or_insert_with(egui::Id::new(key), || (value, now));
        if entry.0 != value {
            *entry = (value, now);
        }
        entry.1
    });
    let settled = now - since >= wait;
    if !settled {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait / 4.0));
    }
    settled
}

/// Makes the input thumbnail and settles the guessed format once an
/// input is read.
fn settle(ctx: &egui::Context, item: &mut Item) {
    if item.format_guessed
        && let Some(art) = item.art.ready()
    {
        item.format_guessed = false;
        if art.clip.looks_like_pixel_art() {
            item.format = Format::Tgs;
        }
    }
    if item.input_thumb.is_none()
        && let Some(clip) = item.input_clip()
    {
        let image = clip.thumbnail(THUMB as u32 * 2);
        item.input_thumb =
            Some(ctx.load_texture(format!("input-{}", item.id), image, texture_options(clip)));
    }
}

/// Frees an item's big frames; its thumbnails stay.
fn release(item: &mut Item) {
    if item.video.ready().is_some() {
        item.video = Load::Idle;
    }
    item.part = None;
    item.still = None;
    if item.result_clip.ready().is_some_and(|clip| !clip.pixelated) {
        item.result_clip = Load::Idle;
    }
}

fn texture_options(clip: &Clip) -> egui::TextureOptions {
    if clip.pixelated { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR }
}

fn load_result_clip(ctx: &egui::Context, item: &mut Item, backend: &Result<Backend, String>) {
    let Some(Made { job, format: Format::Webm, .. }) = &item.made else { return };
    let (Status::Done(done), Ok(backend)) = (&job.status, backend) else { return };
    item.result_clip = Load::Loading(media::load_result(ctx, backend.clone(), &done.output));
}

fn message_in(ui: &egui::Ui, rect: egui::Rect, text: &str, colour: Color32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(24));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(15.0),
        colour,
    );
}

/// What a row's own buttons ask for.
enum RowAction {
    Remove,
    Convert,
}

/// One file in the list: its input, name and state, and its result, with
/// what its buttons asked for.
fn row(
    ui: &mut egui::Ui,
    item: &Item,
    selected: bool,
    queued: Option<usize>,
    fallbacks: &fallback::Fallbacks,
) -> (egui::Response, Option<RowAction>) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, THUMB + 12.0), egui::Sense::click());
    let visuals = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 6.0, visuals.selection.bg_fill.gamma_multiply(0.45));
    } else if response.hovered() {
        ui.painter().rect_filled(
            rect,
            6.0,
            visuals.widgets.hovered.weak_bg_fill.gamma_multiply(0.5),
        );
    }
    let inner = rect.shrink2(vec2(6.0, 6.0));
    let input = egui::Rect::from_min_size(inner.min, vec2(THUMB, THUMB));
    thumb(ui, input, item.input_thumb.as_ref(), item.input_error().is_some(), false);
    let result =
        egui::Rect::from_min_size(egui::pos2(inner.max.x - THUMB, inner.min.y), vec2(THUMB, THUMB));
    let mut action = None;
    if item.result_thumb.is_some() {
        let stale = !item.result_is_current();
        thumb(ui, result, item.result_thumb.as_ref(), false, stale);
        if stale && !item.is_busy() {
            let badge =
                egui::Rect::from_min_size(result.right_top() - vec2(20.0, 0.0), vec2(20.0, 20.0));
            let button = egui::Button::new(RichText::new("⟳").size(13.0).color(Color32::BLACK))
                .fill(widgets::WARN)
                .corner_radius(4.0);
            // placed, so the rows below don't move
            if ui.place(badge, button).on_hover_text("Settings changed: convert again").clicked() {
                action = Some(RowAction::Convert);
            }
        }
    }
    let mut text = egui::Rect::from_min_max(
        egui::pos2(input.max.x + 10.0, inner.min.y),
        egui::pos2(result.min.x - 8.0, inner.max.y),
    );
    if response.hovered() || response.contains_pointer() {
        let button =
            egui::Rect::from_min_size(egui::pos2(text.max.x - 20.0, text.min.y), vec2(20.0, 20.0));
        text.max.x = button.min.x - 4.0;
        // placed, so the rows below don't move
        if ui
            .place(button, egui::Button::new("✖").small().frame(false))
            .on_hover_text("Remove it from the list")
            .clicked()
        {
            action = Some(RowAction::Remove);
        }
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(text).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 2.0;
    let strong = text_format(&child, child.visuals().strong_text_color());
    let (name, _) = drawable(&child, fallbacks, &item.name(), strong);
    child.add(egui::Label::new(name).truncate().selectable(false));
    let what = match (item.format, item.choices.target) {
        (Format::Webm, telegram::Target::Sticker) => "WebM sticker",
        (Format::Webm, telegram::Target::Emoji) => "WebM emoji",
        (Format::Tgs, telegram::Target::Sticker) => "TGS sticker",
        (Format::Tgs, telegram::Target::Emoji) => "TGS emoji",
    };
    child.add(egui::Label::new(RichText::new(what).small().weak()).selectable(false));
    status(&mut child, item, queued);
    let response = response.on_hover_ui(|ui| {
        let plain = text_format(ui, ui.visuals().text_color());
        let mut missing = Vec::new();
        for input in &item.inputs {
            let (path, lacking) =
                drawable(ui, fallbacks, &input.display().to_string(), plain.clone());
            ui.label(path);
            missing.extend(lacking);
        }
        if let Some(note) = missing_note(&missing) {
            widgets::note(ui, note);
        }
    });
    (response, action)
}

/// Body text in `colour`.
fn text_format(ui: &egui::Ui, colour: Color32) -> egui::TextFormat {
    egui::TextFormat::simple(egui::TextStyle::Body.resolve(ui.style()), colour)
}

/// `text` laid out as the window can draw it: each character no font has,
/// neither the window's nor the system's, as a small tag of its code, and
/// formatting characters, which draw nothing, left out. Also the
/// characters no font has.
fn drawable(
    ui: &egui::Ui,
    fallbacks: &fallback::Fallbacks,
    text: &str,
    format: egui::TextFormat,
) -> (egui::text::LayoutJob, Vec<char>) {
    drawable_in(fallbacks, text, format, ui.visuals().widgets.inactive.bg_fill)
}

fn drawable_in(
    fallbacks: &fallback::Fallbacks,
    text: &str,
    format: egui::TextFormat,
    tag_fill: Color32,
) -> (egui::text::LayoutJob, Vec<char>) {
    let mut job = egui::text::LayoutJob::default();
    let mut missing = Vec::new();
    if text.is_ascii() {
        job.append(text, 0.0, format);
        return (job, missing);
    }
    let tag = egui::TextFormat {
        font_id: egui::FontId::monospace(format.font_id.size * 0.72),
        color: format.color.gamma_multiply(0.7),
        background: tag_fill,
        valign: egui::Align::Center,
        ..Default::default()
    };
    let (mut run, mut gap) = (String::new(), 0.0);
    for c in text.chars() {
        if fallback::formatting(c) {
            continue;
        }
        if !fallbacks.missing(c) {
            run.push(c);
            continue;
        }
        if !run.is_empty() {
            job.append(&run, gap, format.clone());
            run.clear();
        }
        job.append(&format!("U+{:04X}", c as u32), 2.0, tag.clone());
        gap = 2.0;
        missing.push(c);
    }
    if !run.is_empty() {
        job.append(&run, gap, format);
    }
    (job, missing)
}

/// What to say of characters no font has, if there are any.
fn missing_note(missing: &[char]) -> Option<String> {
    let mut codes: Vec<String> = missing.iter().map(|c| format!("U+{:04X}", *c as u32)).collect();
    codes.dedup();
    let (them, are, their) = match codes.len() {
        0 => return None,
        1 => ("it", "is", "its"),
        _ => ("them", "are", "their"),
    };
    Some(format!(
        "No font was found for {}, neither among the window's own nor the system's, so {them} {are} \
         shown by {their} code. The file itself is fine.",
        codes.join(", ")
    ))
}

/// A thumbnail in `rect`, marked when it is out of date.
fn thumb(
    ui: &egui::Ui,
    rect: egui::Rect,
    texture: Option<&egui::TextureHandle>,
    failed: bool,
    stale: bool,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_gray(34));
    match texture {
        Some(texture) => {
            let size = texture.size_vec2();
            let scale = (THUMB / size.x).min(THUMB / size.y);
            let shown = egui::Rect::from_center_size(rect.center(), size * scale);
            widgets::checkerboard(&painter, shown, 6.0);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            let tint = if stale { Color32::from_gray(140) } else { Color32::WHITE };
            painter.image(texture.id(), shown, uv, tint);
        }
        None if failed => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "✖",
                egui::FontId::proportional(16.0),
                widgets::BAD,
            );
        }
        None => {}
    }
}

fn status(ui: &mut egui::Ui, item: &Item, queued: Option<usize>) {
    let small = |text: String| RichText::new(text).small();
    match (&item.job, item.failed.as_ref().or(item.made.as_ref())) {
        (Some((job, ..)), _) if job.is_running() => {
            ui.add_space(3.0);
            let size = vec2(ui.available_width(), 6.0);
            let colour = ui.visuals().selection.bg_fill;
            widgets::bar_sized(ui, size, job.progress.overall(), "", colour);
        }
        (Some(_), _) => {
            ui.label(
                small(match queued {
                    Some(0) | None => "next".into(),
                    Some(n) => format!("waiting, {n} ahead"),
                })
                .weak(),
            );
        }
        (None, Some(made)) => match &made.job.status {
            Status::Done(done) => {
                let refused = done.problems.iter().any(|p| p.refused);
                let colour = if refused { widgets::BAD } else { widgets::GOOD };
                let mark = if refused { "✖" } else { "✔" };
                let stale = if item.result_is_current() { "" } else { " · settings changed" };
                ui.colored_label(
                    colour,
                    small(format!("{mark} {}{stale}", widgets::kib(done.bytes))),
                );
            }
            Status::Failed(_) => {
                ui.colored_label(widgets::BAD, small("✖ failed".into()));
            }
            Status::Exists(_) => {
                ui.colored_label(widgets::WARN, small("a file is in the way".into()));
            }
            Status::Cancelled => {
                ui.label(small("stopped".into()).weak());
            }
            Status::Waiting | Status::Running => {}
        },
        (None, None) => {}
    }
}

/// The last result of an item in short: its size and what Telegram would
/// say, beside the preview.
fn summary(ui: &mut egui::Ui, item: &Item, queued: Option<usize>) {
    if let Some((job, ..)) = &item.job {
        progress_column(ui, job, queued);
    }
    if let Some(failed) = &item.failed {
        widgets::section(ui, "Last conversion");
        match &failed.job.status {
            Status::Failed(message) => {
                ui.colored_label(widgets::BAD, message);
            }
            Status::Exists(path) => {
                ui.colored_label(widgets::WARN, format!("{} is in the way", path.display()));
            }
            Status::Cancelled => widgets::note(ui, "Stopped before it was done"),
            Status::Done(_) | Status::Waiting | Status::Running => {}
        }
    }
    let Some(made) = &item.made else { return };
    widgets::section(
        ui,
        if item.result_is_current() { "Result" } else { "Result, from earlier settings" },
    );
    match &made.job.status {
        Status::Done(done) => {
            widgets::gauge(
                ui,
                done.bytes,
                done.limit,
                format!("{} of {}", widgets::kib(done.bytes), widgets::kib(done.limit)),
            );
            ui.add_space(4.0);
            widgets::problems(ui, &done.problems);
            if let Some(plan) = &made.job.progress.webm {
                let spoofed = if done.spoofed { ", duration spoofed" } else { "" };
                widgets::note(
                    ui,
                    format!(
                        "{} × {} px, {:.2} fps, {}{spoofed}",
                        plan.width,
                        plan.height,
                        plan.fps,
                        widgets::seconds(plan.length)
                    ),
                );
            }
            if done.lossy {
                widgets::note(ui, "Changed to fit: see Details below");
            }
            if made.job.replaces {
                ui.colored_label(widgets::WARN, "⚠ Replaced an earlier result made by tgradish");
            }
        }
        Status::Failed(message) => {
            ui.colored_label(widgets::BAD, message);
        }
        Status::Exists(path) => {
            ui.colored_label(widgets::WARN, format!("{} is in the way", path.display()));
        }
        Status::Cancelled => widgets::note(ui, "Stopped before it was done"),
        Status::Waiting | Status::Running => {}
    }
}

/// What a conversion is doing, a bar for each part of it: the stages of a
/// `.tgs`, or each encode of a WebM.
fn progress_column(ui: &mut egui::Ui, job: &Job, queued: Option<usize>) {
    widgets::section(ui, "Converting");
    if job.is_waiting() {
        widgets::note(
            ui,
            match queued {
                Some(0) | None => "Next to convert".to_owned(),
                Some(ahead) => format!("Waiting for {ahead} more"),
            },
        );
        return;
    }
    let progress = &job.progress;
    let running = ui.visuals().selection.bg_fill;
    let done = widgets::GOOD.gamma_multiply(0.45);
    if let Some(stage) = progress.tgs {
        use jobs::TgsStage;
        let fits = progress.lossless_bytes.is_none() && stage > TgsStage::Fitting;
        let stages = [
            (TgsStage::Reading, "Reading the art".to_owned()),
            (TgsStage::Drawing, "Drawing it as shapes".to_owned()),
            (
                TgsStage::Fitting,
                match (progress.lossless_bytes, progress.steps.last()) {
                    (Some(_), Some(step)) => {
                        let count = progress.steps.len();
                        let steps = if count == 1 { "step" } else { "steps" };
                        format!(
                            "Fitting into 64 KiB: {count} {steps}, about {}",
                            widgets::kib(step.bytes as u64)
                        )
                    }
                    (Some(bytes), None) => {
                        format!("Fitting into 64 KiB from {}", widgets::kib(bytes as u64))
                    }
                    (None, _) if fits => "Fits as it is".to_owned(),
                    (None, _) => "Fitting into 64 KiB, if it has to".to_owned(),
                },
            ),
            (TgsStage::Compressing, "Compressing".to_owned()),
        ];
        for (each, text) in stages {
            let (fraction, colour) = match each.cmp(&stage) {
                std::cmp::Ordering::Less => (Some(1.0), done),
                std::cmp::Ordering::Equal if each == TgsStage::Fitting => {
                    (progress.tgs_fitting(), running)
                }
                std::cmp::Ordering::Equal => (None, running),
                std::cmp::Ordering::Greater => (Some(0.0), running),
            };
            widgets::bar(ui, fraction, &text, colour);
            ui.add_space(2.0);
        }
        return;
    }
    if progress.attempts.is_empty() {
        widgets::bar(ui, None, "Starting", running);
    }
    for attempt in &progress.attempts {
        let rate = match attempt.params.rate {
            tgradish_core::events::Rate::Bitrate(kbps) => format!("{kbps:.0} kbit/s"),
            tgradish_core::events::Rate::Crf(crf) => format!("quality {crf}"),
            tgradish_core::events::Rate::Lossless => "lossless".into(),
        };
        let text = format!("Encode {}: {:.0} fps, {rate}", attempt.number, attempt.params.fps);
        match attempt.bytes {
            Some(bytes) => {
                let (mark, colour) = if attempt.fits {
                    ("fits", done)
                } else {
                    ("too large", widgets::BAD.gamma_multiply(0.45))
                };
                let text = format!("{text} → {}, {mark}", widgets::kib(bytes));
                widgets::bar(ui, Some(1.0), &text, colour);
            }
            None => widgets::bar(ui, progress.fraction, &text, running),
        }
        ui.add_space(2.0);
    }
    if let Some(plan) = &progress.webm
        && plan.attempts > 1
    {
        widgets::note(
            ui,
            format!("Up to {} encodes, until one lands just under the limit", plan.attempts),
        );
    }
}

/// How the last result was made, under the settings.
fn details(ui: &mut egui::Ui, item: &Item) {
    let Some(made) = &item.made else { return };
    let Status::Done(done) = &made.job.status else { return };
    let progress = &made.job.progress;
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new("Details of the result").strong()).id_salt(("details", item.id)).show(ui, |ui| {
        if let Some(report) = &progress.report {
            let scale = if report.scale > 1 { format!(", each {0} × {0} input pixels", report.scale) } else { String::new() };
            widgets::note(ui, format!("{} × {} art pixels{scale}; {} colours, {} frames", report.width, report.height, report.colours, report.frames));
            if report.speed > 1.0 {
                widgets::note(ui, format!("Plays {:.2}× faster to last 3 seconds", report.speed));
            }
            if let Some(likely) = report.likely_scale {
                widgets::note(ui, format!("Most of it looks like {0}× art with some pixels off the grid; an art pixel size of {0} snaps them", likely.scale));
            }
        }
        if let (Some(json), Some((layers, rectangles))) = (done.json_bytes, done.shapes) {
            widgets::note(ui, format!("{layers} layers of {rectangles} rectangles, {} unpacked", widgets::kib(json)));
        }
        if let Some(bytes) = progress.lossless_bytes {
            widgets::note(ui, format!("About {} as it is; changed to fit:", widgets::kib(bytes as u64)));
        }
        for step in &progress.steps {
            widgets::note(ui, format!("  • {}, about {} after", reduction(&step.reduction), widgets::kib(step.bytes as u64)));
        }
        if !progress.attempts.is_empty() {
            egui::Grid::new(("attempts", item.id)).num_columns(5).spacing([16.0, 4.0]).striped(true).show(ui, |ui| {
                for header in ["Encode", "frame rate", "rate", "size", "likeness"] {
                    ui.label(RichText::new(header).weak());
                }
                ui.end_row();
                for attempt in &progress.attempts {
                    let kept = if done.kept == Some(attempt.number) { " (kept)" } else { "" };
                    ui.label(format!("{}{kept}", attempt.number));
                    ui.label(format!("{:.2} fps", attempt.params.fps));
                    ui.label(match attempt.params.rate {
                        tgradish_core::events::Rate::Bitrate(kbps) => format!("{kbps:.0} kbit/s"),
                        tgradish_core::events::Rate::Crf(crf) => format!("quality {crf}"),
                        tgradish_core::events::Rate::Lossless => "lossless".into(),
                    });
                    match attempt.bytes {
                        Some(bytes) => ui.colored_label(if attempt.fits { widgets::GOOD } else { widgets::BAD }, widgets::kib(bytes)),
                        None => ui.label("—"),
                    };
                    ui.label(attempt.ssim.map(|ssim| format!("{:.1}%", ssim * 100.0)).unwrap_or_default());
                    ui.end_row();
                }
            });
        }
        for warning in &progress.warnings {
            ui.colored_label(widgets::WARN, format!("⚠ {warning}"));
        }
    });
}

fn reduction(reduction: &tgradish_tgs::reduce::Reduction) -> String {
    use tgradish_tgs::reduce::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => format!("pixels snapped to a grid of {scale}"),
        Reduction::MergeColours { distance } => {
            format!("colours closer than {:.0}% merged", distance * 100.0)
        }
        Reduction::MergeFrames { changed } => {
            format!("frames that change under {:.1}% merged", changed * 100.0)
        }
        Reduction::DropFrames { share } => format!("{:.0}% of frames dropped", share * 100.0),
        Reduction::Despeckle => "single stray pixels removed".into(),
        Reduction::Downscale { factor } => format!("resolution lowered to {:.0}%", factor * 100.0),
    }
}

/// `first`, or the first of `first` numbered before `suffix` (`clip
/// 2.sticker.webm`) that no other item claims and no `blocked` file is
/// at: the item's `own` earlier result if it is one of them.
fn free_name(
    first: &Path,
    suffix: &str,
    claimed: &HashSet<&Path>,
    own: Option<&Path>,
    blocked: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let name = first.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let stem = name.strip_suffix(suffix).unwrap_or(&name).to_owned();
    let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
    let names = || {
        std::iter::once(first.to_path_buf())
            .chain((2..).map(|n| dir.join(format!("{stem} {n}{suffix}"))))
            .filter(|path| !claimed.contains(path.as_path()))
    };
    if let Some(own) = own
        && !blocked(own)
        && names().take(1000).any(|path| path == own)
    {
        return own.to_path_buf();
    }
    names().find(|path| !blocked(path)).expect("some name is free")
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // the listener shares winit's Wayland connection, which closes next
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(drops) = &mut self.drops {
            drops.stop();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.config.gui.smooth_scrolling {
            direct_scrolling(&ctx);
        }
        self.take_input(&ctx);
        self.pump(&ctx);
        self.find_fonts(&ctx);
        egui::Panel::left("files")
            .resizable(true)
            .default_size(330.0)
            .size_range(260.0..=520.0)
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
        if let Some(inspection) = &self.inspection {
            let mut open = true;
            let title = inspection
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let format = egui::TextFormat::simple(
                egui::TextStyle::Heading.resolve(&ctx.global_style()),
                ctx.global_style().visuals.text_color(),
            );
            let title = drawable_in(&self.fallbacks, &title, format, Color32::GRAY).0;
            egui::Window::new(title).open(&mut open).collapsible(false).default_width(460.0).show(
                &ctx,
                |ui| {
                    inspect::show(ui, inspection);
                },
            );
            if !open {
                self.inspection = None;
            }
        }
        if self.about {
            let mut open = true;
            egui::Window::new("About tgradish")
                .open(&mut open)
                .collapsible(false)
                .default_size([560.0, 480.0])
                .show(&ctx, about);
            self.about = open;
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
                "Drop to add",
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
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use tgradish_core::options::Crop;

    use super::*;

    /// A 4x4 PNG with a 2x2 square of `colour` in the middle.
    fn square(path: &Path, colour: [u8; 4]) {
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, 4, 4);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let inside = |i: usize| (1..3).contains(&(i % 4)) && (1..3).contains(&(i / 4));
        let pixels: Vec<u8> =
            (0..16).flat_map(|i| if inside(i) { colour } else { [0; 4] }).collect();
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }

    fn harness(config: Config) -> Harness<'static, App> {
        Harness::builder()
            .with_size([1280.0, 820.0])
            .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin(), None))
    }

    /// Clicks the selected item's Convert button, which shows its shortcut.
    fn convert(harness: &Harness<App>) {
        harness.get(egui_kittest::kittest::by().label_contains("Ctrl+Enter")).click();
    }

    /// Steps the window until no item is busy.
    fn finish(harness: &mut Harness<App>) {
        for _ in 0..6000 {
            harness.step();
            let state = harness.state();
            let reading = state.items.iter().any(|item| {
                item.video.is_loading() || item.art.is_loading() || item.result_clip.is_loading()
            });
            if !reading
                && state.queue.is_empty()
                && state.items.iter().all(|item| item.job.is_none())
            {
                harness.run_steps(2);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the window didn't settle");
    }

    #[test]
    fn converts_pixel_art() {
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("frames");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.run_steps(2);
        harness.get_by_label_contains("Drop videos");

        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.kind, item.format), (Kind::Frames, Format::Tgs));
        assert!(item.input_thumb.is_some());
        // a folder can't be a video
        harness.get_by_label("Vector animation (TGS)");
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(item.made.as_ref().unwrap().job.status, Status::Done(_)));
        assert!(item.result_is_current() && item.result_thumb.is_some());
        assert!(dir.path().join("out").join("frames.sticker.tgs").exists());
        harness.get_by_label_contains("Telegram should accept it");

        // changing a setting makes the result old, and converting again
        // replaces the file this window wrote
        harness.state_mut().items[0].choices.target = telegram::Target::Emoji;
        harness.run_steps(2);
        assert!(!harness.state().items[0].result_is_current());
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert!(dir.path().join("out").join("frames.emoji.tgs").exists());
        assert!(harness.state().items[0].result_is_current());
    }

    fn video(dir: &Path, size: &str) -> Option<PathBuf> {
        video_of(dir, size, 1)
    }

    /// A test video of `size` lasting `seconds`.
    fn video_of(dir: &Path, size: &str, seconds: u32) -> Option<PathBuf> {
        let video = dir.join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc2=s={size}:d={seconds}"),
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skipped: needs ffmpeg on PATH");
            return None;
        }
        Some(video)
    }

    #[test]
    fn converts_a_cropped_video() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "320x240") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        {
            let item = &mut harness.state_mut().items[0];
            assert_eq!(
                (item.kind, item.format, item.input_size()),
                (Kind::Video, Format::Webm, Some((320, 240)))
            );
            item.choices.preset = Some("fast".into());
            item.choices.crop = Some(Crop { x: 40, y: 20, width: 200, height: 100 });
        }
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        let Status::Done(done) = &item.made.as_ref().unwrap().job.status else {
            panic!("{:?}", item.made.as_ref().unwrap().job.status)
        };
        let info = tgradish_core::webm::inspect_file(&done.output).unwrap();
        let picture = info.video.unwrap();
        assert_eq!((picture.width, picture.height), (512, 256));
        assert!(item.result_clip.ready().is_some());
    }

    #[test]
    fn shows_a_sharp_frame_while_paused() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "1280x720") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        assert_eq!(harness.state().items[0].input_clip().unwrap().width, 640);
        harness.state_mut().items[0].view.playing = false;
        for _ in 0..500 {
            harness.step();
            if harness.state().items[0]
                .still
                .as_ref()
                .is_some_and(|(_, still)| still.ready().is_some())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let (_, still) = harness.state().items[0].still.as_ref().expect("asked for");
        assert_eq!(still.ready().map(|clip| (clip.width, clip.height)), Some((1280, 720)));
    }

    #[test]
    fn numbers_results_of_files_with_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        square(&a.join("same.png"), [255, 0, 0, 255]);
        square(&b.join("same.png"), [0, 0, 255, 255]);
        let out = dir.path().join("out");
        let mut config = Config::default();
        config.gui.output_dir = Some(out.clone());
        let mut harness = harness(config);
        harness.state_mut().add(vec![a.join("same.png"), b.join("same.png")]);
        finish(&mut harness);
        harness.get_by_label_contains("Convert all").click();
        harness.run_steps(2);
        finish(&mut harness);
        for item in &harness.state().items {
            assert!(matches!(item.made.as_ref().unwrap().job.status, Status::Done(_)));
        }
        assert!(out.join("same.sticker.tgs").exists() && out.join("same 2.sticker.tgs").exists());
    }

    #[test]
    fn keeps_files_put_where_a_result_was() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let output = dir.path().join("art.sticker.tgs");
        assert!(output.exists());
        // someone else's file where the result was
        std::fs::remove_file(&output).unwrap();
        std::fs::write(&output, b"mine").unwrap();
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        // the result goes next to it, under a number
        let item = &harness.state().items[0];
        let made = &item.made.as_ref().unwrap().job;
        assert_eq!(made.output, dir.path().join("art 2.sticker.tgs"));
        assert!(made.output.exists() && !made.replaces);
        assert_eq!(std::fs::read(&output).unwrap(), b"mine");
    }

    #[test]
    fn replaces_results_tgradish_made_earlier() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let output = dir.path().join("art.sticker.tgs");
        // made by an earlier window, which this one knows nothing of
        for earlier in [true, false] {
            let mut harness = harness(Config::default());
            harness.state_mut().add(vec![art.clone()]);
            finish(&mut harness);
            convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            let made = &harness.state().items[0].made.as_ref().unwrap().job;
            assert_eq!(made.output, output);
            assert_eq!(made.replaces, !earlier);
        }
    }

    #[test]
    fn keeps_a_result_old_when_converting_again_fails() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert!(harness.state().items[0].result_is_current());
        // a crop outside the picture fails
        harness.state_mut().items[0].choices.crop =
            Some(Crop { x: 10, y: 10, width: 4, height: 4 });
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(item.failed.as_ref().unwrap().job.status, Status::Failed(_)));
        assert!(item.made.is_some() && !item.result_is_current());
    }

    #[test]
    fn shows_the_frame_paused_on_at_full_size() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("red-blue.mp4");
        // red until 0.48 s, then blue
        let graph = "color=c=red:s=1280x720:r=25:d=0.48[a];color=c=blue:s=1280x720:r=25:d=0.52[b];[a][b]concat=n=2:v=1:a=0";
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-filter_complex", graph, "-pix_fmt", "yuv420p"])
            .arg(&video)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skipped: needs ffmpeg on PATH");
            return;
        }
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        let item = &mut harness.state_mut().items[0];
        (item.view.playing, item.view.time) = (false, 0.45);
        for _ in 0..500 {
            harness.step();
            let ready = |item: &Item| {
                item.still.as_ref().and_then(|(_, still)| still.ready().map(|clip| clip.id))
            };
            if let Some(id) = ready(&harness.state().items[0]) {
                harness.run_steps(2);
                assert_eq!(harness.state().screen.showing(), Some(id));
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let (_, still) = harness.state().items[0].still.as_ref().expect("asked for");
        let clip = still.ready().expect("read");
        assert_eq!(clip.width, 1280);
        let pixel = &clip.frames[0][..4];
        assert!(pixel[0] > 200 && pixel[2] < 60, "the frame at 0.45 s is red, not {pixel:?}");
    }

    #[test]
    fn shows_still_images_at_full_size() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("photo.jpg");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=1280x720", "-frames:v", "1"])
            .arg(&photo)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skipped: needs ffmpeg on PATH");
            return;
        }
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![photo]);
        finish(&mut harness);
        for _ in 0..500 {
            harness.step();
            if harness.state().items[0]
                .still
                .as_ref()
                .is_some_and(|(_, still)| still.ready().is_some())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.run_steps(2);
        let (_, still) = harness.state().items[0].still.as_ref().expect("asked for");
        let clip = still.ready().expect("read");
        assert_eq!((clip.width, harness.state().screen.showing()), (1280, Some(clip.id)));
    }

    #[test]
    fn guesses_pixel_art_and_photos_apart() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!(
            (item.kind, item.format, item.format_guessed),
            (Kind::Image, Format::Tgs, false)
        );
    }

    /// Renders the window to `$TGRADISH_SCREENSHOTS/*.png` for a look at
    /// the layout: `TGRADISH_SCREENSHOTS=/tmp cargo test -p tgradish-gui
    /// screenshots -- --ignored`. Needs a GPU wgpu can use, and ffmpeg.
    #[test]
    #[ignore]
    fn screenshots() {
        let out =
            PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("dance");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        config.ffmpeg.choice = FfmpegChoice::System;
        // filtering as the window does it, so pixel art stays sharp
        let filtering = egui_wgpu::RendererOptions {
            predictable_texture_filtering: false,
            ..egui_wgpu::RendererOptions::PREDICTABLE
        };
        let mut harness = Harness::builder()
            .with_size([1280.0, 820.0])
            .with_render_options(filtering)
            .wgpu()
            .build_eframe(move |cc| {
                let app = App::with(config.clone(), None, Presets::builtin(), None);
                app.style(&cc.egui_ctx);
                app
            });
        harness.run_steps(3);
        let save = |harness: &mut Harness<App>, name: &str| {
            let image = harness.render().expect("rendering needs a GPU");
            image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
        };
        save(&mut harness, "empty");
        if let Some(video) = video(dir.path(), "640x360") {
            harness.state_mut().add(vec![video]);
            finish(&mut harness);
            harness.state_mut().items[0].choices.crop =
                Some(Crop { x: 120, y: 60, width: 320, height: 200 });
            harness.state_mut().items[0].view.playing = false;
            harness.run_steps(3);
            save(&mut harness, "video");
        }
        // names in scripts the window's fonts lack, drawn with the system's
        // with characters no font has, an unassigned one and a newer
        // emoji, and a zero width joiner
        let named = dir.path().join("日本語 العربية ภาษาไทย \u{2fe0}\u{200d}\u{1faf9}.png");
        square(&named, [0, 160, 0, 255]);
        harness.state_mut().add(vec![named]);
        finish(&mut harness);
        for _ in 0..200 {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        harness.state_mut().items.last_mut().unwrap().view.playing = false;
        harness.run_steps(3);
        save(&mut harness, "tgs");
        harness.state_mut().items.last_mut().unwrap().view.show = Show::Input;
        harness.run_steps(3);
        save(&mut harness, "tgs-input");
        harness.state_mut().prefs.open = true;
        harness.run_steps(3);
        save(&mut harness, "settings");
        harness.state_mut().prefs.open = false;
        let sticker = dir.path().join("out").join("dance.sticker.tgs");
        harness.state_mut().inspection = Some(inspect::Inspection::of(&sticker));
        harness.run_steps(3);
        save(&mut harness, "inspect");
        harness.state_mut().inspection = None;
        // a .tgs halfway through fitting
        let mut job = Job::waiting(dir.path().join("progress.tgs"));
        job.status = Status::Running;
        job.progress = jobs::Progress {
            tgs: Some(jobs::TgsStage::Fitting),
            lossless_bytes: Some(200_000),
            steps: vec![tgradish_tgs::sticker::Step {
                reduction: tgradish_tgs::reduce::Reduction::MergeColours { distance: 0.02 },
                bytes: 110_000,
                error: 0.01,
            }],
            ..jobs::Progress::default()
        };
        let item = harness.state_mut().items.last_mut().unwrap();
        item.job = Some((job, Format::Tgs, item.choices.clone()));
        harness.run_steps(3);
        save(&mut harness, "progress");
        harness.state_mut().about = true;
        harness.run_steps(3);
        save(&mut harness, "about");
    }

    #[test]
    fn warns_before_spoofing() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video_of(dir.path(), "64x48", 4) else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        harness.get_by_label_contains("will be spoofed");
        harness.get_by_label("Default");
        // cut to 3 seconds, or not spoofed at all: no warning
        harness.state_mut().items[0].choices.length = Some(3.0);
        harness.run_steps(2);
        assert!(harness.query_by_label_contains("will be spoofed").is_none());
        harness.state_mut().items[0].choices.length = None;
        harness.state_mut().items[0].choices.webm.spoof =
            Some(tgradish_core::options::Spoof::Never);
        harness.run_steps(2);
        assert!(harness.query_by_label_contains("will be spoofed").is_none());
    }

    #[test]
    fn explains_characters_no_font_has() {
        assert_eq!(missing_note(&[]), None);
        let one = missing_note(&['\u{2fe0}']).unwrap();
        assert!(one.starts_with("No font was found for U+2FE0,") && one.contains("its code"));
        let two = missing_note(&['\u{2fe0}', '\u{1faf9}', '\u{1faf9}']).unwrap();
        assert!(two.contains("U+2FE0, U+1FAF9,") && two.contains("their code"), "{two}");
        assert!(fallback::formatting('\u{200d}') && fallback::formatting('\u{fe0f}'));
        assert!(!fallback::formatting('a') && !fallback::formatting('日'));
    }

    #[test]
    fn names_results_apart() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("clipboard.sticker.tgs");
        let second = dir.path().join("clipboard 2.sticker.tgs");
        let none = HashSet::new();
        let suffix = ".sticker.tgs";
        let existing = |path: &Path| path.exists();
        let free = |_: &Path| false;
        assert_eq!(free_name(&first, suffix, &none, None, &existing), first);
        // a file in the way that may not be replaced
        std::fs::write(&first, b"").unwrap();
        assert_eq!(free_name(&first, suffix, &none, None, &existing), second);
        assert_eq!(free_name(&first, suffix, &none, None, &free), first);
        // another file's name is taken; an item's own stays its own
        let claimed = HashSet::from([first.as_path()]);
        assert_eq!(free_name(&first, suffix, &claimed, None, &free), second);
        assert_eq!(free_name(&first, suffix, &none, Some(&second), &free), second);
        assert_eq!(free_name(&first, suffix, &claimed, Some(&first), &free), second);
    }
}

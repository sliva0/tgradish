//! The tgradish window: a list of files, each with its own settings, a
//! preview to crop and trim it, and its result. It converts with
//! `tgradish-core` directly, like the CLI.

mod canvas;
#[cfg(all(unix, not(target_os = "macos")))]
mod dnd;
mod inspect;
mod item;
mod jobs;
mod media;
mod prefs;
mod settings;
mod timeline;
mod widgets;

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText, vec2};
use tgradish_core::backend::Backend;
use tgradish_core::clipboard::Pasted;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::presets::{Format, Presets};
use tgradish_core::{convert, paths, telegram, tgs};

use crate::canvas::{Cropping, Picture, Screen};
use crate::item::{Item, Kind, Made, Show};
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

/// Shows an error in a dialog, for when there is no terminal to print to.
pub fn show_error(message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("tgradish")
        .set_description(message)
        .show();
}

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
    written: HashSet<PathBuf>,
    screen: Screen,
    prefs: prefs::Prefs,
    inspection: Option<inspect::Inspection>,
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
            written: HashSet::new(),
            screen: Screen::default(),
            prefs: prefs::Prefs::default(),
            inspection: None,
            message,
            pasted: false,
            #[cfg(all(unix, not(target_os = "macos")))]
            drops: None,
            hovering: false,
            preview_share: 0.56,
        }
    }

    fn style(&self, ctx: &egui::Context) {
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

    /// Where an item's result goes.
    fn output_for(&self, item: &Item) -> PathBuf {
        let dir = self.config.gui.output_dir.clone().filter(|dir| !dir.as_os_str().is_empty());
        let input = &item.inputs[0];
        let next_to_input = match item.format {
            Format::Webm => convert::default_output(input, item.choices.target),
            Format::Tgs => tgs::default_output(input, item.choices.target),
        };
        if item.pasted.is_some() {
            // not next to the image, which is in a temporary directory
            let dir = dir.or_else(paths::pictures_dir).unwrap_or_default();
            let own = item.made.as_ref().map(|made| made.job.output.as_path());
            return pasted_output(&next_to_input, input, &dir, own);
        }
        match (dir, next_to_input.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => next_to_input,
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
            if reads_art
                && (item.art.is_idle()
                    || (item.art_reading != item.choices.reading() && !item.art.is_loading()))
            {
                item.art_reading = item.choices.reading();
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
            if let Status::Done(done) = &job.status {
                let output = done.output.clone();
                item.result_thumb = None;
                item.result_clip = match &done.preview {
                    Some(preview) => Load::Ready(Clip::from_preview(preview.clone(), true)),
                    None => Load::Idle,
                };
                if selected {
                    item.view.show = Show::Result;
                }
                self.written.insert(output);
            }
            let item = self.item_mut(id).expect("just found");
            item.made = Some(Made { job, format, choices });
            if item.result_clip.is_idle()
                && matches!(item.made.as_ref().map(|made| &made.job.status), Some(Status::Done(_)))
            {
                load_result_clip(ctx, item, backend);
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
            let overwrite = self.config.gui.overwrite || self.written.contains(&job.output);
            let (inputs, sequence) = (item.inputs.clone(), item.sequence);
            let item = self.item_mut(id).expect("found above");
            let (job, ..) = item.job.as_mut().expect("found above");
            match plan {
                Ok(plan) => {
                    job.start(plan, inputs, sequence, overwrite, ctx);
                    return;
                }
                Err(message) => {
                    job.status = Status::Failed(message);
                }
            }
        }
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
                .add_filter("images", &["png", "gif", "webp", "ase", "aseprite"])
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
            for item in &self.items {
                let selected = self.selected == Some(item.id);
                let queued = self.queue.iter().position(|&id| id == item.id);
                let (response, remove) = row(ui, item, selected, queued);
                if remove {
                    removed = Some(item.id);
                } else if response.clicked() {
                    clicked = Some(item.id);
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
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(8.0);
            self.actions(ui, id);
            ui.add_space(6.0);
        });
        let context =
            settings::Context { presets: &self.presets, config: &self.config, extra_args };
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
                        preview(ui, &mut self.screen, item);
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
                                summary(ui, item);
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
                    let progress = &job.progress;
                    let bar = match progress.fraction {
                        Some(fraction) => egui::ProgressBar::new(fraction).text(&progress.stage),
                        None => egui::ProgressBar::new(0.0).animate(true).text(&progress.stage),
                    };
                    ui.add(bar.desired_width(ui.available_width().min(420.0)));
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
                    if let Some(made) = &item.made {
                        match &made.job.status {
                            Status::Failed(message) => {
                                ui.colored_label(widgets::BAD, format!("Failed: {message}"));
                            }
                            Status::Exists(_) => {
                                ui.colored_label(
                                    widgets::WARN,
                                    "A file not made here is in the way:",
                                );
                                if ui.button("Replace it").clicked() {
                                    replace = Some(made.job.output.clone());
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
                ui.label(RichText::new(name).strong()).on_hover_text(output.display().to_string());
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Saves to {folder}{}", std::path::MAIN_SEPARATOR))
                            .weak(),
                    )
                    .truncate(),
                )
                .on_hover_text("Settings chooses where results go");
            });
        });
        let shortcut =
            ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));
        if let Some(path) = replace {
            self.written.insert(path);
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
fn preview(ui: &mut egui::Ui, screen: &mut Screen, item: &mut Item) {
    let has_result = item.result_clip.ready().is_some();
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
            if item.view.zoom > 1.0 {
                if ui.small_button("Fit").clicked() {
                    item.view.zoom = 1.0;
                }
                ui.label(format!("{:.0}%", item.view.zoom * 100.0));
            } else {
                widgets::note(
                    ui,
                    match (item.view.show, item.choices.crop.is_some()) {
                        (Show::Input, false) => {
                            "Drag to crop · wheel to zoom · right button to pan"
                        }
                        (Show::Input, true) => {
                            "Drag the crop or its edges · double-click for the whole picture"
                        }
                        (Show::Result, _) => "Wheel to zoom · right button to pan",
                    },
                );
            }
        });
    });

    let length = item.input_length();
    let timeline_height =
        if length.is_some() && item.view.show == Show::Input { 38.0 } else { 0.0 };
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
    let filling = (item.format == Format::Webm
        && item.choices.webm.resize == Some(tgradish_core::options::Resize::Crop))
    .then(|| {
        let (width, height) = item.choices.target.box_size();
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
                let still =
                    still.filter(|(at, _)| !item.view.playing && (*at == time || range.is_none()));
                let clip = still.map(|(_, still)| still).or(part).unwrap_or(clip);
                let picture = Picture { clip, frame: clip.index_at(time), size, square: false };
                let ratio = settings::ratio(item.view.aspect, size);
                let cropping = Cropping { crop: &mut item.choices.crop, ratio, filling };
                canvas::show(ui, screen, rect, &picture, &mut item.view, Some(cropping));
            } else {
                message_in(ui, rect, "Reading…", ui.visuals().weak_text_color());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        Show::Result => {
            if let Some(clip) = item.result_clip.ready() {
                let length = clip.end().max(1e-3);
                if item.view.playing && clip.is_animated() {
                    item.view.time += dt;
                    ui.ctx().request_repaint();
                }
                let start = range.map_or(0.0, |(start, _)| start);
                let at = (item.view.time - start).rem_euclid(length);
                let square = item.made.as_ref().is_some_and(|made| made.format == Format::Tgs);
                let picture = Picture {
                    clip,
                    frame: clip.index_at(at),
                    size: (clip.width, clip.height),
                    square,
                };
                canvas::show(ui, screen, rect, &picture, &mut item.view, None);
            }
        }
    }

    if timeline_height > 0.0
        && let Some(length) = length
    {
        ui.add_space(6.0);
        timeline::show(
            ui,
            timeline::Timeline {
                length,
                start: &mut item.choices.start,
                used: &mut item.choices.length,
                time: &mut item.view.time,
                playing: &mut item.view.playing,
                limit: telegram::MAX_SECONDS,
            },
        );
    }
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

/// One file in the list: its input, name and state, and its result. Also
/// whether its remove button was clicked.
fn row(
    ui: &mut egui::Ui,
    item: &Item,
    selected: bool,
    queued: Option<usize>,
) -> (egui::Response, bool) {
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
    if item.result_thumb.is_some() {
        thumb(ui, result, item.result_thumb.as_ref(), false, !item.result_is_current());
    }
    let mut text = egui::Rect::from_min_max(
        egui::pos2(input.max.x + 10.0, inner.min.y),
        egui::pos2(result.min.x - 8.0, inner.max.y),
    );
    let mut remove = false;
    if response.hovered() || response.contains_pointer() {
        let button =
            egui::Rect::from_min_size(egui::pos2(text.max.x - 20.0, text.min.y), vec2(20.0, 20.0));
        text.max.x = button.min.x - 4.0;
        let clicked = ui
            .put(button, egui::Button::new("✖").small().frame(false))
            .on_hover_text("Remove it from the list")
            .clicked();
        remove = clicked;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(text).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 2.0;
    child.add(egui::Label::new(RichText::new(item.name()).strong()).truncate().selectable(false));
    let what = match (item.format, item.choices.target) {
        (Format::Webm, telegram::Target::Sticker) => "WebM sticker",
        (Format::Webm, telegram::Target::Emoji) => "WebM emoji",
        (Format::Tgs, telegram::Target::Sticker) => "TGS sticker",
        (Format::Tgs, telegram::Target::Emoji) => "TGS emoji",
    };
    child.add(egui::Label::new(RichText::new(what).small().weak()).selectable(false));
    status(&mut child, item, queued);
    let response = response.on_hover_text(
        item.inputs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"),
    );
    (response, remove)
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
            if stale {
                let badge =
                    egui::Rect::from_min_size(rect.right_top() - vec2(18.0, 0.0), vec2(18.0, 18.0));
                painter.rect_filled(badge, 4.0, widgets::WARN);
                painter.text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    "⟳",
                    egui::FontId::proportional(13.0),
                    Color32::BLACK,
                );
            }
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
    match (&item.job, &item.made) {
        (Some((job, ..)), _) if job.is_running() => {
            let bar = match job.progress.fraction {
                Some(fraction) => egui::ProgressBar::new(fraction),
                None => egui::ProgressBar::new(0.0).animate(true),
            };
            ui.add(bar.desired_height(6.0).desired_width(ui.available_width()));
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
fn summary(ui: &mut egui::Ui, item: &Item) {
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

/// Where the result of a pasted image goes: in `dir`, under a name nothing
/// has yet, since every pasted image has the same name, or the one its
/// `own` earlier result has.
fn pasted_output(next_to_input: &Path, image: &Path, dir: &Path, own: Option<&Path>) -> PathBuf {
    let stem = image.file_stem().unwrap_or_default().to_string_lossy();
    let name = next_to_input.file_name().unwrap_or_default().to_string_lossy();
    let rest = name.strip_prefix(&*stem).unwrap_or(&name);
    std::iter::once(dir.join(&*name))
        .chain((2..).map(|n| dir.join(format!("{stem} {n}{rest}"))))
        .find(|path| !path.exists() || Some(path.as_path()) == own)
        .expect("some name is free")
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_input(&ctx);
        self.pump(&ctx);
        if let Some(message) = self.message.clone() {
            egui::Panel::bottom("message").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(widgets::WARN, &message);
                    if ui.small_button("OK").clicked() {
                        self.message = None;
                    }
                });
            });
        }
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
        let video = dir.join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc2=s={size}:d=1"),
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
    }

    #[test]
    fn names_pasted_results_apart() {
        let dir = tempfile::tempdir().unwrap();
        let image = Path::new("/tmp/somewhere/clipboard.png");
        let next_to = tgs::default_output(image, telegram::Target::Sticker);
        let first = pasted_output(&next_to, image, dir.path(), None);
        assert_eq!(first, dir.path().join("clipboard.sticker.tgs"));
        std::fs::write(&first, b"").unwrap();
        assert_eq!(
            pasted_output(&next_to, image, dir.path(), None),
            dir.path().join("clipboard 2.sticker.tgs")
        );
        // its own result it replaces
        assert_eq!(pasted_output(&next_to, image, dir.path(), Some(&first)), first);
    }
}

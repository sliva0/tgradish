//! The tgradish window: choose a format and preset, adjust options, add
//! files by dropping, pasting or picking them, convert, and preview the
//! results. It converts with `tgradish-core` directly, like the CLI.

mod form;
mod jobs;
mod preview;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use serde_json::{Map, Value};
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::{CancelToken, FfmpegChoice};
use tgradish_core::options::Options;
use tgradish_core::presets::{Format, Presets};
use tgradish_core::tgs::{self, TgsOptions};
use tgradish_core::{telegram, webm};

use crate::form::Form;
use crate::jobs::{Job, Plan, Status};
use crate::preview::Player;

const FORMATS: [Format; 2] = [Format::Webm, Format::Tgs];

fn format_label(format: Format) -> &'static str {
    match format {
        Format::Webm => "Video sticker (WebM)",
        Format::Tgs => "Animated sticker from pixel art (TGS)",
    }
}

/// Opens the window and runs until it is closed.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tgradish")
            .with_inner_size([1060.0, 720.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("tgradish", options, Box::new(|_| Ok(Box::new(App::new()))))
}

/// The preset and options of one format.
struct FormatState {
    preset: String,
    /// Options the user set over the preset's, as JSON.
    values: Map<String, Value>,
    form: Form,
}

struct App {
    config_path: Option<PathBuf>,
    config: Config,
    presets: Presets,
    format: Format,
    states: HashMap<Format, FormatState>,
    /// Jobs with ids, which previews are kept by.
    jobs: Vec<(usize, Job)>,
    next_id: usize,
    selected: Option<usize>,
    /// Working through the queue.
    running: bool,
    /// Add several images at once as the frames of one sticker.
    join: bool,
    players: HashMap<usize, Player>,
    settings_open: bool,
    ffmpeg_status: Option<String>,
    inspection: Option<(PathBuf, String)>,
    message: Option<String>,
}

impl App {
    /// The window with the user's config and presets.
    fn new() -> App {
        let config_path = Config::default_path();
        let mut message = None;
        let config = match config_path.as_deref().map(Config::load) {
            Some(Ok(config)) => config,
            Some(Err(err)) => {
                message = Some(format!("the config couldn't be read, using defaults: {err}"));
                Config::default()
            }
            None => Config::default(),
        };
        let presets = Presets::load_user().unwrap_or_else(|err| {
            message = Some(format!("presets couldn't be read: {err}"));
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
        let states = FORMATS
            .into_iter()
            .map(|format| {
                let schema = match format {
                    Format::Webm => schemars::schema_for!(Options),
                    Format::Tgs => schemars::schema_for!(TgsOptions),
                };
                let schema = serde_json::to_value(schema).expect("schemas serialize");
                let state = FormatState {
                    preset: config.preset_for(format).to_owned(),
                    values: Map::new(),
                    form: Form::from_schema(&schema),
                };
                (format, state)
            })
            .collect();
        App {
            config_path,
            config,
            presets,
            format: Format::Webm,
            states,
            jobs: Vec::new(),
            next_id: 0,
            selected: None,
            running: false,
            join: false,
            players: HashMap::new(),
            settings_open: false,
            ffmpeg_status: None,
            inspection: None,
            message,
        }
    }

    /// The options a preset sets, with everything it extends, as JSON.
    fn preset_values(&self, name: &str) -> Map<String, Value> {
        let resolved = self.presets.resolve(name).ok();
        let value = resolved.map(|options| serde_json::to_value(options).unwrap_or_default());
        value.and_then(|value| value.as_object().cloned()).unwrap_or_default()
    }

    /// What a job started now would convert with.
    fn plan(&self) -> Result<Plan, String> {
        let state = &self.states[&self.format];
        let mut options = self.preset_values(&state.preset);
        options.extend(state.values.clone());
        let options = Value::Object(options);
        Ok(match self.format {
            Format::Webm => {
                let options: Options =
                    serde_json::from_value(options).map_err(|err| err.to_string())?;
                let ffmpeg = &self.config.ffmpeg;
                let backend = Backend::select(ffmpeg.choice, ffmpeg.path.as_deref())
                    .map_err(|err| err.to_string())?;
                Plan::Webm { options, backend }
            }
            Format::Tgs => {
                let options: TgsOptions =
                    serde_json::from_value(options).map_err(|err| err.to_string())?;
                Plan::Tgs { options }
            }
        })
    }

    /// Adds files to the queue: each its own job, or with `join` the frames
    /// of one sticker. Directories are frames of one sticker.
    fn add(&mut self, paths: Vec<PathBuf>) {
        let (dirs, files): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.into_iter().partition(|p| p.is_dir());
        let mut jobs = Vec::new();
        for dir in dirs {
            if self.format == Format::Tgs {
                jobs.push(Job::new(vec![dir], true));
            } else {
                self.message = Some(format!(
                    "{} is a folder; folders of frames make TGS stickers",
                    dir.display()
                ));
            }
        }
        if self.join && self.format == Format::Tgs && files.len() > 1 {
            jobs.push(Job::new(files, true));
        } else {
            jobs.extend(files.into_iter().map(|file| Job::new(vec![file], false)));
        }
        for job in jobs {
            self.jobs.push((self.next_id, job));
            self.selected = Some(self.next_id);
            self.next_id += 1;
        }
    }

    /// Takes in progress and starts the next job when the queue is running.
    fn pump(&mut self, ctx: &egui::Context) {
        for (_, job) in &mut self.jobs {
            job.poll();
        }
        if !self.running || self.jobs.iter().any(|(_, job)| job.is_running()) {
            return;
        }
        let Some(index) = self.jobs.iter().position(|(_, job)| job.status == Status::Waiting)
        else {
            self.running = false;
            return;
        };
        match self.plan() {
            Ok(plan) => {
                let output = plan
                    .output(&self.jobs[index].1.inputs[0], self.config.gui.output_dir.as_deref());
                self.jobs[index].1.start(plan, output, self.config.gui.overwrite, ctx.clone());
            }
            Err(err) => {
                self.jobs[index].1.status = Status::Failed(err);
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for format in FORMATS {
                ui.selectable_value(&mut self.format, format, format_label(format));
            }
            ui.separator();
            ui.label("Preset");
            let format = self.format;
            let names: Vec<(String, String)> = self
                .presets
                .iter()
                .filter(|(name, _)| self.presets.format(name).is_ok_and(|f| f == format))
                .map(|(name, preset)| {
                    (name.to_owned(), preset.map(|p| p.description.clone()).unwrap_or_default())
                })
                .collect();
            let state = self.states.get_mut(&format).expect("every format has a state");
            egui::ComboBox::from_id_salt("preset").selected_text(&state.preset).show_ui(ui, |ui| {
                for (name, description) in &names {
                    let picked = ui.selectable_value(&mut state.preset, name.clone(), name);
                    if !description.is_empty() {
                        picked.on_hover_text(description);
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    self.settings_open = true;
                }
                if ui.button("Inspect a sticker…").clicked() {
                    self.inspect_picked();
                }
            });
        });
    }

    fn options_panel(&mut self, ui: &mut egui::Ui) {
        let base = self.preset_values(&self.states[&self.format].preset);
        let state = self.states.get_mut(&self.format).expect("every format has a state");
        ui.horizontal(|ui| {
            ui.heading("Options");
            if !state.values.is_empty() && ui.button("Back to the preset").clicked() {
                state.values.clear();
                state.form.reset_texts();
            }
        });
        ui.label(
            egui::RichText::new(
                "Hover a name for what it does. Empty fields take the preset's value.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            state.form.show(ui, &mut state.values, &base);
        });
    }

    fn queue(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Add files…").clicked()
                && let Some(files) = rfd::FileDialog::new().pick_files()
            {
                self.add(files);
            }
            if self.format == Format::Tgs {
                if ui.button("Add a folder of frames…").clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.add(vec![dir]);
                }
                ui.checkbox(&mut self.join, "Several images make one sticker").on_hover_text(
                    "Files added together become the frames of one sticker, in name order",
                );
            }
            ui.separator();
            let waiting = self.jobs.iter().any(|(_, job)| job.status == Status::Waiting);
            if self.running {
                if ui.button("Stop").on_hover_text("Finish the current file, then stop").clicked() {
                    self.running = false;
                }
            } else if ui.add_enabled(waiting, egui::Button::new("Convert")).clicked() {
                self.running = true;
            }
            if ui.button("Clear finished").clicked() {
                self.jobs.retain(|(_, job)| !job.is_finished());
                let ids: Vec<usize> = self.jobs.iter().map(|(id, _)| *id).collect();
                self.players.retain(|id, _| ids.contains(id));
            }
        });
        ui.separator();
        if self.jobs.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(
                        "Drop videos, images or pixel art here,\nor paste their paths",
                    )
                    .size(18.0)
                    .weak(),
                );
            });
            return;
        }
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("queue")
            .max_height(ui.available_height() * 0.45)
            .show(ui, |ui| {
                for (id, job) in &self.jobs {
                    ui.horizontal(|ui| {
                        let selected = self.selected == Some(*id);
                        if ui.selectable_label(selected, job.name()).clicked() {
                            self.selected = Some(*id);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if job.is_running() {
                                if ui.small_button("Cancel").clicked() {
                                    job.cancel();
                                }
                            } else if ui.small_button("Remove").clicked() {
                                remove = Some(*id);
                            }
                            status_line(ui, &job.status);
                        });
                    });
                }
            });
        if let Some(id) = remove {
            self.jobs.retain(|(other, _)| *other != id);
            self.players.remove(&id);
        }
        ui.separator();
        self.details(ui);
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected else { return };
        let Some((_, job)) = self.jobs.iter().find(|(other, _)| *other == id) else { return };
        ui.columns(2, |columns| {
            let ui = &mut columns[0];
            if let Some(output) = &job.output {
                ui.horizontal(|ui| {
                    ui.label(format!("Result: {}", output.display()));
                    if matches!(job.status, Status::Done { .. })
                        && ui.small_button("Show").clicked()
                    {
                        jobs::reveal(output);
                    }
                });
            }
            match &job.status {
                Status::Done { bytes, limit, lossy, issues } => {
                    let size = format!(
                        "{:.1} KiB, {:.0}% of Telegram's limit{}",
                        *bytes as f64 / 1024.0,
                        *bytes as f64 * 100.0 / *limit as f64,
                        if *lossy { ", changed to fit" } else { "" }
                    );
                    ui.label(size);
                    if issues.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 170, 90),
                            "Telegram should accept it",
                        );
                    }
                    for issue in issues {
                        ui.colored_label(ui.visuals().error_fg_color, issue);
                    }
                }
                Status::Failed(message) => {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
                _ => {}
            }
            egui::ScrollArea::vertical().id_salt("log").show(ui, |ui| {
                for line in &job.log {
                    ui.label(egui::RichText::new(line).monospace());
                }
            });
            let ui = &mut columns[1];
            if let Some(preview) = &job.preview {
                let player = self
                    .players
                    .entry(id)
                    .or_insert_with(|| Player::new(ui.ctx(), &format!("job-{id}"), preview));
                let side = ui.available_width().min(ui.available_height()).clamp(64.0, 360.0);
                player.show(ui, side);
            }
        });
    }

    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Settings").open(&mut open).resizable(false).show(ctx, |ui| {
            egui::Grid::new("settings").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("ffmpeg");
                let ffmpeg = &mut self.config.ffmpeg;
                egui::ComboBox::from_id_salt("ffmpeg-choice")
                    .selected_text(match ffmpeg.choice {
                        FfmpegChoice::Auto => "built in if there is one, else the system's",
                        FfmpegChoice::Builtin => "built in",
                        FfmpegChoice::System => "the system's",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut ffmpeg.choice,
                            FfmpegChoice::Auto,
                            "built in if there is one, else the system's",
                        );
                        ui.selectable_value(&mut ffmpeg.choice, FfmpegChoice::Builtin, "built in");
                        ui.selectable_value(
                            &mut ffmpeg.choice,
                            FfmpegChoice::System,
                            "the system's",
                        );
                    });
                ui.end_row();
                ui.label("ffmpeg program");
                ui.horizontal(|ui| {
                    let mut text =
                        ffmpeg.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                    let edit = egui::TextEdit::singleline(&mut text)
                        .hint_text("found on PATH")
                        .desired_width(240.0);
                    if ui.add(edit).changed() {
                        ffmpeg.path = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
                    }
                    if ui.button("Choose…").clicked()
                        && let Some(file) = rfd::FileDialog::new().pick_file()
                    {
                        ffmpeg.path = Some(file);
                    }
                });
                ui.end_row();
                ui.label("");
                if ui.button("Check ffmpeg").clicked() {
                    self.ffmpeg_status = Some(check_ffmpeg(&self.config));
                }
                ui.end_row();
                if let Some(status) = &self.ffmpeg_status {
                    ui.label("");
                    ui.label(status);
                    ui.end_row();
                }

                ui.label("Results");
                let gui = &mut self.config.gui;
                ui.vertical(|ui| {
                    let mut elsewhere = gui.output_dir.is_some();
                    ui.radio_value(&mut elsewhere, false, "next to each input");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut elsewhere, true, "in");
                        let mut text = gui
                            .output_dir
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        if ui
                            .add(egui::TextEdit::singleline(&mut text).desired_width(200.0))
                            .changed()
                        {
                            gui.output_dir = Some(PathBuf::from(text.trim()));
                            elsewhere = true;
                        }
                        if ui.button("Choose…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            gui.output_dir = Some(dir);
                            elsewhere = true;
                        }
                    });
                    if !elsewhere {
                        gui.output_dir = None;
                    } else if gui.output_dir.is_none() {
                        gui.output_dir = Some(PathBuf::new());
                    }
                    ui.checkbox(&mut gui.overwrite, "Replace results that already exist");
                });
                ui.end_row();

                for format in FORMATS {
                    ui.label(format!("Starting preset, {}", format.extension()));
                    let current = self.config.preset_for(format).to_owned();
                    let mut picked = current.clone();
                    egui::ComboBox::from_id_salt(("default-preset", format.extension()))
                        .selected_text(&picked)
                        .show_ui(ui, |ui| {
                            for (name, _) in self.presets.iter() {
                                if self.presets.format(name).is_ok_and(|f| f == format) {
                                    ui.selectable_value(&mut picked, name.to_owned(), name);
                                }
                            }
                        });
                    if picked != current {
                        match format {
                            Format::Webm => self.config.preset = Some(picked),
                            Format::Tgs => self.config.tgs_preset = Some(picked),
                        }
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.message = Some(match &self.config_path {
                        Some(path) => match self.config.save(path) {
                            Ok(()) => format!("settings saved to {}", path.display()),
                            Err(err) => format!("settings couldn't be saved: {err}"),
                        },
                        None => "no place to save settings: no home directory".to_owned(),
                    });
                }
                if let Some(path) = &self.config_path {
                    ui.label(egui::RichText::new(path.display().to_string()).weak());
                }
            });
        });
        self.settings_open = open;
    }

    fn inspect_picked(&mut self) {
        let Some(file) =
            rfd::FileDialog::new().add_filter("stickers", &["webm", "tgs"]).pick_file()
        else {
            return;
        };
        let text = inspect(&file);
        self.inspection = Some((file, text));
    }

    fn inspection(&mut self, ctx: &egui::Context) {
        let Some((file, text)) = &self.inspection else { return };
        let mut open = true;
        let title = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        egui::Window::new(title).open(&mut open).default_size([520.0, 420.0]).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(egui::RichText::new(text).monospace());
            });
        });
        if !open {
            self.inspection = None;
        }
    }

    /// Files dropped on the window, or paths pasted as text.
    fn take_input(&mut self, ctx: &egui::Context) {
        let (dropped, pasted) = ctx.input(|input| {
            let dropped: Vec<PathBuf> =
                input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect();
            let pasted: Vec<PathBuf> = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Paste(text) => Some(text.clone()),
                    _ => None,
                })
                .flat_map(|text| paths_in(&text))
                .collect();
            (dropped, pasted)
        });
        // pasting into a text field is just text
        let typing = ctx.memory(|memory| memory.focused().is_some());
        let mut paths = dropped;
        if !typing {
            paths.extend(pasted);
        }
        if !paths.is_empty() {
            self.add(paths);
        }
    }
}

/// Paths in pasted text, one per line, that exist.
fn paths_in(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(|line| line.trim().trim_matches('"'))
        .map(|line| line.strip_prefix("file://").unwrap_or(line))
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

fn status_line(ui: &mut egui::Ui, status: &Status) {
    match status {
        Status::Waiting => {
            ui.label(egui::RichText::new("waiting").weak());
        }
        Status::Running { stage, fraction } => {
            let bar = match fraction {
                Some(fraction) => egui::ProgressBar::new(*fraction).text(stage),
                None => egui::ProgressBar::new(0.0).animate(true).text(stage),
            };
            ui.add(bar.desired_width(260.0));
        }
        Status::Done { bytes, issues, .. } => {
            let text = format!("{:.1} KiB", *bytes as f64 / 1024.0);
            if issues.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(80, 170, 90), format!("done, {text}"));
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("done, {text}, with problems"),
                );
            }
        }
        Status::Failed(_) => {
            ui.colored_label(ui.visuals().error_fg_color, "failed");
        }
        Status::Cancelled => {
            ui.label(egui::RichText::new("cancelled").weak());
        }
    }
}

fn check_ffmpeg(config: &Config) -> String {
    let backend = match Backend::select(config.ffmpeg.choice, config.ffmpeg.path.as_deref()) {
        Ok(backend) => backend,
        Err(err) => return err.to_string(),
    };
    match backend.capabilities(&CancelToken::new()) {
        Ok(caps) if caps.libvpx_vp9 => format!("ffmpeg {} with libvpx-vp9: ready", caps.version),
        Ok(caps) => {
            format!("ffmpeg {} can't encode VP9 with libvpx, which stickers need", caps.version)
        }
        Err(err) => err.to_string(),
    }
}

/// What `tgradish inspect` says about a file, as text.
fn inspect(path: &Path) -> String {
    let pretty = |value: &dyn erased::Serialize| {
        serde_json::to_string_pretty(&value.to_value()).unwrap_or_default()
    };
    if tgs::is_sticker(path) {
        return match tgs::inspect_file(path) {
            Ok((stats, issues)) => {
                let mut text = pretty(&stats);
                text.push_str("\n\n");
                if issues.is_empty() {
                    text.push_str("Telegram should accept it.");
                }
                for issue in issues {
                    text.push_str(&format!("{:?}: {}\n", issue.severity, issue.message));
                }
                text
            }
            Err(err) => err.to_string(),
        };
    }
    match webm::inspect_file(path) {
        Ok(info) => {
            let target = match &info.video {
                Some(video) if video.width == 100 && video.height == 100 => telegram::Target::Emoji,
                _ => telegram::Target::Sticker,
            };
            let mut text = format!("as a {}:\n", target.name());
            text.push_str(&pretty(&info));
            text.push_str("\n\n");
            let issues = telegram::check(&info, target);
            if issues.is_empty() {
                text.push_str("Telegram should accept it.");
            }
            for issue in issues {
                text.push_str(&format!("{issue}\n"));
            }
            text
        }
        Err(err) => err.to_string(),
    }
}

/// Serializes values of different types through one function.
mod erased {
    pub trait Serialize {
        fn to_value(&self) -> serde_json::Value;
    }

    impl<T: serde::Serialize> Serialize for T {
        fn to_value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or_default()
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_input(&ctx);
        self.pump(&ctx);
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        if let Some(message) = self.message.clone() {
            egui::Panel::bottom("message").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&message);
                    if ui.small_button("OK").clicked() {
                        self.message = None;
                    }
                });
            });
        }
        egui::Panel::left("options").resizable(true).default_size(400.0).show(ui, |ui| {
            self.options_panel(ui);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.queue(ui);
        });
        self.settings(&ctx);
        self.inspection(&ctx);
        if ctx.input(|input| !input.raw.hovered_files.is_empty()) {
            let painter =
                ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, "drop".into()));
            let screen = ctx.content_rect();
            painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                "Drop to add",
                egui::FontId::proportional(28.0),
                egui::Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

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

    #[test]
    fn converts_pixel_art() {
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("frames");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);

        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = Harness::builder()
            .with_size([1100.0, 760.0])
            .build_eframe(|_| App::with(config, None, Presets::builtin(), None));
        harness.run_steps(2);
        harness.get_by_label("Video sticker (WebM)");
        // WebM's options, then TGS's
        harness.get_by_label("CRF");
        harness.get_by_label("Animated sticker from pixel art (TGS)").click();
        harness.run_steps(2);
        harness.get_by_label("Reductions");
        assert!(harness.query_by_label("CRF").is_none());

        harness.state_mut().add(vec![frames]);
        harness.run_steps(2);
        harness.get_by_label("Convert").click();
        for _ in 0..1000 {
            harness.step();
            if harness.state().jobs.iter().all(|(_, job)| job.is_finished()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let job = &harness.state().jobs[0].1;
        assert!(matches!(job.status, Status::Done { .. }), "{:?}", job.status);
        assert!(job.preview.as_ref().is_some_and(|p| p.frames.len() == 2));
        assert!(dir.path().join("out").join("frames.sticker.tgs").exists());
        harness.run_steps(2);
        harness.get_by_label_contains("Telegram should accept it");
    }

    /// Renders the window to `$TGRADISH_SCREENSHOTS/*.png` for a look at
    /// the layout: `TGRADISH_SCREENSHOTS=/tmp cargo test -p tgradish-gui
    /// screenshots -- --ignored`. Needs a GPU wgpu can use.
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
        let mut harness = Harness::builder()
            .with_size([1100.0, 760.0])
            .wgpu()
            .build_eframe(|_| App::with(config, None, Presets::builtin(), None));
        harness.run_steps(3);
        let save = |harness: &mut Harness<App>, name: &str| {
            let image = harness.render().expect("rendering needs a GPU");
            image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
        };
        save(&mut harness, "webm");
        harness.get_by_label("Animated sticker from pixel art (TGS)").click();
        harness.run_steps(3);
        harness.state_mut().add(vec![frames]);
        harness.get_by_label("Convert").click();
        for _ in 0..1000 {
            harness.step();
            if harness.state().jobs.iter().all(|(_, job)| job.is_finished()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.run_steps(3);
        save(&mut harness, "tgs");
    }

    /// Steps the window until every job is finished.
    fn finish(harness: &mut Harness<App>) {
        for _ in 0..6000 {
            harness.step();
            if harness.state().jobs.iter().all(|(_, job)| job.is_finished()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("jobs didn't finish");
    }

    #[test]
    fn converts_video() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:d=1",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skipped: needs ffmpeg on PATH");
            return;
        }
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = Harness::builder()
            .with_size([1100.0, 760.0])
            .build_eframe(|_| App::with(config, None, Presets::builtin(), None));
        harness.run_steps(2);
        harness.state_mut().states.get_mut(&Format::Webm).unwrap().preset = "fast".into();
        harness.state_mut().add(vec![video]);
        harness.run_steps(2);
        harness.get_by_label("Convert").click();
        finish(&mut harness);
        let job = &harness.state().jobs[0].1;
        assert!(matches!(job.status, Status::Done { .. }), "{:?}", job.status);
        assert!(job.preview.as_ref().is_some_and(|preview| !preview.frames.is_empty()));
        assert!(dir.path().join("clip.sticker.webm").exists());
    }

    #[test]
    fn finds_paths_in_pasted_text() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a b.gif");
        std::fs::write(&file, b"x").unwrap();
        let text = format!(
            "{}\nnot a file\n\"{}\"\nfile://{}",
            file.display(),
            file.display(),
            file.display()
        );
        assert_eq!(paths_in(&text), vec![file.clone(), file.clone(), file]);
    }
}

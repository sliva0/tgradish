//! What a sticker file holds and what Telegram would say about it.

use std::path::{Path, PathBuf};

use eframe::egui;
use tgradish_core::inspect::{self, Inspected};
use tgradish_core::still;
use tgradish_core::telegram::{self, Target};
use tgradish_core::webm::WebmInfo;
use tgradish_tgs::check::Stats;
use tgradish_tgs::limits::{self, telegram as server};

use crate::app::App;
use crate::fonts::drawable_in;
use crate::jobs::Problem;
use crate::l10n::fl;
use crate::widgets::{self, gauge, label, note, problems};

pub enum Found {
    Webm { info: Box<WebmInfo>, target: Target },
    Tgs(Box<Stats>),
    Image { info: still::Info, target: Target },
    Failed(String),
}

pub struct Inspection {
    pub path: PathBuf,
    pub found: Found,
    pub problems: Vec<Problem>,
}

impl Inspection {
    pub fn of(path: &Path) -> Inspection {
        let (found, problems) = match inspect::inspect(path, None) {
            Ok(Inspected::Image { target, info, issues }) => {
                let problems = issues.iter().map(|issue| Problem::refused(issue, target));
                (Found::Image { info, target }, problems.collect())
            }
            Ok(Inspected::Tgs { stats, issues }) => {
                (Found::Tgs(stats), issues.into_iter().map(Problem::of_tgs).collect())
            }
            Ok(Inspected::Webm { target, info, issues }) => {
                let problems = issues.iter().map(|issue| Problem::refused(issue, target));
                (Found::Webm { info, target }, problems.collect())
            }
            Err(err) => (Found::Failed(err.to_string()), Vec::new()),
        };
        Inspection { path: path.to_path_buf(), found, problems }
    }
}

fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).min_col_width(150.0).spacing([12.0, 7.0]).show(ui, rows);
}

/// A row of how much of a limit something uses.
fn budget(ui: &mut egui::Ui, name: &str, hint: &str, used: u64, limit: u64, text: String) {
    label(ui, name, hint);
    ui.scope(|ui| {
        ui.set_width(300.0);
        gauge(ui, used, limit, text);
    });
    ui.end_row();
}

fn of(used: impl std::fmt::Display, limit: impl std::fmt::Display) -> String {
    fl!("of-limit", used = used.to_string(), limit = limit.to_string())
}

pub fn show(ui: &mut egui::Ui, inspection: &Inspection) {
    match &inspection.found {
        Found::Failed(message) => {
            ui.colored_label(widgets::BAD, message);
            return;
        }
        Found::Webm { info, target } => webm(ui, info, *target),
        Found::Tgs(stats) => animation(ui, stats),
        Found::Image { info, target } => image(ui, info, *target),
    }
    ui.add_space(8.0);
    problems(ui, &inspection.problems);
}

fn webm(ui: &mut egui::Ui, info: &WebmInfo, target: Target) {
    note(
        ui,
        match target {
            Target::Sticker => fl!("inspect-webm-sticker"),
            Target::Emoji => fl!("inspect-webm-emoji"),
        },
    );
    ui.add_space(6.0);
    grid(ui, "inspect-webm", |ui| {
        let limit = target.max_bytes();
        let size = of(widgets::kib(info.file_size), widgets::kib(limit));
        let (name, hint) = (fl!("inspect-size"), fl!("inspect-size", "hint"));
        budget(ui, &name, &hint, info.file_size, limit, size);
        let max = telegram::MAX_SECONDS;
        match (info.header_duration, info.content_duration) {
            (Some(header), content) => {
                let text = if info.spoofed() {
                    fl!("inspect-duration", "spoofed", seconds = format!("{header:.3}"))
                } else {
                    of(widgets::seconds(header), widgets::seconds(max))
                };
                let (name, hint) = (fl!("inspect-duration"), fl!("inspect-duration", "hint"));
                let (used, most) = ((header * 1000.0) as u64, (max * 1000.0) as u64);
                budget(ui, &name, &hint, used, most, text);
                if let Some(content) = content.filter(|_| info.spoofed()) {
                    label(ui, &fl!("inspect-plays"), &fl!("inspect-plays", "hint"));
                    ui.label(widgets::seconds(content));
                    ui.end_row();
                }
            }
            (None, Some(content)) => {
                label(ui, &fl!("inspect-duration"), "");
                let seconds = widgets::seconds(content);
                ui.label(fl!("inspect-duration", "no-header", seconds = seconds));
                ui.end_row();
            }
            (None, None) => {}
        }
        if let Some(fps) = info.fps() {
            let most = telegram::MAX_FPS;
            let text = fl!(
                "inspect-frame-rate",
                "value",
                fps = format!("{fps:.2}"),
                most = format!("{most:.0}"),
                frames = fl!("count-frames", count = info.video_frames)
            );
            let name = fl!("inspect-frame-rate");
            budget(ui, &name, "", (fps * 100.0) as u64, (most * 100.0) as u64, text);
        }
        label(ui, &fl!("inspect-picture"), "");
        match &info.video {
            Some(video) => {
                let codec = if video.codec_id == "V_VP9" { "VP9" } else { video.codec_id.as_str() };
                let (width, height) = (video.width, video.height);
                ui.label(if video.alpha {
                    fl!("inspect-video", "alpha", width = width, height = height, codec = codec)
                } else {
                    fl!("inspect-video", width = width, height = height, codec = codec)
                });
            }
            None => {
                ui.label(fl!("inspect-picture", "none"));
            }
        }
        ui.end_row();
        if info.audio_tracks > 0 {
            label(ui, &fl!("inspect-audio"), "");
            ui.label(fl!("count-tracks", count = info.audio_tracks));
            ui.end_row();
        }
        if let Some(title) = &info.title {
            label(ui, &fl!("inspect-title"), "");
            ui.add(egui::Label::new(title).truncate());
            ui.end_row();
        }
        label(ui, &fl!("inspect-made-with"), &fl!("inspect-made-with", "hint"));
        let made = match (&info.writing_app, &info.muxing_app) {
            (Some(writing), Some(muxing)) if writing != muxing => format!("{writing}, {muxing}"),
            (Some(app), _) | (None, Some(app)) => app.clone(),
            (None, None) => fl!("inspect-made-with", "unknown"),
        };
        ui.add(egui::Label::new(made).truncate());
        ui.end_row();
        if let Some(signature) = &info.signature {
            label(ui, &fl!("inspect-note"), &fl!("inspect-note", "hint"));
            ui.add(egui::Label::new(signature).truncate());
            ui.end_row();
        }
    });
}

fn image(ui: &mut egui::Ui, info: &still::Info, target: Target) {
    note(
        ui,
        match target {
            Target::Sticker => fl!("inspect-image-sticker"),
            Target::Emoji => fl!("inspect-image-emoji"),
        },
    );
    ui.add_space(6.0);
    grid(ui, "inspect-image", |ui| {
        let limit = telegram::MAX_STILL_BYTES;
        let size = of(widgets::kib(info.bytes), widgets::kib(limit));
        let (name, hint) = (fl!("inspect-size"), fl!("inspect-size", "hint"));
        budget(ui, &name, &hint, info.bytes, limit, size);
        label(ui, &fl!("inspect-picture"), "");
        let (width, height) = (info.width, info.height);
        ui.label(match (info.lossy, info.alpha) {
            (false, false) => fl!("inspect-webp", width = width, height = height),
            (false, true) => fl!("inspect-webp", "alpha", width = width, height = height),
            (true, false) => fl!("inspect-webp", "lossy", width = width, height = height),
            (true, true) => fl!("inspect-webp", "lossy-alpha", width = width, height = height),
        });
        ui.end_row();
        label(ui, &fl!("inspect-made-with"), &fl!("inspect-made-with", "image"));
        let made = info.made_with.clone().unwrap_or_else(|| fl!("inspect-made-with", "unknown"));
        ui.add(egui::Label::new(made).truncate());
        ui.end_row();
    });
}

fn animation(ui: &mut egui::Ui, stats: &Stats) {
    let (width, height, fps) = (stats.width, stats.height, stats.fps);
    note(ui, fl!("inspect-tgs", width = width, height = height, fps = fps));
    ui.add_space(6.0);
    grid(ui, "inspect-tgs", |ui| {
        if let Some(name) = &stats.name {
            label(ui, &fl!("inspect-name"), &fl!("inspect-name", "hint"));
            ui.add(egui::Label::new(name).truncate());
            ui.end_row();
        }
        if let Some(bytes) = stats.tgs_bytes {
            let (bytes, limit) = (bytes as u64, server::MAX_BYTES as u64);
            let text = of(widgets::kib(bytes), widgets::kib(limit));
            let (name, hint) = (fl!("inspect-size"), fl!("inspect-size", "tgs"));
            budget(ui, &name, &hint, bytes, limit, text);
        }
        let seconds = if stats.fps > 0.0 { stats.frames / stats.fps } else { 0.0 };
        let frames = fl!("count-frames", count = stats.frames);
        let text =
            fl!("inspect-length", "value", seconds = widgets::seconds(seconds), frames = frames);
        let (name, hint) = (fl!("inspect-length"), fl!("inspect-length", "hint"));
        budget(ui, &name, &hint, (seconds * 1000.0) as u64, 3000, text);
        let refused = fl!("inspect-refuses-more");
        let cost = server::cost(stats.shapes, stats.layers);
        let (name, hint) = (fl!("inspect-cost"), fl!("inspect-cost", "hint"));
        let most = server::MAX_COST as u64;
        budget(ui, &name, &hint, cost as u64, most, of(cost, server::MAX_COST));
        let (layers, most) = (stats.layers as u64, server::MAX_LAYERS as u64);
        budget(ui, &fl!("inspect-layers"), &refused, layers, most, of(layers, most));
        let (shapes, most) =
            (stats.max_shapes_per_layer as u64, server::MAX_SHAPES_PER_LAYER as u64);
        budget(ui, &fl!("inspect-shapes"), &refused, shapes, most, of(shapes, most));
        let (points, most) = (stats.max_paint_points as u64, server::MAX_PAINT_POINTS as u64);
        let (name, hint) = (fl!("inspect-points"), fl!("inspect-points", "hint"));
        budget(ui, &name, &hint, points, most, of(points, most));
        let (json, desktop) = (stats.json_bytes as u64, limits::MAX_RAW_JSON as u64);
        let (name, hint) = (fl!("inspect-unpacked"), fl!("inspect-unpacked", "hint"));
        let text = of(widgets::kib(json), widgets::kib(desktop));
        budget(ui, &name, &hint, json, desktop, text);
        if !stats.features.is_empty() {
            label(ui, &fl!("inspect-features"), &fl!("inspect-features", "hint"));
            ui.label(stats.features.iter().copied().collect::<Vec<_>>().join(", "));
            ui.end_row();
        }
    });
}

impl App {
    /// What an inspected sticker holds, in a dialog, while there is one.
    pub(crate) fn inspection_dialog(&mut self, ctx: &egui::Context) {
        let Some(inspection) = &self.inspection else { return };
        let name = inspection
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let style = ctx.global_style();
        let format = egui::TextFormat::simple(
            egui::TextStyle::Heading.resolve(&style),
            style.visuals.text_color(),
        );
        let title = drawable_in(&self.fallbacks, &name, format, style.visuals.code_bg_color).0;
        let (mut another, mut close) = (false, false);
        let modal = egui::Modal::new(egui::Id::new("inspect")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.add(egui::Label::new(title).truncate());
            ui.add_space(4.0);
            show(ui, inspection);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = ui.button(fl!("close")).clicked();
                another = ui.button(fl!("inspect-another")).clicked();
            });
        });
        if close || modal.should_close() {
            self.inspection = None;
        }
        if another {
            self.pick_inspected();
        }
    }

    /// Asks for a sticker to inspect.
    pub(crate) fn pick_inspected(&mut self) {
        if let Some(file) = rfd::FileDialog::new()
            .add_filter(fl!("stickers-filter"), &["webm", "tgs", "webp"])
            .pick_file()
        {
            self.inspection = Some(Inspection::of(&file));
        }
    }
}

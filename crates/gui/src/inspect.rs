//! What a sticker file holds and what Telegram would say about it.

use std::path::{Path, PathBuf};

use eframe::egui;
use tgradish_core::telegram::{self, Target};
use tgradish_core::tgs;
use tgradish_core::webm::{self, WebmInfo};
use tgradish_tgs::check::{Severity, Stats};
use tgradish_tgs::limits::{self, telegram as server};

use crate::jobs::Problem;
use crate::widgets::{self, gauge, label, note, problems};

pub enum Found {
    Webm { info: Box<WebmInfo>, target: Target },
    Tgs(Box<Stats>),
    Failed(String),
}

pub struct Inspection {
    pub path: PathBuf,
    pub found: Found,
    pub problems: Vec<Problem>,
}

impl Inspection {
    pub fn of(path: &Path) -> Inspection {
        let (found, problems) = if tgs::is_sticker(path) {
            match tgs::inspect_file(path) {
                Ok((stats, issues)) => {
                    let problems = issues
                        .into_iter()
                        .map(|issue| Problem {
                            refused: issue.severity == Severity::Error,
                            text: issue.message,
                        })
                        .collect();
                    (Found::Tgs(Box::new(stats)), problems)
                }
                Err(err) => (Found::Failed(err.to_string()), Vec::new()),
            }
        } else {
            match webm::inspect_file(path) {
                Ok(info) => {
                    // emoji are the only square 100 px stickers
                    let target = match &info.video {
                        Some(video) if video.width == 100 && video.height == 100 => Target::Emoji,
                        _ => Target::Sticker,
                    };
                    let problems = telegram::check(&info, target)
                        .iter()
                        .map(|issue| Problem { refused: true, text: issue.to_string() })
                        .collect();
                    (Found::Webm { info: Box::new(info), target }, problems)
                }
                Err(err) => (Found::Failed(err.to_string()), Vec::new()),
            }
        };
        Inspection { path: path.to_path_buf(), found, problems }
    }
}

fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).min_col_width(130.0).spacing([12.0, 7.0]).show(ui, rows);
}

pub fn show(ui: &mut egui::Ui, inspection: &Inspection) {
    match &inspection.found {
        Found::Failed(message) => {
            ui.colored_label(widgets::BAD, message);
            return;
        }
        Found::Webm { info, target } => webm(ui, info, *target),
        Found::Tgs(stats) => animation(ui, stats),
    }
    ui.add_space(8.0);
    problems(ui, &inspection.problems);
}

fn webm(ui: &mut egui::Ui, info: &WebmInfo, target: Target) {
    note(ui, format!("A video {}", target.name()));
    ui.add_space(4.0);
    gauge(
        ui,
        info.file_size,
        target.max_bytes(),
        format!("{} of {}", widgets::kib(info.file_size), widgets::kib(target.max_bytes())),
    );
    ui.add_space(6.0);
    grid(ui, "inspect-webm", |ui| {
        label(ui, "Picture", "");
        match &info.video {
            Some(video) => {
                let codec = if video.codec_id == "V_VP9" { "VP9" } else { video.codec_id.as_str() };
                let alpha = if video.alpha { ", with transparency" } else { "" };
                ui.label(format!("{} × {} px, {codec}{alpha}", video.width, video.height));
            }
            None => {
                ui.label("none");
            }
        }
        ui.end_row();
        label(ui, "Frames", "");
        let fps = info.fps().map(|fps| format!(" at {fps:.2} fps")).unwrap_or_default();
        ui.label(format!("{}{fps}", info.video_frames));
        ui.end_row();
        label(ui, "Duration", "Telegram reads the one in the header; spoofing makes it short");
        match (info.header_duration, info.content_duration) {
            (Some(header), Some(content)) if content > header + 0.05 => {
                ui.label(format!(
                    "{:.3} s in the header, plays {}: spoofed",
                    header,
                    widgets::seconds(content)
                ));
            }
            (Some(header), _) => {
                ui.label(widgets::seconds(header));
            }
            (None, Some(content)) => {
                ui.label(format!("{}, none in the header", widgets::seconds(content)));
            }
            (None, None) => {
                ui.label("unknown");
            }
        }
        ui.end_row();
        if info.audio_tracks > 0 {
            label(ui, "Audio", "");
            ui.label(format!("{} tracks", info.audio_tracks));
            ui.end_row();
        }
        if let Some(title) = &info.title {
            label(ui, "Title", "");
            ui.label(title);
            ui.end_row();
        }
        label(ui, "Made with", "");
        let app = info.writing_app.as_deref().or(info.muxing_app.as_deref()).unwrap_or("unknown");
        let marked = if info.signature.is_some() { ", marked by tgradish" } else { "" };
        ui.label(format!("{app}{marked}"));
        ui.end_row();
    });
}

fn animation(ui: &mut egui::Ui, stats: &Stats) {
    note(ui, "A vector animation");
    ui.add_space(4.0);
    if let Some(bytes) = stats.tgs_bytes {
        let (bytes, limit) = (bytes as u64, server::MAX_BYTES as u64);
        gauge(ui, bytes, limit, format!("{} of {}", widgets::kib(bytes), widgets::kib(limit)));
    }
    ui.add_space(6.0);
    grid(ui, "inspect-tgs", |ui| {
        label(ui, "Canvas", "");
        ui.label(format!("{} × {}", stats.width, stats.height));
        ui.end_row();
        label(ui, "Frames", "");
        let seconds = if stats.fps > 0.0 { stats.frames / stats.fps } else { 0.0 };
        ui.label(format!("{} at {} fps, {}", stats.frames, stats.fps, widgets::seconds(seconds)));
        ui.end_row();
        let used = |ui: &mut egui::Ui, used: usize, limit: usize, hint: &str| {
            let text = format!("{used} of {limit}");
            if used > limit {
                ui.colored_label(widgets::BAD, text).on_hover_text(hint);
            } else {
                ui.label(text).on_hover_text(hint);
            }
        };
        label(ui, "Unpacked", "Telegram Desktop refuses larger animations");
        let (json, desktop) = (stats.json_bytes as u64, limits::MAX_RAW_JSON as u64);
        ui.label(format!("{} of {}", widgets::kib(json), widgets::kib(desktop)));
        ui.end_row();
        label(ui, "Layers", "");
        used(ui, stats.layers, server::MAX_LAYERS, "Telegram refuses more");
        ui.end_row();
        label(ui, "Drawing cost", "Shapes plus 9 for each layer, as Telegram counts them");
        used(
            ui,
            server::cost(stats.shapes, stats.layers),
            server::MAX_COST,
            "Telegram refuses more",
        );
        ui.end_row();
        label(ui, "Most shapes in a layer", "");
        used(ui, stats.max_shapes_per_layer, server::MAX_SHAPES_PER_LAYER, "Telegram refuses more");
        ui.end_row();
        label(ui, "Most points in a fill", "Path points one fill paints");
        used(ui, stats.max_paint_points, server::MAX_PAINT_POINTS, "Telegram refuses more");
        ui.end_row();
        if !stats.features.is_empty() {
            label(ui, "Features", "Lottie features beyond shapes and transforms");
            ui.label(stats.features.iter().copied().collect::<Vec<_>>().join(", "));
            ui.end_row();
        }
    });
}

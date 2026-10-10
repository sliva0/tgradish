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
use tgradish_core::mark::{Client, Mark};

pub enum Found {
    Webm { info: Box<WebmInfo>, target: Target },
    Tgs(Box<Stats>),
    Failed(String),
}

pub struct Inspection {
    pub path: PathBuf,
    pub found: Found,
    pub problems: Vec<Problem>,
    /// What tgradish's hidden mark in it says.
    pub mark: Option<Mark>,
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
        let mark = tgradish_core::mark::read_file(path);
        Inspection { path: path.to_path_buf(), found, problems, mark }
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
    format!("{used} of {limit}")
}

pub fn show(ui: &mut egui::Ui, inspection: &Inspection) {
    match &inspection.found {
        Found::Failed(message) => {
            ui.colored_label(widgets::bad(ui), message);
            return;
        }
        Found::Webm { info, target } => webm(ui, info, *target, inspection.mark),
        Found::Tgs(stats) => animation(ui, stats, inspection.mark),
    }
    ui.add_space(8.0);
    problems(ui, &inspection.problems);
}

/// Who made it, by the hidden mark.
fn made_by(ui: &mut egui::Ui, mark: Option<Mark>) {
    let Some(mark) = mark else { return };
    label(ui, "Hidden mark", "tgradish hides this in every sticker it makes");
    let client = match mark.client {
        Client::Library => "",
        Client::Cli => ", from the command line",
        Client::Window => ", in its window",
        Client::Bot => ", through the bot",
    };
    let user = if mark.user == 0 { String::new() } else { format!(", user {}", mark.user_text()) };
    ui.label(format!("tgradish {}{client}{user}", mark.version_text()))
        .on_hover_text("The user is a hash of the user's name: stickers by one person share it");
    ui.end_row();
}

fn webm(ui: &mut egui::Ui, info: &WebmInfo, target: Target, mark: Option<Mark>) {
    note(ui, format!("A video {}", target.name()));
    ui.add_space(6.0);
    grid(ui, "inspect-webm", |ui| {
        let limit = target.max_bytes();
        let size = of(widgets::kib(info.file_size), widgets::kib(limit));
        budget(ui, "Size", "Telegram refuses larger files", info.file_size, limit, size);
        let max = telegram::MAX_SECONDS;
        match (info.header_duration, info.content_duration) {
            (Some(header), content) => {
                let spoofed = content.is_some_and(|content| content > header + 0.05);
                let text = if spoofed {
                    format!("{header:.3} s in the header, spoofed")
                } else {
                    of(widgets::seconds(header), widgets::seconds(max))
                };
                let hint = "Telegram reads the duration in the header; spoofing makes it short";
                budget(ui, "Duration", hint, (header * 1000.0) as u64, (max * 1000.0) as u64, text);
                if let Some(content) = content.filter(|_| spoofed) {
                    label(ui, "Plays", "How long the video really is");
                    ui.label(widgets::seconds(content));
                    ui.end_row();
                }
            }
            (None, Some(content)) => {
                label(ui, "Duration", "");
                ui.label(format!("{}, none in the header", widgets::seconds(content)));
                ui.end_row();
            }
            (None, None) => {}
        }
        if let Some(fps) = info.fps() {
            let most = telegram::MAX_FPS;
            let text = format!("{fps:.2} fps of {most:.0}, {} frames", info.video_frames);
            budget(ui, "Frame rate", "", (fps * 100.0) as u64, (most * 100.0) as u64, text);
        }
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
        ui.label(app);
        ui.end_row();
        made_by(ui, mark);
    });
}

fn animation(ui: &mut egui::Ui, stats: &Stats, mark: Option<Mark>) {
    note(
        ui,
        format!("A vector animation, {} × {} at {} fps", stats.width, stats.height, stats.fps),
    );
    ui.add_space(6.0);
    grid(ui, "inspect-tgs", |ui| {
        if let Some(bytes) = stats.tgs_bytes {
            let (bytes, limit) = (bytes as u64, server::MAX_BYTES as u64);
            let text = of(widgets::kib(bytes), widgets::kib(limit));
            budget(ui, "Size", "Telegram refuses larger stickers", bytes, limit, text);
        }
        let seconds = if stats.fps > 0.0 { stats.frames / stats.fps } else { 0.0 };
        let text = format!("{} of 3.00 s, {} frames", widgets::seconds(seconds), stats.frames);
        budget(
            ui,
            "Length",
            "Animated stickers last at most 3 seconds",
            (seconds * 1000.0) as u64,
            3000,
            text,
        );
        let refused = "Telegram refuses more";
        let cost = server::cost(stats.shapes, stats.layers);
        budget(
            ui,
            "Drawing cost",
            "Shapes plus 9 for each layer, as Telegram counts them",
            cost as u64,
            server::MAX_COST as u64,
            of(cost, server::MAX_COST),
        );
        let (layers, most) = (stats.layers as u64, server::MAX_LAYERS as u64);
        budget(ui, "Layers", refused, layers, most, of(layers, most));
        let (shapes, most) =
            (stats.max_shapes_per_layer as u64, server::MAX_SHAPES_PER_LAYER as u64);
        budget(ui, "Most shapes in a layer", refused, shapes, most, of(shapes, most));
        let (points, most) = (stats.max_paint_points as u64, server::MAX_PAINT_POINTS as u64);
        budget(
            ui,
            "Most points in a fill",
            "Path points one fill paints; Telegram refuses more",
            points,
            most,
            of(points, most),
        );
        let (json, desktop) = (stats.json_bytes as u64, limits::MAX_RAW_JSON as u64);
        budget(
            ui,
            "Unpacked",
            "Telegram Desktop doesn't play larger animations",
            json,
            desktop,
            of(widgets::kib(json), widgets::kib(desktop)),
        );
        if !stats.features.is_empty() {
            label(ui, "Features", "Lottie features beyond shapes and transforms");
            ui.label(stats.features.iter().copied().collect::<Vec<_>>().join(", "));
            ui.end_row();
        }
        made_by(ui, mark);
    });
}

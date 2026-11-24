//! Sticker inspection dialog showing file properties and Telegram validation
//! issues.

use std::path::{Path, PathBuf};

use eframe::egui;
use tgradish_core::inspect::{self, Inspected};
use tgradish_core::telegram::{self, Target};
use tgradish_core::tgradish_tgs::check::Stats;
use tgradish_core::tgradish_tgs::limits::{self, telegram as server};
use tgradish_core::webm::container::WebmInfo;
use tgradish_core::webp;

use crate::app::App;
use crate::fonts::drawable_in;
use crate::jobs::{self, Problem};
use crate::l10n::fl;
use crate::theme;
use crate::widgets::{self, gauge, note, problems};

pub(crate) enum Found {
    Webm { info: Box<WebmInfo>, target: Target },
    Tgs(Box<Stats>),
    Image { info: webp::Info, target: Target },
    Failed(crate::l10n::Text),
}

pub(crate) struct Inspection {
    pub(crate) path: PathBuf,
    pub(crate) found: Found,
    pub(crate) problems: Vec<Problem>,
}

impl Inspection {
    pub(crate) fn of(path: &Path) -> Inspection {
        let (found, problems) = match inspect::inspect(path, None) {
            Ok(Inspected::Webp { target, info, issues }) => {
                (Found::Image { info, target }, jobs::problems(&issues))
            }
            Ok(Inspected::Tgs { info, issues }) => (Found::Tgs(info), jobs::problems(&issues)),
            Ok(Inspected::Webm { target, info, issues }) => {
                (Found::Webm { info, target }, jobs::problems(&issues))
            }
            // a kind of file the GUI doesn't know yet
            Ok(_) => (Found::Failed(crate::l10n::later!("inspect-unknown")), Vec::new()),
            Err(err) => (Found::Failed(crate::jobs::failure(&err)), Vec::new()),
        };
        Inspection { path: path.to_path_buf(), found, problems }
    }
}

/// The least width of a grid's column of names, and the room between it
/// and what each row says.
const NAMES: f32 = 150.0;
const BETWEEN: f32 = 12.0;
/// The width of a [`budget`]'s bar, where there is room for it.
const BUDGET: f32 = 300.0;

fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    // a grid's cells are as narrow as its least column as it is first laid
    // out, which its rows' heights would keep: what they say is given the
    // room the dialog has instead, and neither column is kept wider. The
    // names are as wide as the widest of them last drawn, where that
    // leaves the bars their width, as many languages' are longer.
    let available = ui.available_width();
    let widest = ui.data(|data| data.get_temp(widest_id(false))).unwrap_or(NAMES);
    let names = widest.min(available - BETWEEN - BUDGET).max(NAMES);
    let room = (available - names - BETWEEN).max(0.0);
    ui.data_mut(|data| {
        data.insert_temp(room_id(), (names, room));
        data.insert_temp(widest_id(true), 0.0_f32);
    });
    let least = names.min(room);
    egui::Grid::new(id).num_columns(2).min_col_width(least).spacing([BETWEEN, 7.0]).show(ui, rows);
    ui.data_mut(|data| {
        let drawn: f32 = data.get_temp(widest_id(true)).unwrap_or(NAMES);
        data.insert_temp(widest_id(false), drawn);
    });
}

/// Where [`grid`] keeps the room its rows have for their names and for
/// what they say, an `(f32, f32)`: written by each grid of the dialog as
/// it is drawn, and read by its rows with [`room`].
fn room_id() -> egui::Id {
    egui::Id::new("inspect room")
}

/// Where [`grid`] keeps how wide the widest name of its rows is, unwrapped,
/// an `f32`: as last drawn, or as its rows `drawn` now note it.
fn widest_id(drawn: bool) -> egui::Id {
    egui::Id::new(("inspect widest name", drawn))
}

/// The room [`grid`] gives what a row says.
fn room(ui: &egui::Ui) -> f32 {
    ui.data(|data| data.get_temp::<(f32, f32)>(room_id())).map_or(BUDGET, |(_, room)| room)
}

/// A row's name, with `hint` on hover where there is one, wrapping where it
/// is wider than the column of names, which would leave less room than
/// [`grid`] gave what the row says.
fn label(ui: &mut egui::Ui, text: &str, hint: &str) {
    let width = widgets::text_width(ui, text, 0.0).ceil();
    let names = ui.data_mut(|data| {
        let widest = data.get_temp_mut_or(widest_id(true), 0.0_f32);
        *widest = widest.max(width);
        data.get_temp::<(f32, f32)>(room_id()).map_or(NAMES, |(names, _)| names)
    });
    ui.scope(|ui| {
        ui.set_max_width(names);
        let label = ui.add(egui::Label::new(text).wrap());
        if !hint.is_empty() {
            label.on_hover_text(hint);
        }
    });
}

/// A row of how much of a limit something uses: its bar narrower, its
/// words wrapping, where the dialog is too narrow for it.
fn budget(ui: &mut egui::Ui, name: &str, hint: &str, used: u64, limit: u64, text: String) {
    label(ui, name, hint);
    ui.scope(|ui| {
        ui.set_width(room(ui).min(BUDGET));
        gauge(ui, used, limit, text);
    });
    ui.end_row();
}

/// What a row says, wrapping where the dialog is too narrow for it.
fn value(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.scope(|ui| {
        ui.set_max_width(room(ui));
        ui.add(egui::Label::new(text).wrap());
    });
}

fn of(used: impl std::fmt::Display, limit: impl std::fmt::Display) -> String {
    fl!("of-limit", used = used.to_string(), limit = limit.to_string())
}

pub(crate) fn show(ui: &mut egui::Ui, inspection: &Inspection) {
    match &inspection.found {
        Found::Failed(message) => {
            ui.colored_label(theme::bad(ui.visuals()), message.say());
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
                    value(ui, widgets::seconds(content));
                    ui.end_row();
                }
            }
            (None, Some(content)) => {
                label(ui, &fl!("inspect-duration"), "");
                let seconds = widgets::seconds(content);
                value(ui, fl!("inspect-duration", "no-header", seconds = seconds));
                ui.end_row();
            }
            (None, None) => {}
        }
        if let Some(fps) = info.fps() {
            let most = telegram::MAX_FPS;
            let text = fl!(
                "inspect-frame-rate",
                "value",
                fps = widgets::trimmed(fps),
                most = widgets::trimmed(most),
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
                value(
                    ui,
                    if video.alpha {
                        fl!("inspect-video", "alpha", width = width, height = height, codec = codec)
                    } else {
                        fl!("inspect-video", width = width, height = height, codec = codec)
                    },
                );
            }
            None => {
                value(ui, fl!("inspect-picture", "none"));
            }
        }
        ui.end_row();
        if info.audio_tracks > 0 {
            label(ui, &fl!("inspect-audio"), "");
            value(ui, fl!("count-tracks", count = info.audio_tracks));
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

fn image(ui: &mut egui::Ui, info: &webp::Info, target: Target) {
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
        value(
            ui,
            match (info.lossy, info.alpha) {
                (false, false) => fl!("inspect-webp", width = width, height = height),
                (false, true) => fl!("inspect-webp", "alpha", width = width, height = height),
                (true, false) => fl!("inspect-webp", "lossy", width = width, height = height),
                (true, true) => fl!("inspect-webp", "lossy-alpha", width = width, height = height),
            },
        );
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
        let most = (telegram::MAX_SECONDS * 1000.0) as u64;
        budget(ui, &name, &hint, (seconds * 1000.0) as u64, most, text);
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
            value(ui, stats.features.iter().copied().collect::<Vec<_>>().join(", "));
            ui.end_row();
        }
    });
}

impl App {
    /// What an inspected sticker holds, in a dialog, while there is one.
    pub(crate) fn inspection_dialog(&mut self, ctx: &egui::Context) {
        let Some(inspection) = &self.dialogs.inspection else { return };
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
        let title =
            drawable_in(&self.fonts.fallbacks, &name, format, style.visuals.code_bg_color).0;
        let (mut another, mut close) = (false, false);
        let modal = widgets::dialog(ctx, "inspect").width(520.0).show(ctx, |ui| {
            close = widgets::dialog_title(ui, title);
            ui.add_space(4.0);
            // the dialog fits the window, what was found scrolling within
            widgets::dialog_body(ui, egui::ScrollArea::vertical(), |ui| {
                show(ui, inspection);
            });
            widgets::dialog_buttons(ui, |ui| {
                close |= ui.add(widgets::dialog_button(fl!("close"))).clicked();
                another = ui.add(widgets::dialog_button(fl!("inspect-another"))).clicked();
            });
        });
        if close || modal.should_close() {
            self.dialogs.inspection = None;
        }
        if another {
            self.picker.pick(ctx, crate::files::Pick::Inspected);
        }
    }
}

//! The settings of one item: what to make, beside the preview, and how,
//! under it.

mod advanced;
mod aseprite;
mod commands;
mod picture;
mod plan;
mod tgs;
mod time;
mod webm;

use std::collections::HashMap;

use eframe::egui::{self, RichText, vec2};
use tgradish_core::config::Config;
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;

use crate::item::{Item, Kind};
use crate::l10n::fl;
use crate::preset_dialogs::{self, Ask, is_users};
use crate::widgets::{self, Choice, note, segments};
use advanced::advanced;
use aseprite::aseprite;
use commands::commands;
use picture::picture;
use tgs::{reading, tgs_quality};
use time::{frame, time};
use webm::webm_quality;

pub(crate) use picture::{exact_of, ratio, resize_of, webm_sizes};
pub use plan::plan;

/// What the settings depend on besides the item.
pub struct Context<'a> {
    pub presets: &'a Presets,
    pub config: &'a Config,
    /// ffmpeg is a separate program, so raw arguments can be passed.
    pub extra_args: bool,
    /// Where the result goes.
    pub output: Option<std::path::PathBuf>,
    /// The config file the window reads and saves.
    pub config_path: Option<&'a std::path::Path>,
    /// Where presets are saved, if anywhere.
    pub presets_dir: Option<&'a std::path::Path>,
    /// The timeline counts frames rather than seconds, and so do the
    /// settings of the part used.
    pub in_frames: bool,
}

/// The built-in presets, in the order of their speed.
const BUILTIN: [&str; 3] = ["fast", "balanced", "best"];

/// The built-in presets in the order of their speed, then the user's own.
pub fn preset_names(presets: &Presets) -> Vec<(String, Result<String, String>)> {
    let mut names: Vec<(String, Result<String, String>)> = presets
        .iter()
        .map(|(name, preset)| {
            (name.to_owned(), preset.map(|p| p.description.clone()).map_err(str::to_owned))
        })
        .collect();
    names.sort_by_key(|(name, _)| {
        (BUILTIN.iter().position(|b| b == name).unwrap_or(BUILTIN.len()), name.clone())
    });
    names
}

/// What a built-in preset does for `format`. Core's own descriptions
/// speak of every format at once, for the command line; the window shows
/// only what the preset does for what is being made.
pub fn builtin_description(name: &str, format: Format) -> Option<String> {
    Some(match (name, format) {
        ("fast", Format::Webm) => fl!("preset-fast", "webm"),
        ("balanced", Format::Webm) => fl!("preset-balanced", "webm"),
        ("best", Format::Webm) => fl!("preset-best", "webm"),
        ("fast", Format::Tgs) => fl!("preset-fast", "tgs"),
        ("balanced", Format::Tgs) => fl!("preset-balanced", "tgs"),
        ("best", Format::Tgs) => fl!("preset-best", "tgs"),
        _ => return None,
    })
}

/// What a preset is called: the built-in ones in the window's language,
/// the user's own as they named them.
pub fn preset_label(name: &str) -> String {
    match name {
        "fast" => fl!("preset-fast"),
        "balanced" => fl!("preset-balanced"),
        "best" => fl!("preset-best"),
        _ => name.to_owned(),
    }
}

/// What to make: format, target and preset. Narrow, for beside the
/// preview.
pub fn make(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.label(RichText::new(fl!("make")).strong().size(15.5));
    ui.add_space(4.0);
    let formats = [
        (Format::Webm, fl!("format-video"), fl!("format-video", "hint")),
        (Format::Tgs, fl!("format-animation"), fl!("format-animation", "hint")),
        (Format::Image, fl!("format-image"), fl!("format-image", "hint")),
    ];
    let mut format = item.format;
    if segments(ui, &mut format, &formats, |_| Ok(())) {
        item.format = format;
        item.format_guessed = false;
    }
    segments(ui, &mut item.choices.target, &targets(), |_| Ok(()));
    // .tgs animations are vectors: square, but of no size in pixels
    note(
        ui,
        match (item.format, item.choices.target) {
            (Format::Webm, Target::Sticker) => fl!("needs-webm-sticker"),
            (Format::Webm, Target::Emoji) => fl!("needs-webm-emoji"),
            (Format::Tgs, _) => fl!("needs-tgs"),
            (Format::Image, Target::Sticker) => fl!("needs-image-sticker"),
            (Format::Image, Target::Emoji) => fl!("needs-image-emoji"),
        },
    );
    // picked for what isn't pixel art, which would come out blocky; the
    // window's own guess never does
    let not_art = item.format == Format::Tgs
        && !item.format_guessed
        && !matches!(item.kind, Kind::Aseprite | Kind::Frames)
        && item.art.ready().is_some_and(|art| !art.clip.looks_like_pixel_art());
    if not_art {
        ui.colored_label(widgets::WARN, format!("⚠ {}", fl!("not-pixel-art")));
    }

    // presets say how to encode videos and animations; a picture has
    // nothing to choose there
    ui.add_space(8.0);
    let image = item.format == Format::Image;
    let nothing = fl!("preset", "image");
    ui.add_enabled_ui(!image, |ui| {
        let users =
            context.presets.iter().any(|(_, preset)| preset.is_err() || preset.is_ok_and(is_users));
        ui.horizontal(|ui| {
            ui.label(fl!("preset"))
                .on_hover_text(fl!("preset", "hint"))
                .on_disabled_hover_text(nothing.as_str());
            // where the user's own are renamed or deleted, once there are any
            if users && context.presets_dir.is_some() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let manage = egui::Button::new(RichText::new(fl!("manage-presets")).small())
                        .frame(false);
                    if ui.add(manage).on_hover_text(fl!("manage-presets", "hint")).clicked() {
                        preset_dialogs::ask(ui.ctx(), Ask::Manage);
                    }
                });
            }
        });
        let current = item.choices.preset_name(item.format, context.config).to_owned();
        let default = context.config.preset_for(item.format).to_owned();
        let names = preset_names(context.presets);
        let mut chosen = current.as_str();
        // the built-in three side by side, none lit while one of the
        // user's is chosen
        let speeds: Vec<Choice<&str>> = names
            .iter()
            .filter(|(name, _)| BUILTIN.contains(&name.as_str()))
            .map(|(name, description)| {
                // the built-in one's words for what is being made, or
                // those of the user's file standing in for it
                let builtin = context.presets.get(name).is_ok_and(|preset| preset.path.is_none());
                let description = match builtin.then(|| builtin_description(name, item.format)) {
                    Some(Some(text)) => text,
                    _ => description.as_deref().unwrap_or("").to_owned(),
                };
                (name.as_str(), preset_label(name), description)
            })
            .collect();
        // a file of the user's may stand in for one, and be broken
        let broken = |name: &str| {
            names.iter().any(|(other, description)| other == name && description.is_err())
        };
        segments(ui, &mut chosen, &speeds, |name| {
            if broken(name) { Err(fl!("preset", "broken")) } else { Ok(()) }
        });
        // the user's own under them, a button each
        let own: Vec<_> =
            names.iter().filter(|(name, _)| !BUILTIN.contains(&name.as_str())).collect();
        if !own.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for (name, description) in own {
                    let response =
                        widgets::choice(ui, chosen == name, name, 150.0, description.is_ok());
                    let response =
                        match description {
                            Err(_) => response.on_disabled_hover_text(fl!("preset", "broken")),
                            Ok(description) if !description.is_empty() => response.on_hover_text(
                                fl!("preset-described", name = name, description = description),
                            ),
                            Ok(_) => response.on_hover_text(name),
                        };
                    if response.clicked() {
                        chosen = name;
                    }
                }
            });
        }
        if chosen != current {
            item.choices.preset = (chosen != default).then(|| chosen.to_owned());
        }
        match (builtin_description(chosen, item.format), context.presets.get(chosen)) {
            _ if image => note(ui, nothing),
            (Some(text), Ok(preset)) if preset.path.is_none() => note(ui, text),
            (_, Ok(preset)) => note(ui, preset.description.clone()),
            _ => {}
        }
    });
}

/// Stickers or emoji, for picking which a file becomes.
pub fn targets() -> [Choice<Target>; 2] {
    [
        (Target::Sticker, fl!("target-sticker"), String::new()),
        (Target::Emoji, fl!("target-emoji"), String::new()),
    ]
}

/// Width of the settings from which they go in two columns.
const TWO_COLUMNS: f32 = 1150.0;
/// Room between the two columns, with a line down its middle.
const GUTTER: f32 = 24.0;

/// Shows the settings under the preview: what goes in, how it looks and
/// plays, and then the file that comes out, beside it where there is room.
pub fn show(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let parts = Part::of(item);
    if ui.available_width() >= TWO_COLUMNS {
        columns(ui, item, context, &parts);
    } else {
        for &part in &parts {
            part.show(ui, item, context);
        }
    }
    // long lines: under the columns, as wide as both
    commands(ui, item, context);
}

/// `parts` in two columns, each part where there is less so far.
fn columns(ui: &mut egui::Ui, item: &mut Item, context: &Context, parts: &[Part]) {
    // how high each part was last drawn
    let id = egui::Id::new("settings heights");
    let mut heights: HashMap<Part, f32> = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let height = |part: Part| heights.get(&part).copied().unwrap_or_default();
    // the picture first on the left, the Aseprite file's on the right, and
    // the rest each where there is less so far; to the right only where
    // clearly less, so a part changing a little doesn't move another
    let (mut left, mut right) = (vec![Part::Picture], Vec::new());
    if parts.contains(&Part::Aseprite) {
        right.push(Part::Aseprite);
    }
    let sum = |column: &[Part]| column.iter().map(|&part| height(part)).sum::<f32>();
    for &part in parts.iter().filter(|&&part| !matches!(part, Part::Picture | Part::Aseprite)) {
        if sum(&right) + BALANCE < sum(&left) {
            right.push(part);
        } else {
            left.push(part);
        }
    }
    let width = (ui.available_width() - GUTTER) / 2.0;
    let layout = egui::Layout::top_down(egui::Align::Min);
    let spacing = ui.spacing().item_spacing;
    let mut drawn = HashMap::new();
    let columns = ui.horizontal_top(|ui| {
        // the gutter alone between them, their own spacing within
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, column) in [left, right].into_iter().enumerate() {
            if index == 1 {
                ui.add_space(GUTTER);
            }
            ui.allocate_ui_with_layout(egui::vec2(width, 0.0), layout, |ui| {
                ui.spacing_mut().item_spacing = spacing;
                ui.set_width(width);
                for part in column {
                    // in the column itself, so a part knows what is above it
                    let top = ui.cursor().top();
                    part.show(ui, item, context);
                    drawn.insert(part, (ui.min_rect().bottom() - top).max(0.0));
                }
            });
        }
    });
    let rect = columns.response.rect;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().vline(rect.left() + width + GUTTER / 2.0, rect.y_range(), stroke);
    // drawn at other heights: placed again on the next frame
    let moved = drawn
        .iter()
        .any(|(part, height)| heights.get(part).is_none_or(|before| (before - height).abs() > 1.0));
    if moved {
        heights.extend(drawn);
        ui.data_mut(|data| data.insert_temp(id, heights));
        ui.ctx().request_repaint();
    }
}

/// How much shorter the right column must be to take the next part.
const BALANCE: f32 = 40.0;

/// A part of the settings, which the columns share out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Part {
    Aseprite,
    Picture,
    Time,
    Quality,
    Reading,
    Advanced,
}

impl Part {
    /// The parts an item has, in order: what is read and how it looks and
    /// plays, then the file that comes out.
    fn of(item: &Item) -> Vec<Part> {
        let mut parts = Vec::new();
        if item.kind == Kind::Aseprite {
            parts.push(Part::Aseprite);
        }
        parts.extend([Part::Picture, Part::Time]);
        match item.format {
            Format::Webm => parts.extend([Part::Quality, Part::Advanced]),
            Format::Tgs => parts.extend([Part::Quality, Part::Reading]),
            // lossless, or lossy where only that fits: nothing to choose
            Format::Image => {}
        }
        parts
    }

    fn show(self, ui: &mut egui::Ui, item: &mut Item, context: &Context) {
        match (self, item.format) {
            (Part::Aseprite, _) => aseprite(ui, item, context),
            (Part::Picture, _) => picture(ui, item, context),
            (Part::Time, Format::Image) => frame(ui, item),
            (Part::Time, _) => time(ui, item, context),
            (Part::Quality, Format::Webm) => webm_quality(ui, item, context),
            (Part::Quality, Format::Tgs) => tgs_quality(ui, item, context),
            (Part::Reading, _) => reading(ui, item),
            (Part::Advanced, _) => advanced(ui, item, context),
            (Part::Quality, Format::Image) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_presets_by_speed() {
        let names: Vec<String> =
            preset_names(&Presets::builtin()).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["fast", "balanced", "best"]);
    }
}

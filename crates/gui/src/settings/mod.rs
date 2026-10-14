//! The settings of one item: what to make, beside the preview, and how,
//! under it.

mod advanced;
mod aseprite;
mod commands;
mod picture;
mod plan;
mod save;
mod tgs;
mod time;
mod webm;

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use tgradish_core::config::Config;
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;

use crate::item::{Item, Kind};
use crate::widgets::{Choice, note, segments};
use advanced::advanced;
use aseprite::aseprite;
use commands::commands;
use picture::picture;
use tgs::{reading, tgs_quality};
use time::{frame, time};
use webm::webm_quality;

pub(crate) use picture::{exact_of, ratio, resize_of, webm_sizes};
pub use plan::plan;
pub use save::presets_saved;

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
}

/// The built-in presets in the order of their speed, then the user's own.
pub fn preset_names(presets: &Presets) -> Vec<(String, Result<String, String>)> {
    const BUILTIN: [&str; 3] = ["fast", "balanced", "best"];
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
pub fn builtin(name: &str, format: Format) -> Option<&'static str> {
    Some(match (name, format) {
        ("fast", Format::Webm) => {
            "Fits only the bitrate, with the fast encoder: seconds, but blurrier"
        }
        ("balanced", Format::Webm) => "Tries a few frame rates and keeps the one that looks best",
        ("best", Format::Webm) => {
            "Like balanced, with the slowest encoder: minutes for long videos"
        }
        ("fast", Format::Tgs) => "Under a second, a few percent larger",
        ("balanced", Format::Tgs) => "Between fast and best",
        ("best", Format::Tgs) => "The smallest file, in a few seconds",
        _ => return None,
    })
}

pub fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// What to make: format, target and preset. Narrow, for beside the
/// preview.
pub fn make(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.label(RichText::new("Make").strong().size(15.5));
    ui.add_space(4.0);
    let formats = [
        (Format::Webm, "Video", "A video sticker: VP9 in WebM, from any video or picture"),
        (
            Format::Tgs,
            "Animation",
            "An animated sticker: pixel art drawn as vector shapes, sharp at any size, and small",
        ),
        (Format::Image, "Image", "A static sticker: one frame of a video, or a picture, as WebP"),
    ];
    let mut format = item.format;
    if segments(ui, &mut format, &formats, |_| Ok(())) {
        item.format = format;
        item.format_guessed = false;
    }
    let targets = [(Target::Sticker, "Sticker", ""), (Target::Emoji, "Custom emoji", "")];
    segments(ui, &mut item.choices.target, &targets, |_| Ok(()));
    // .tgs animations are vectors: square, but of no size in pixels
    note(
        ui,
        match (item.format, item.choices.target) {
            (Format::Webm, Target::Sticker) => "WebM, 512 px on the longer side, up to 256 KiB",
            (Format::Webm, Target::Emoji) => "WebM, 100 × 100 px, up to 64 KiB",
            (Format::Tgs, _) => "TGS, square, up to 64 KiB",
            (Format::Image, Target::Sticker) => "WebP, 512 px on the longer side, up to 512 KiB",
            (Format::Image, Target::Emoji) => "WebP, 100 × 100 px",
        },
    );

    // presets say how to encode videos and animations; a picture has
    // nothing to choose there
    ui.add_space(8.0);
    let image = item.format == Format::Image;
    let nothing = "Pictures are saved as they are: there is nothing to encode";
    ui.add_enabled_ui(!image, |ui| {
        ui.label("Preset")
            .on_hover_text("How hard to work on the result; your own values below go on top")
            .on_disabled_hover_text(nothing);
        let current = item.choices.preset_name(item.format, context.config).to_owned();
        let default = context.config.preset_for(item.format).to_owned();
        let names = preset_names(context.presets);
        let labels: Vec<String> = names.iter().map(|(name, _)| capitalised(name)).collect();
        let choices: Vec<Choice<&str>> = names
            .iter()
            .zip(&labels)
            .map(|((name, description), label)| {
                (name.as_str(), label.as_str(), description.as_deref().unwrap_or(""))
            })
            .collect();
        let broken = |name: &str| {
            names.iter().find(|(n, _)| n == name).and_then(|(_, d)| d.as_ref().err()).is_some()
        };
        let mut chosen = current.as_str();
        // the built-in three fit side by side; more go in a list
        if choices.len() <= 3 {
            segments(ui, &mut chosen, &choices, |name| {
                if broken(name) { Err("This preset's file is broken") } else { Ok(()) }
            });
        } else {
            let shown = choices.iter().find(|(name, ..)| *name == chosen).map_or("", |c| c.1);
            egui::ComboBox::from_id_salt("preset").selected_text(shown).show_ui(ui, |ui| {
                for &(name, label, hint) in &choices {
                    let option = egui::Button::selectable(chosen == name, label);
                    let response = ui
                        .add_enabled(!broken(name), option)
                        .on_disabled_hover_text("This preset's file is broken");
                    let response =
                        if hint.is_empty() { response } else { response.on_hover_text(hint) };
                    if response.clicked() {
                        chosen = name;
                    }
                }
            });
        }
        if chosen != current {
            item.choices.preset = (chosen != default).then(|| chosen.to_owned());
        }
        match (builtin(chosen, item.format), context.presets.get(chosen)) {
            _ if image => note(ui, nothing),
            (Some(text), Ok(preset)) if preset.path.is_none() => note(ui, text),
            (_, Ok(preset)) => note(ui, preset.description.clone()),
            _ => {}
        }
    });
    save::save_preset(ui, item, context);
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

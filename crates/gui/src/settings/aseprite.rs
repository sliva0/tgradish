//! What of an Aseprite file is read, whatever it is made into: the frames
//! of one tag, and which layers show.

use eframe::egui::{self, RichText};
use tgradish_core::presets::Format;
use tgradish_tgs::frames::{Direction, LayerInfo, Tag};

use super::Context;
use crate::item::{Choices, Item};
use crate::widgets::{self, row, section, section_with};

/// Indent of each level of groups in the list of layers.
const LEVEL: f32 = 20.0;
/// Where a level's line runs, from the start of its indent.
const TRUNK: f32 = 9.0;

/// What the preset reads of an Aseprite file, which the item's choices go
/// over: a tag, and layers shown and hidden.
#[derive(Default)]
struct Preset {
    tag: Option<String>,
    show: Option<Vec<String>>,
    hide: Option<Vec<String>>,
}

impl Preset {
    fn of(item: &Item, context: &Context) -> Preset {
        let (presets, config) = (context.presets, context.config);
        let choices = &item.choices;
        let read = match item.format {
            Format::Tgs => choices
                .tgs_base(presets, config)
                .map(|options| (options.tag, options.show_layers, options.hide_layers)),
            Format::Webm | Format::Image => choices
                .webm_base(presets, config)
                .map(|options| (options.tag, options.show_layers, options.hide_layers)),
        };
        read.map_or_else(|_| Preset::default(), |(tag, show, hide)| Preset { tag, show, hide })
    }
}

/// The tag and layers to read, if there is a choice.
pub(super) fn aseprite(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let read =
        item.aseprite().map(|(tags, layers, frames)| (tags.to_vec(), layers.to_vec(), frames));
    if read.is_some() {
        item.aseprite_seen.clone_from(&read);
    }
    // as last read, while it is read again
    let read = read.or_else(|| item.aseprite_seen.clone().filter(|_| item.input_reading()));
    let Some((tags, layers, frames)) = read else {
        // a tag or layer the file no longer has, as after it was edited:
        // nothing to choose from until it reads again
        let failed = item.input_error().is_some();
        let choices = &mut item.choices;
        let chosen =
            choices.tag.is_some() || choices.show_layers.is_some() || choices.hide_layers.is_some();
        if !(chosen && failed) {
            return;
        }
        section(ui, "Aseprite file");
        ui.horizontal(|ui| {
            widgets::note(ui, "It doesn't read with the tag and layers chosen");
            if ui.button("Read all of it").clicked() {
                (choices.tag, choices.show_layers, choices.hide_layers) = (None, None, None);
            }
        });
        return;
    };
    // a hidden layer, even the only one, is one to show
    let listed = layers.len() >= 2 || layers.iter().any(|layer| !layer.visible);
    if tags.is_empty() && !listed {
        return;
    }
    // with a sign that it is being read again
    let reading = item.input_reading();
    section_with(ui, "Aseprite file", |ui| {
        if reading {
            ui.add(egui::Spinner::new().size(12.0));
        }
    });
    let preset = Preset::of(item, context);
    let choices = &mut item.choices;
    if !tags.is_empty() {
        let hint = "Only the frames of one tag, in its direction";
        row(ui, "Tag", hint, Ok(()), |ui| {
            let read = choices.tag.clone().or(preset.tag.clone());
            // a preset's tag can be changed here, not left out
            let all = egui::RadioButton::new(read.is_none(), "All frames");
            let all = ui
                .add_enabled(preset.tag.is_none(), all)
                .on_disabled_hover_text("The preset reads one tag");
            if all.clicked() {
                choices.tag = None;
            }
            let picked = egui::ScrollArea::horizontal()
                .id_salt("tags")
                .show(ui, |ui| tag_strip(ui, &tags, frames, read.as_deref()))
                .inner;
            match picked {
                // the one read again: all of it, unless the preset reads it
                Some(name) if read.as_ref() == Some(&name) && preset.tag.is_none() => {
                    choices.tag = None;
                }
                Some(name) => choices.tag = Some(name),
                None => {}
            }
        });
    }
    if listed {
        let hint = "Which layers show, the top one first; a hidden group hides the layers in it";
        row(ui, "Layers", hint, Ok(()), |ui| layer_list(ui, choices, &preset, &layers));
    }
}

/// Width of a frame's cell under the tags, and height of its row.
const CELL: f32 = 18.0;
const CELLS_HIGH: f32 = 16.0;
/// Height of a row of tags.
const LANE: f32 = 20.0;

/// Which row each tag goes in, top first, and how many rows: earlier tags
/// higher, and none where its label or its bracket would cross another's,
/// nor where a higher tag's bracket comes down through it.
fn lanes(spans: &[(f32, f32, f32)]) -> (Vec<usize>, usize) {
    // what each row holds across, and where brackets come down through it
    let mut taken: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut lanes = Vec::new();
    let crosses = |held: &[(f32, f32)], from: f32, to: f32| {
        held.iter().any(|&(start, end)| from < end + 4.0 && start < to + 4.0)
    };
    for &(left, right, end) in spans {
        let lane = (0..=taken.len())
            .find(|&lane| {
                lane == taken.len()
                    || (!crosses(&taken[lane], left, end)
                        && taken[lane + 1..]
                            .iter()
                            .all(|held| !crosses(held, left, left) && !crosses(held, right, right)))
            })
            .unwrap();
        if lane == taken.len() {
            taken.push(Vec::new());
        }
        taken[lane].push((left, end));
        for below in &mut taken[lane + 1..] {
            below.push((left, left));
            below.push((right, right));
        }
        lanes.push(lane);
    }
    let rows = taken.len();
    (lanes, rows)
}

/// The tags of a `frames` long file as Aseprite shows them: a label of each
/// tag's colour, its bracket from there over the frames it holds, and the
/// frames numbered under them, those of the tag `read` lit. The name of a
/// tag clicked, if one is.
fn tag_strip(ui: &mut egui::Ui, tags: &[Tag], frames: usize, read: Option<&str>) -> Option<String> {
    let frames = frames.max(tags.iter().map(|tag| tag.to as usize + 1).max().unwrap_or(0));
    let font = egui::FontId::proportional(12.0);
    let galleys: Vec<_> = tags
        .iter()
        .map(|tag| {
            let [red, green, blue, _] = tag.colour;
            // dark text on light colours, light on dark
            let light = 0.299 * f32::from(red) + 0.587 * f32::from(green) + 0.114 * f32::from(blue);
            let text = if light > 140.0 { egui::Color32::BLACK } else { egui::Color32::WHITE };
            ui.painter().layout_no_wrap(tag.name.clone(), font.clone(), text)
        })
        .collect();
    // across: where its bracket starts and ends, and how far its label goes
    let spans: Vec<(f32, f32, f32)> = tags
        .iter()
        .zip(&galleys)
        .map(|(tag, galley)| {
            let left = tag.from as f32 * CELL + 3.0;
            let right = (tag.to + 1) as f32 * CELL - 3.0;
            (left, right, right.max(left + galley.size().x + 10.0))
        })
        .collect();
    let (lanes, rows) = lanes(&spans);
    let width = (frames as f32 * CELL).max(spans.iter().map(|span| span.2).fold(0.0, f32::max));
    let size = egui::vec2(width, rows as f32 * LANE + CELLS_HIGH + 2.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let visuals = ui.visuals().clone();
    let painter = ui.painter_at(rect.expand(1.0));
    let cells_top = rect.top() + rows as f32 * LANE + 2.0;
    // the frames, numbered from 1 as Aseprite counts them
    let lit = tags.iter().find(|tag| Some(tag.name.as_str()) == read);
    for frame in 0..frames {
        let cell = egui::Rect::from_min_size(
            egui::pos2(rect.left() + frame as f32 * CELL, cells_top),
            egui::vec2(CELL - 1.0, CELLS_HIGH),
        );
        let held = lit.is_some_and(|tag| (tag.from as usize..=tag.to as usize).contains(&frame));
        let fill = if held {
            crate::theme::accent(&visuals).gamma_multiply(0.35)
        } else {
            visuals.extreme_bg_color
        };
        painter.rect_filled(cell, 1.0, fill);
        painter.text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            (frame + 1).to_string(),
            egui::FontId::proportional(10.0),
            visuals.weak_text_color(),
        );
    }
    let mut picked = None;
    for (((tag, galley), &(left, right, _)), &lane) in
        tags.iter().zip(&galleys).zip(&spans).zip(&lanes)
    {
        let [red, green, blue, alpha] = tag.colour;
        let alpha = if alpha == 0 { 255 } else { alpha };
        let colour = egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha);
        let top = rect.top() + lane as f32 * LANE;
        let label = egui::Rect::from_min_size(
            egui::pos2(rect.left() + left - 3.0, top + 1.0),
            galley.size() + egui::vec2(8.0, 3.0),
        );
        // the bracket: down from the label's middle at the first frame, and
        // at the last, joined along the top
        let stroke = egui::Stroke::new(2.0, colour);
        let middle = label.center().y;
        let (left, right) = (rect.left() + left, rect.left() + right);
        painter.hline(left..=right, middle, stroke);
        painter.vline(left, middle..=cells_top - 1.0, stroke);
        painter.vline(right, middle..=cells_top - 1.0, stroke);
        let response = ui
            .interact(label, ui.id().with(("tag", &tag.name)), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let picked_now = Some(tag.name.as_str()) == read;
        let edge = if picked_now || response.hovered() {
            egui::Stroke::new(1.5, visuals.strong_text_color())
        } else {
            egui::Stroke::NONE
        };
        painter.rect(label, 3.0, colour, edge, egui::StrokeKind::Outside);
        painter.galley(label.min + egui::vec2(4.0, 1.5), galley.clone(), egui::Color32::BLACK);
        let direction = match tag.direction {
            Direction::Forward => "",
            Direction::Reverse => ", in reverse",
            Direction::PingPong => ", ping-pong",
            Direction::PingPongReverse => ", ping-pong in reverse",
        };
        // as Aseprite counts them, from 1
        let (from, to) = (tag.from + 1, tag.to + 1);
        let held =
            if from == to { format!("Frame {from}") } else { format!("Frames {from} to {to}") };
        if response.on_hover_text(format!("{held}{direction}")).clicked() {
            picked = Some(tag.name.clone());
        }
    }
    picked
}

/// Whether each layer is the last of its group's, or of the top level's.
fn last_of_group(layers: &[LayerInfo]) -> Vec<bool> {
    (0..layers.len())
        .map(|index| {
            let level = layers[index].level;
            // another at its level, before the group ends
            !layers[index + 1..]
                .iter()
                .take_while(|layer| layer.level >= level)
                .any(|layer| layer.level == level)
        })
        .collect()
}

/// A box for each layer, under its group, ticked where it shows, with
/// lines joining each to its group, as `tree` draws them.
fn layer_list(ui: &mut egui::Ui, choices: &mut Choices, preset: &Preset, layers: &[LayerInfo]) {
    // a list, closer than rows of settings
    ui.spacing_mut().item_spacing.y = 2.0;
    let last = last_of_group(layers);
    // for each level above the layer's, whether its line goes on below
    let mut going_on: Vec<bool> = Vec::new();
    // the level of the group hiding the layers listed under it
    let mut hidden_from: Option<u16> = None;
    for (index, layer) in layers.iter().enumerate() {
        if hidden_from.is_some_and(|level| layer.level <= level) {
            hidden_from = None;
        }
        let level = usize::from(layer.level);
        going_on.truncate(level);
        let mut on = shows(&named(choices, preset), layer);
        let row = ui.horizontal(|ui| {
            ui.add_space(LEVEL * (level + 1) as f32);
            let name = RichText::new(&layer.name);
            let name = if layer.group { name.strong() } else { name };
            let response = ui
                .add_enabled(hidden_from.is_none(), egui::Checkbox::new(&mut on, name))
                .on_disabled_hover_text("Its group is hidden");
            if response.changed() {
                set(choices, preset, layer, on);
            }
            // the first ends with a way back to the preset's or the file's
            let back = match (&preset.show, &preset.hide) {
                (None, None) => "Back to the layers the file shows",
                _ => "Back to the layers the preset shows",
            };
            if index == 0
                && (choices.show_layers.is_some() || choices.hide_layers.is_some())
                && widgets::reset_to(ui, back)
            {
                (choices.show_layers, choices.hide_layers) = (None, None);
            }
        });
        // the lines: of the groups above it that go on, and its own
        let rect = row.response.rect;
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let painter = ui.painter();
        let x = |level: usize| rect.left() + LEVEL * level as f32 + TRUNK;
        let (top, bottom) = (rect.top() - 1.0, rect.bottom() + 1.0);
        for (above, _) in going_on.iter().enumerate().filter(|(_, on)| **on) {
            painter.vline(x(above), top..=bottom, stroke);
        }
        let middle = rect.center().y;
        let end = if last[index] { middle } else { bottom };
        painter.vline(x(level), top..=end, stroke);
        painter.hline(x(level)..=rect.left() + LEVEL * (level + 1) as f32 - 4.0, middle, stroke);
        going_on.push(!last[index]);
        if layer.group && !on && hidden_from.is_none() {
            hidden_from = Some(layer.level);
        }
    }
}

/// The layers shown and hidden by name: the item's, or else the preset's.
fn named(choices: &Choices, preset: &Preset) -> (Vec<String>, Vec<String>) {
    let list = |item: &Option<Vec<String>>, preset: &Option<Vec<String>>| {
        item.as_ref().or(preset.as_ref()).cloned().unwrap_or_default()
    };
    (list(&choices.show_layers, &preset.show), list(&choices.hide_layers, &preset.hide))
}

/// Whether `layer` shows: as the file says, unless it is named.
fn shows((show, hide): &(Vec<String>, Vec<String>), layer: &LayerInfo) -> bool {
    !hide.contains(&layer.name) && (layer.visible || show.contains(&layer.name))
}

/// Shows or hides every layer named as `layer` is, naming it only where
/// that differs from the file, and keeping to the preset's lists where
/// they say the same.
fn set(choices: &mut Choices, preset: &Preset, layer: &LayerInfo, on: bool) {
    let (mut show, mut hide) = named(choices, preset);
    show.retain(|name| *name != layer.name);
    hide.retain(|name| *name != layer.name);
    if on != layer.visible {
        if on { &mut show } else { &mut hide }.push(layer.name.clone());
    }
    let own = |list: Vec<String>, preset: &Option<Vec<String>>| {
        (list != preset.clone().unwrap_or_default()).then_some(list)
    };
    choices.show_layers = own(show, &preset.show);
    choices.hide_layers = own(hide, &preset.hide);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::telegram::Target;

    #[test]
    fn names_layers_only_where_they_differ_from_the_file() {
        let layer =
            |name: &str, visible| LayerInfo { name: name.into(), group: false, level: 0, visible };
        let (shadow, sketch) = (layer("Shadow", true), layer("Sketch", false));
        let none = Preset::default();
        let mut choices = Choices::new(Target::Sticker);
        set(&mut choices, &none, &shadow, false);
        set(&mut choices, &none, &sketch, true);
        assert_eq!(choices.hide_layers, Some(vec!["Shadow".into()]));
        assert_eq!(choices.show_layers, Some(vec!["Sketch".into()]));
        let both = named(&choices, &none);
        assert!(!shows(&both, &shadow) && shows(&both, &sketch));
        // back as the file has them, nothing is named
        set(&mut choices, &none, &shadow, true);
        set(&mut choices, &none, &sketch, false);
        assert_eq!((&choices.show_layers, &choices.hide_layers), (&None, &None));

        // a preset showing the sketch: ticked, and hidden over it
        let preset = Preset { show: Some(vec!["Sketch".into()]), ..Preset::default() };
        assert!(shows(&named(&choices, &preset), &sketch));
        set(&mut choices, &preset, &sketch, false);
        assert_eq!(choices.show_layers, Some(Vec::new()));
        assert!(!shows(&named(&choices, &preset), &sketch));
        // shown again, as the preset has it
        set(&mut choices, &preset, &sketch, true);
        assert_eq!((choices.show_layers, choices.hide_layers), (None, None));
    }
}

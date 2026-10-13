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
/// Side of a tag's colour swatch.
const SWATCH: f32 = 10.0;

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
    let read = item.aseprite().map(|(tags, layers)| (tags.to_vec(), layers.to_vec()));
    if read.is_some() {
        item.aseprite_seen.clone_from(&read);
    }
    // as last read, while it is read again
    let read = read.or_else(|| item.aseprite_seen.clone().filter(|_| item.input_reading()));
    let Some((tags, layers)) = read else {
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
            // as many as the file has, and as long: radios wrap
            ui.horizontal_wrapped(|ui| {
                // a preset's tag can be changed here, not left out
                let all = egui::RadioButton::new(read.is_none(), "All frames");
                let all = ui
                    .add_enabled(preset.tag.is_none(), all)
                    .on_disabled_hover_text("The preset reads one tag");
                if all.clicked() {
                    choices.tag = None;
                }
                for tag in &tags {
                    if tag_radio(ui, read.as_ref() == Some(&tag.name), tag).clicked() {
                        choices.tag = Some(tag.name.clone());
                    }
                }
            });
        });
    }
    if listed {
        let hint = "Which layers show, the top one first; a hidden group hides the layers in it";
        row(ui, "Layers", hint, Ok(()), |ui| layer_list(ui, choices, &preset, &layers));
    }
}

/// A tag to pick, its colour beside its name, its frames on hover.
fn tag_radio(ui: &mut egui::Ui, selected: bool, tag: &Tag) -> egui::Response {
    // room for the swatch before the name
    let response = ui.radio(selected, format!("     {}", tag.name));
    if tag.colour[3] > 0 {
        let spacing = ui.spacing();
        let left = response.rect.left() + spacing.icon_width + spacing.icon_spacing + 1.0;
        let swatch = egui::Rect::from_center_size(
            egui::pos2(left + SWATCH / 2.0, response.rect.center().y),
            egui::vec2(SWATCH, SWATCH),
        );
        let [red, green, blue, alpha] = tag.colour;
        let colour = egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha);
        let outline = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter().rect(swatch, 2.0, colour, outline, egui::StrokeKind::Inside);
    }
    let direction = match tag.direction {
        Direction::Forward => "",
        Direction::Reverse => ", in reverse",
        Direction::PingPong => ", ping-pong",
        Direction::PingPongReverse => ", ping-pong in reverse",
    };
    // as Aseprite counts them, from 1
    let (from, to) = (tag.from + 1, tag.to + 1);
    let frames =
        if from == to { format!("Frame {from}") } else { format!("Frames {from} to {to}") };
    response.on_hover_text(format!("{frames}{direction}"))
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

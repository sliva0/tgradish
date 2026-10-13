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
    let read = item
        .aseprite()
        .map(|(tags, layers, durations)| (tags.to_vec(), layers.to_vec(), durations.to_vec()));
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
    let format = item.format;
    let choices = &mut item.choices;
    if !tags.is_empty() {
        let hint = "Picks the frames of a tag: as where it starts and ends, or read in its \
                    own order where it plays backwards or there and back";
        row(ui, "Tag", hint, Ok(()), |ui| tag_row(ui, choices, &preset, &tags, &frames, format));
    }
    if listed {
        let hint = "Which layers show, the top one first; a hidden group hides the layers in it";
        row(ui, "Layers", hint, Ok(()), |ui| layer_list(ui, choices, &preset, &layers));
    }
}

/// The tags to pick, as buttons of their colours. One played forward
/// sets where the input starts and ends, or for a picture, the frame it
/// is; one that plays backwards, or there and back, is read in its own
/// order, which a cut can't do.
fn tag_row(
    ui: &mut egui::Ui,
    choices: &mut Choices,
    preset: &Preset,
    tags: &[Tag],
    durations: &[f64],
    format: Format,
) {
    // where each frame starts, then where the last ends
    let mut starts = vec![0.0];
    for duration in durations {
        starts.push(starts.last().unwrap() + duration);
    }
    let total = *starts.last().unwrap();
    let span = |tag: &Tag| {
        let at = |frame: usize| starts.get(frame).copied().unwrap_or(total);
        (at(tag.from as usize), at(tag.to as usize + 1))
    };
    let read = choices.tag.clone().or(preset.tag.clone());
    let picture = format == Format::Image;
    // the part used, which a tag played forward sets
    let start = choices.start.unwrap_or(0.0);
    let end = choices.length.map_or(total, |length| start + length);
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let whole = read.is_none() && (picture || (near(start, 0.0) && near(end, total)));
    let picked = |tag: &Tag| match tag.direction {
        Direction::Forward if read.is_none() => {
            let (from, to) = span(tag);
            match picture {
                true => choices.frame.is_some_and(|frame| frame >= from - 1e-6 && frame < to),
                false => near(start, from) && near(end, to),
            }
        }
        _ => read.as_deref() == Some(tag.name.as_str()),
    };
    let picks: Vec<bool> = tags.iter().map(picked).collect();
    ui.horizontal_wrapped(|ui| {
        // a preset's tag can be changed here, not left out
        let all = egui::Button::new("All frames").selected(whole && !picks.contains(&true));
        let all = ui
            .add_enabled(preset.tag.is_none(), all)
            .on_hover_text("Every frame of the file")
            .on_disabled_hover_text("The preset reads one tag");
        if all.clicked() {
            choices.tag = None;
            (choices.start, choices.length) = (None, None);
        }
        for (tag, &picked) in tags.iter().zip(&picks) {
            let (from, to) = span(tag);
            if !tag_button(ui, tag, picked).clicked() {
                continue;
            }
            match tag.direction {
                // as where the input starts and ends, or its frame
                Direction::Forward if picked && !picture => {
                    (choices.start, choices.length) = (None, None)
                }
                Direction::Forward => {
                    choices.tag = None;
                    match picture {
                        true => choices.frame = Some(from),
                        false => (choices.start, choices.length) = (Some(from), Some(to - from)),
                    }
                }
                // read in its order, the whole of which is used
                _ if picked => choices.tag = None,
                _ => {
                    choices.tag = Some(tag.name.clone());
                    (choices.start, choices.length) = (None, None);
                }
            }
        }
    });
}

/// A button of a tag's colour, outlined when `picked`, its frames on
/// hover.
fn tag_button(ui: &mut egui::Ui, tag: &Tag, picked: bool) -> egui::Response {
    // opaque, as Aseprite shows them
    let [red, green, blue, _] = tag.colour;
    let colour = egui::Color32::from_rgb(red, green, blue);
    // dark text on light colours, light on dark
    let light = 0.299 * f32::from(red) + 0.587 * f32::from(green) + 0.114 * f32::from(blue);
    let text = if light > 140.0 { egui::Color32::BLACK } else { egui::Color32::WHITE };
    let galley = ui.painter().layout_no_wrap(
        tag.name.clone(),
        egui::TextStyle::Button.resolve(ui.style()),
        text,
    );
    let size = galley.size() + egui::vec2(16.0, 6.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, picked, &tag.name)
    });
    let edge = match (picked, response.hovered()) {
        (true, _) => egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
        (false, true) => egui::Stroke::new(1.0, ui.visuals().strong_text_color()),
        (false, false) => egui::Stroke::NONE,
    };
    // inside, so nothing around it cuts the outline off
    ui.painter().rect(rect, 4.0, colour, edge, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, text);
    let direction = match tag.direction {
        Direction::Forward => "",
        Direction::Reverse => ", in reverse",
        Direction::PingPong => ", there and back",
        Direction::PingPongReverse => ", back and there",
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

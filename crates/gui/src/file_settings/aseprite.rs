//! What of an Aseprite file is read, whatever it is made into: the frames
//! of one tag, and which layers show.

use std::rc::Rc;

use eframe::egui::{self, RichText};
use tgradish_core::presets::Format;
use tgradish_core::tgradish_tgs::frames::aseprite::{Direction, LayerInfo, Tag};

use crate::cuts::{Cut, Frames};
use crate::item::{Choices, Item};
use crate::l10n::fl;
use crate::widgets::{self, row, section, section_with};

/// Indent of each level of groups in the list of layers.
const LEVEL: f32 = 20.0;
/// Where a level's line runs, from the start of its indent.
const TRUNK: f32 = 9.0;

/// The tag and layers to read, if there is a choice.
pub(super) fn aseprite(ui: &mut egui::Ui, item: &mut Item) {
    // kept, copied only when it changes, to show while it is read again
    let fresh = item.aseprite().map(|(tags, layers, durations)| {
        let seen = item.aseprite_seen.as_deref();
        let same =
            seen.is_some_and(|(t, l, d)| (&t[..], &l[..], &d[..]) == (tags, layers, durations));
        (!same).then(|| Rc::new((tags.to_vec(), layers.to_vec(), durations.to_vec())))
    });
    let read = fresh.is_some();
    if let Some(Some(changed)) = fresh {
        item.aseprite_seen = Some(changed);
    }
    let shown = read || item.input_reading();
    let read = item.aseprite_seen.clone().filter(|_| shown);
    let Some(read) = read else {
        // a tag or layer the file no longer has, as after it was edited:
        // nothing to choose from until it reads again
        let failed = item.input_error().is_some();
        let choices = &mut item.choices;
        let chosen =
            choices.tag.is_some() || choices.show_layers.is_some() || choices.hide_layers.is_some();
        if !(chosen && failed) {
            return;
        }
        section(ui, &fl!("aseprite-file"));
        ui.horizontal(|ui| {
            widgets::note(ui, fl!("unreadable-choices"));
            if ui.button(fl!("read-all")).clicked() {
                (choices.tag, choices.show_layers, choices.hide_layers) = (None, None, None);
            }
        });
        return;
    };
    let (tags, layers, frames) = &*read;
    // a hidden layer, even the only one, is one to show
    let listed = layers.len() >= 2 || layers.iter().any(|layer| !layer.visible);
    if tags.is_empty() && !listed {
        return;
    }
    // with a sign that it is being read again
    let reading = item.input_reading();
    section_with(ui, &fl!("aseprite-file"), |ui| {
        if reading {
            ui.add(egui::Spinner::new().size(12.0));
        }
    });
    let format = item.format;
    let choices = &mut item.choices;
    if !tags.is_empty() {
        row(ui, &fl!("tag"), &fl!("tag", "hint"), Ok(()), |ui| {
            tag_row(ui, choices, tags, frames, format)
        });
    }
    if listed {
        row(ui, &fl!("layers"), &fl!("layers", "hint"), Ok(()), |ui| {
            layer_list(ui, choices, layers)
        });
    }
}

/// The frames of an Aseprite file that shows each for `durations`, as the
/// timeline cuts them for `format`: drawn for ffmpeg, as a video's.
fn frames_of(durations: &[f64], format: Format) -> Option<Frames> {
    let mut times = vec![0.0];
    for duration in durations {
        times.push(times.last().unwrap() + duration);
    }
    let cut = if format == Format::Tgs { Cut::Art } else { Cut::Video };
    (times.len() >= 2).then_some(Frames { times, cut })
}

/// The tags to pick, as buttons of their colours. One played forward
/// sets where the input starts and ends, or for a picture, the frame it
/// is, as the timeline writes them; one that plays backwards, or there and
/// back, is read in its own order, which a cut can't do.
fn tag_row(
    ui: &mut egui::Ui,
    choices: &mut Choices,
    tags: &[Tag],
    durations: &[f64],
    format: Format,
) {
    let Some(frames) = frames_of(durations, format) else { return };
    let total = frames.times[frames.count()];
    let count = frames.count();
    // as boundaries: its first frame and the one after its last
    let span = |tag: &Tag| {
        let from = (tag.from as usize).min(count - 1);
        (from, (tag.to as usize + 1).clamp(from + 1, count))
    };
    let read = choices.tag.clone();
    let picture = format == Format::Webp;
    // the part used, which a tag played forward sets
    let used = frames.part(total, choices.start, choices.length);
    let shown = frames.at(choices.frame.unwrap_or(0.0));
    let by_name = |tag: &Tag| tag.direction != Direction::Forward;
    let whole = read.is_none() && (picture || used == (0, count));
    let picked = |tag: &Tag| match by_name(tag) || read.is_some() {
        false if picture => (span(tag).0..span(tag).1).contains(&shown),
        false => used == span(tag),
        true => read.as_deref() == Some(tag.name.as_str()),
    };
    let picks: Vec<bool> = tags.iter().map(picked).collect();
    ui.horizontal_wrapped(|ui| {
        let all = egui::Button::new(fl!("all-frames")).selected(whole && !picks.contains(&true));
        let all = ui.add(all).on_hover_text(fl!("all-frames", "hint"));
        if all.clicked() {
            choices.tag = None;
            (choices.start, choices.length) = (None, None);
        }
        for (tag, &picked) in tags.iter().zip(&picks) {
            let (from, after) = span(tag);
            if !tag_button(ui, tag, picked).clicked() {
                continue;
            }
            match by_name(tag) {
                // read in its order, the whole of which is used
                true if picked => choices.tag = None,
                true => {
                    choices.tag = Some(tag.name.clone());
                    (choices.start, choices.length) = (None, None);
                }
                // as where the input starts and ends, or its frame
                false if picked && !picture => (choices.start, choices.length) = (None, None),
                false => {
                    choices.tag = None;
                    match picture {
                        true => choices.frame = (from > 0).then_some(frames.times[from]),
                        false => {
                            frames.set_part((from, after), &mut choices.start, &mut choices.length)
                        }
                    }
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
    // as Aseprite counts them, from 1
    let (from, to) = (tag.from + 1, tag.to + 1);
    let frames = if from == to {
        fl!("tag-frame", number = from)
    } else {
        fl!("tag-frames", from = from, to = to)
    };
    response.on_hover_text(match tag.direction {
        Direction::Forward => frames,
        Direction::Reverse => fl!("tag-reverse", frames = frames),
        Direction::PingPong => fl!("tag-ping-pong", frames = frames),
        Direction::PingPongReverse => fl!("tag-pong-ping", frames = frames),
    })
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
fn layer_list(ui: &mut egui::Ui, choices: &mut Choices, layers: &[LayerInfo]) {
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
        let mut on = shows(&named(choices), layer);
        let row = ui.horizontal(|ui| {
            ui.add_space(LEVEL * (level + 1) as f32);
            // as Aseprite writes them, the background underlined
            let name = RichText::new(&layer.name).monospace();
            let name = if layer.group { name.strong() } else { name };
            let name = if layer.background { name.underline() } else { name };
            let response = ui
                .add_enabled(hidden_from.is_none(), egui::Checkbox::new(&mut on, name))
                .on_disabled_hover_text(fl!("layers", "group-hidden"));
            if response.changed() {
                set(choices, layer, on);
            }
            // the first ends with a way back to the file's
            let own = choices.show_layers.is_some() || choices.hide_layers.is_some();
            if index == 0 && widgets::reset_to(ui, own, &fl!("layers", "file")) {
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

/// The layers the item shows and hides by name.
fn named(choices: &Choices) -> (Vec<String>, Vec<String>) {
    let list = |names: &Option<Vec<String>>| names.clone().unwrap_or_default();
    (list(&choices.show_layers), list(&choices.hide_layers))
}

/// Whether `layer` shows: as the file says, unless it is named.
fn shows((show, hide): &(Vec<String>, Vec<String>), layer: &LayerInfo) -> bool {
    !hide.contains(&layer.name) && (layer.visible || show.contains(&layer.name))
}

/// Shows or hides every layer named as `layer` is, naming it only where
/// that differs from the file.
fn set(choices: &mut Choices, layer: &LayerInfo, on: bool) {
    let (mut show, mut hide) = named(choices);
    show.retain(|name| *name != layer.name);
    hide.retain(|name| *name != layer.name);
    if on != layer.visible {
        if on { &mut show } else { &mut hide }.push(layer.name.clone());
    }
    choices.show_layers = (!show.is_empty()).then_some(show);
    choices.hide_layers = (!hide.is_empty()).then_some(hide);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::telegram::Target;

    #[test]
    fn cuts_tags_as_the_timeline_does() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::{NodeT, Queryable};

        let tag = |name: &str, from, to| {
            let mut tag = Tag::default();
            (tag.name, tag.from, tag.to) = (name.into(), from, to);
            tag.colour = [200, 80, 80, 255];
            tag
        };
        let tags = [tag("idle", 0, 1), tag("walk", 2, 5)];
        let durations = [0.1; 8];
        let run = |choices: Choices, format: Format| {
            let tags = tags.to_vec();
            let mut harness = Harness::new_ui_state(
                move |ui, choices: &mut Choices| {
                    tag_row(ui, choices, &tags, &durations, format);
                },
                choices,
            );
            harness.run_steps(2);
            harness
        };
        // egui says a button is picked as toggled
        let picked = |harness: &Harness<Choices>, name: &str| {
            let toggled = harness.get_by_label(name).accesskit_node().toggled();
            toggled == Some(egui::accesskit::Toggled::True)
        };
        for format in [Format::Webm, Format::Tgs] {
            // written as the timeline writes the frames, so converting keeps
            // them, and still picked once the timeline writes them again
            let mut harness = run(Choices::new(Target::Sticker), format);
            harness.get_by_label("walk").click();
            harness.run_steps(2);
            let frames = frames_of(&durations, format).unwrap();
            let (mut start, mut length) = (None, None);
            frames.set_part((2, 6), &mut start, &mut length);
            let choices = harness.state();
            assert_eq!((choices.start, choices.length), (start, length), "{format:?}");
            assert_eq!(frames.part(0.8, start, length), (2, 6));
            assert!(picked(&harness, "walk") && !picked(&harness, "idle"));
        }
        // a picture's frame starts where the picker puts it
        let mut harness = run(Choices::new(Target::Sticker), Format::Webp);
        harness.get_by_label("walk").click();
        harness.run_steps(2);
        let frames = frames_of(&durations, Format::Webp).unwrap();
        assert_eq!(harness.state().frame, Some(frames.times[2]));
        assert!(picked(&harness, "walk"));
    }

    #[test]
    fn names_layers_only_where_they_differ_from_the_file() {
        let layer = |name: &str, visible| {
            let mut layer = LayerInfo::default();
            (layer.name, layer.visible) = (name.into(), visible);
            layer
        };
        let (shadow, sketch) = (layer("Shadow", true), layer("Sketch", false));
        let mut choices = Choices::new(Target::Sticker);
        set(&mut choices, &shadow, false);
        set(&mut choices, &sketch, true);
        assert_eq!(choices.hide_layers, Some(vec!["Shadow".into()]));
        assert_eq!(choices.show_layers, Some(vec!["Sketch".into()]));
        let both = named(&choices);
        assert!(!shows(&both, &shadow) && shows(&both, &sketch));
        // back as the file has them, nothing is named
        set(&mut choices, &shadow, true);
        set(&mut choices, &sketch, false);
        assert_eq!((&choices.show_layers, &choices.hide_layers), (&None, &None));
    }
}

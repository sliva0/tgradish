//! What of an Aseprite file is read, whatever it is made into: the frames
//! of one tag, and which layers show.

use eframe::egui::{self, RichText};
use tgradish_core::presets::Format;
use tgradish_tgs::frames::LayerInfo;

use super::Context;
use crate::item::{Choices, Item};
use crate::widgets::{self, heading, row};

/// Indent of each level of groups in the list of layers.
const LEVEL: f32 = 18.0;

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

/// The tag and layers to read, first of the settings: false when there is
/// nothing to choose.
pub(super) fn aseprite(ui: &mut egui::Ui, item: &mut Item, context: &Context) -> bool {
    let Some((tags, layers)) =
        item.aseprite().map(|(tags, layers)| (tags.to_vec(), layers.to_vec()))
    else {
        // a tag or layer the file no longer has, as after it was edited:
        // nothing to choose from until it reads again
        let failed = item.input_error().is_some();
        let choices = &mut item.choices;
        let chosen =
            choices.tag.is_some() || choices.show_layers.is_some() || choices.hide_layers.is_some();
        if !(chosen && failed) {
            return false;
        }
        heading(ui, "Aseprite file");
        ui.horizontal(|ui| {
            widgets::note(ui, "It doesn't read with the tag and layers chosen");
            if ui.button("Read all of it").clicked() {
                (choices.tag, choices.show_layers, choices.hide_layers) = (None, None, None);
            }
        });
        return true;
    };
    // a hidden layer, even the only one, is one to show
    let listed = layers.len() >= 2 || layers.iter().any(|layer| !layer.visible);
    if tags.is_empty() && !listed {
        return false;
    }
    heading(ui, "Aseprite file");
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
                    if ui.radio(read.as_ref() == Some(tag), tag).clicked() {
                        choices.tag = Some(tag.clone());
                    }
                }
            });
        });
    }
    if listed {
        let hint = "Which layers show, the top one first; a hidden group hides the layers in it";
        row(ui, "Layers", hint, Ok(()), |ui| layer_list(ui, choices, &preset, &layers));
    }
    true
}

/// A box for each layer, under its group, ticked where it shows.
fn layer_list(ui: &mut egui::Ui, choices: &mut Choices, preset: &Preset, layers: &[LayerInfo]) {
    // a list, closer than rows of settings
    ui.spacing_mut().item_spacing.y = 0.0;
    // the level of the group hiding the layers listed under it
    let mut hidden_from: Option<u16> = None;
    for layer in layers {
        if hidden_from.is_some_and(|level| layer.level <= level) {
            hidden_from = None;
        }
        let mut on = shows(&named(choices, preset), layer);
        ui.horizontal(|ui| {
            ui.add_space(LEVEL * f32::from(layer.level));
            let name = RichText::new(&layer.name);
            let name = if layer.group { name.strong() } else { name };
            let response = ui
                .add_enabled(hidden_from.is_none(), egui::Checkbox::new(&mut on, name))
                .on_disabled_hover_text("Its group is hidden");
            if response.changed() {
                set(choices, preset, layer, on);
            }
            // the last box ends with a way back to the preset's or file's
            let back = match (&preset.show, &preset.hide) {
                (None, None) => "Back to the layers the file shows",
                _ => "Back to the layers the preset shows",
            };
            if std::ptr::eq(layer, layers.last().unwrap())
                && (choices.show_layers.is_some() || choices.hide_layers.is_some())
                && ui
                    .add(egui::Button::new(RichText::new("⟲").size(13.0)).small().frame(false))
                    .on_hover_text(back)
                    .clicked()
            {
                (choices.show_layers, choices.hide_layers) = (None, None);
            }
        });
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

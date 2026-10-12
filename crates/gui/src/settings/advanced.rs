//! Encoder options for WebM, which most people never need, so they sit
//! folded away: libvpx-vp9 options by name, and raw ffmpeg arguments
//! where the system's ffmpeg runs.

use eframe::egui::{self, RichText};
use tgradish_core::options::Options;

use super::Context;
use crate::item::Item;
use crate::widgets::{self, note, row};

/// libvpx-vp9 options worth knowing, offered to add with a click.
const ENCODER_EXAMPLES: [(&str, &str, &str); 7] = [
    ("tune-content", "screen", "Flat graphics and screen recordings: sharper edges"),
    ("aq-mode", "2", "Spends bits where the picture is complex"),
    ("sharpness", "4", "Keeps more fine detail, at some cost in smoothness"),
    ("arnr-strength", "3", "Less noise reduction across frames"),
    ("lag-in-frames", "25", "Looks further ahead to spend bits where they count"),
    ("g", "60", "A keyframe at least every 60 frames"),
    ("qmax", "50", "Never worse than quality 50 anywhere"),
];

pub(super) fn advanced(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new("Encoder options").strong()).id_salt("advanced").show(ui, |ui| {
        let options: &mut Options = &mut item.choices.webm;
        {
            let hint = "Encoder options by name, like tune-content = screen for flat graphics. ffmpeg -h encoder=libvpx-vp9 lists them";
            row(ui, "libvpx-vp9", hint, Ok(()), |ui| {
                let mut pairs: Vec<(String, String)> = options.encoder_options.clone().unwrap_or_default().into_iter().collect();
                let mut changed = false;
                let mut remove = None;
                for (index, (name, value)) in pairs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(name).hint_text("For example: tune-content").desired_width(190.0)).changed();
                        ui.label("=");
                        changed |= ui.add(egui::TextEdit::singleline(value).hint_text("For example: screen").desired_width(150.0)).changed();
                        if ui.small_button("✖").on_hover_text("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    pairs.remove(index);
                    changed = true;
                }
                if ui.small_button("Add an option").clicked() {
                    pairs.push((String::new(), String::new()));
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    widgets::note(ui, "Examples:");
                    for (name, value, hint) in ENCODER_EXAMPLES {
                        if ui.small_button(format!("{name}={value}")).on_hover_text(hint).clicked() {
                            match pairs.iter_mut().find(|(other, _)| other == name) {
                                Some(pair) => pair.1 = value.to_owned(),
                                None => pairs.push((name.to_owned(), value.to_owned())),
                            }
                            changed = true;
                        }
                    }
                });
                if changed {
                    options.encoder_options = (!pairs.is_empty()).then(|| pairs.into_iter().collect());
                }
            });
        }

        let system = "Only with the system's ffmpeg, which Settings can pick";
        let allowed = if context.extra_args { Ok(()) } else { Err(system) };
        let hint = "Raw arguments added before the output, split like a shell would";
        row(ui, "ffmpeg arguments", hint, allowed, |ui| {
            let mut text = options.extra_args.as_ref().map(|words| shlex::try_join(words.iter().map(String::as_str)).unwrap_or_default()).unwrap_or_default();
            let edit = ui.add(egui::TextEdit::singleline(&mut text).hint_text("For example: -lag-in-frames 25").desired_width(300.0));
            if edit.changed() {
                options.extra_args = shlex::split(&text).filter(|words| !words.is_empty());
            }
            if !context.extra_args {
                note(ui, system);
            }
        });
    });
}

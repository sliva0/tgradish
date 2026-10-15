//! Encoder options for WebM, which most people never need, so they sit
//! folded away: libvpx-vp9 options by name, and raw ffmpeg arguments
//! where the system's ffmpeg runs.

use eframe::egui::{self, RichText};
use tgradish_core::options::Options;

use super::Context;
use crate::item::Item;
use crate::l10n::fl;
use crate::widgets::{self, note, row};

/// libvpx-vp9 options worth knowing, offered to add with a click.
fn encoder_examples() -> [(&'static str, &'static str, String); 6] {
    [
        ("tune-content", "screen", fl!("example-tune-content")),
        ("aq-mode", "2", fl!("example-aq-mode")),
        ("sharpness", "4", fl!("example-sharpness")),
        ("arnr-strength", "3", fl!("example-arnr-strength")),
        ("g", "60", fl!("example-g")),
        ("qmax", "50", fl!("example-qmax")),
    ]
}

pub(super) fn advanced(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new(fl!("encoder-options")).strong())
        .id_salt("advanced")
        .show(ui, |ui| {
            let options: &mut Options = &mut item.choices.webm;
            {
                let hint = fl!("vp9-options");
                row(ui, "libvpx-vp9", &hint, Ok(()), |ui| {
                    let mut pairs: Vec<(String, String)> =
                        options.encoder_options.clone().unwrap_or_default().into_iter().collect();
                    let mut changed = false;
                    let mut remove = None;
                    for (index, (name, value)) in pairs.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(name)
                                        .hint_text(fl!("vp9-options", "name"))
                                        .desired_width(190.0),
                                )
                                .changed();
                            ui.label("=");
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(value)
                                        .hint_text(fl!("vp9-options", "value"))
                                        .desired_width(150.0),
                                )
                                .changed();
                            if ui.small_button("✖").on_hover_text(fl!("remove")).clicked() {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        pairs.remove(index);
                        changed = true;
                    }
                    if ui.small_button(fl!("add-option")).clicked() {
                        pairs.push((String::new(), String::new()));
                        changed = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        widgets::note(ui, fl!("examples"));
                        for (name, value, hint) in encoder_examples() {
                            if ui
                                .small_button(format!("{name}={value}"))
                                .on_hover_text(hint)
                                .clicked()
                            {
                                match pairs.iter_mut().find(|(other, _)| other == name) {
                                    Some(pair) => pair.1 = value.to_owned(),
                                    None => pairs.push((name.to_owned(), value.to_owned())),
                                }
                                changed = true;
                            }
                        }
                    });
                    if changed {
                        options.encoder_options =
                            (!pairs.is_empty()).then(|| pairs.into_iter().collect());
                    }
                });
            }

            let system = fl!("ffmpeg-arguments", "system");
            let allowed = if context.extra_args { Ok(()) } else { Err(system.as_str()) };
            row(ui, &fl!("ffmpeg-arguments"), &fl!("ffmpeg-arguments", "hint"), allowed, |ui| {
                let mut text = options
                    .extra_args
                    .as_ref()
                    .map(|words| {
                        shlex::try_join(words.iter().map(String::as_str)).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text(fl!("ffmpeg-arguments", "placeholder"))
                        .desired_width(300.0),
                );
                if edit.changed() {
                    options.extra_args = shlex::split(&text).filter(|words| !words.is_empty());
                }
                if !context.extra_args {
                    note(ui, system.as_str());
                }
            });
        });
}

//! The bar under the selected item: converting it, stopping or not
//! converting it after all, what went wrong last time, and where its
//! result goes.

use eframe::egui::{self, RichText, vec2};

use crate::app::App;
use crate::fonts::{drawable, missing_note, text_format};
use crate::jobs::Status;
use crate::l10n::fl;
use crate::outputs::Written;
use crate::{system, widgets};

/// Room the right of the bar takes, at least: the result's name and the
/// button that shows its folder.
const SAVES_TO: f32 = 300.0;

impl App {
    pub(crate) fn actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let output = self.item(id).map(|item| self.output_for(item));
        let queued = self.queue.iter().position(|&other| other == id);
        let presets_kept = self.presets_dir.is_some();
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        let mut convert = false;
        let mut replace = None;
        ui.horizontal(|ui| {
            match &item.job {
                Some((job, ..)) if job.is_running() => {
                    // until the conversion gets to its next check
                    let stopping = job.is_stopping();
                    let text = if stopping { fl!("stopping") } else { fl!("stop") };
                    let stop = egui::Button::new(text).min_size(vec2(140.0, 32.0));
                    if ui.add_enabled(!stopping, stop).clicked() {
                        job.cancel();
                    }
                    widgets::note(ui, fl!("converting-note"));
                }
                Some((job, ..)) => {
                    if ui
                        .add(egui::Button::new(fl!("dont-convert")).min_size(vec2(140.0, 32.0)))
                        .clicked()
                    {
                        job.cancel();
                        item.job = None;
                        self.queue.retain(|&other| other != id);
                    } else {
                        let ahead = queued.unwrap_or(0);
                        widgets::note(
                            ui,
                            if ahead == 0 {
                                fl!("next-to-convert")
                            } else {
                                fl!("waiting-for", ahead = ahead)
                            },
                        );
                    }
                }
                None => {
                    let current = item.result_is_current();
                    let text =
                        if item.made.is_some() { fl!("convert-again") } else { fl!("convert") };
                    let button = egui::Button::new(RichText::new(text).strong().size(15.0))
                        .min_size(vec2(140.0, 32.0))
                        .shortcut_text(fl!("convert-keys"));
                    let hint =
                        if current { fl!("convert-again-hint") } else { fl!("convert-hint") };
                    if ui.add(button).on_hover_text(hint).clicked() {
                        convert = true;
                    }
                    if presets_kept {
                        // an icon alone where the file's name and folder need the room
                        let text = fl!("save-preset");
                        let room = ui.available_width() - SAVES_TO;
                        let save = if room >= widgets::text_width(ui, &text, 16.0) {
                            egui::Button::new(text)
                        } else {
                            egui::Button::new(RichText::new("💾").size(15.0))
                        };
                        let hint = fl!("save-preset", "hint");
                        let save = save.min_size(vec2(32.0, 32.0));
                        if ui.add(save).on_hover_text(hint).clicked() {
                            crate::preset_dialogs::ask(
                                ui.ctx(),
                                crate::preset_dialogs::Ask::Save(id),
                            );
                        }
                    }
                    if let Some(failed) = &item.failed {
                        match &failed.job.status {
                            Status::Failed(message) => {
                                let failed = fl!("failed", error = message.as_str());
                                ui.colored_label(widgets::BAD, failed);
                            }
                            Status::Exists(path) => {
                                ui.colored_label(widgets::WARN, fl!("in-the-way"));
                                if ui.button(fl!("replace-it")).clicked() {
                                    replace = Some(path.clone());
                                }
                            }
                            Status::Cancelled => widgets::note(ui, fl!("stopped")),
                            Status::Done(_) | Status::Waiting | Status::Running => {}
                        }
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let Some(output) = &output else { return };
                let name = output
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let folder = output.parent().map(|p| p.display().to_string()).unwrap_or_default();
                let fallbacks = &self.fallbacks;
                let strong = text_format(ui, ui.visuals().strong_text_color());
                let (name, missing) = drawable(ui, fallbacks, &name, strong);
                // the button's short name where the long one leaves the
                // name no room, as in narrow windows or long words
                let named = ui.painter().layout_job(name.clone()).size().x;
                let full = fl!("show-in-folder");
                let padding = 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x;
                let fits = widgets::text_width(ui, &full, padding) + named <= ui.available_width();
                let text = if fits { full } else { fl!("show-folder", "short") };
                if output.exists() && ui.button(text).clicked() {
                    system::reveal(output);
                }
                ui.label(name).on_hover_ui(|ui| {
                    let plain = text_format(ui, ui.visuals().text_color());
                    ui.label(drawable(ui, fallbacks, &output.display().to_string(), plain).0);
                    if let Some(note) = missing_note(&missing) {
                        widgets::note(ui, note);
                    }
                });
                let weak = text_format(ui, ui.visuals().weak_text_color());
                let folder = format!("{folder}{}", std::path::MAIN_SEPARATOR);
                let saves = fl!("saves-to", folder = folder);
                ui.add(egui::Label::new(drawable(ui, fallbacks, &saves, weak).0).truncate())
                    .on_hover_text(fl!("saves-to", "hint"));
            });
        });
        let shortcut = !self.dialog_open()
            && ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));
        // the user lets this item replace the file that is there now
        if let Some(path) = replace {
            if let Some(written) = Written::of(id, &path) {
                self.written.insert(path, written);
            }
            convert = true;
        }
        if convert || (shortcut && self.item(id).is_some_and(|item| item.job.is_none())) {
            self.enqueue(id);
        }
    }
}

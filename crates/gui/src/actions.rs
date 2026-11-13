//! The bar under the selected item: converting it, stopping or not
//! converting it after all and how far it has got, what went wrong last
//! time, and where its result goes.

use eframe::egui::{self, RichText, vec2};

use crate::app::App;
use crate::fonts::{drawable, missing_note, text_format};
use crate::jobs::Status;
use crate::l10n::fl;
use crate::outputs::Written;
use crate::{system, theme, widgets};

/// Room the right of the bar takes, at least: the result's name and the
/// button that shows its folder.
const SAVES_TO: f32 = 300.0;

impl App {
    pub(crate) fn actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let output = self.item(id).map(|item| self.output_for(item));
        // a file there that this item didn't write, which tgradish made or
        // the settings let any result replace
        let replaces = output.as_ref().filter(|output| output.exists() && !self.wrote(id, output));
        let replaces = replaces.map(|output| match self.made_by_tgradish(output) {
            true => fl!("replaces-earlier"),
            false => fl!("replaces-earlier", "any"),
        });
        let queued = self.queue.iter().position(|&other| other == id);
        let presets_kept = self.presets_dir.is_some();
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        let mut convert = false;
        let mut replace = None;
        // the room goes to the progress, and the column beside the preview
        // already names the file
        let converting = item.job.as_ref().is_some_and(|conversion| conversion.job.is_running());
        ui.horizontal(|ui| {
            match item.job.as_ref().map(|conversion| &conversion.job) {
                Some(job) if job.is_running() => {
                    // until the conversion gets to its next check
                    let stopping = job.is_stopping();
                    let text = if stopping { fl!("stopping") } else { fl!("stop") };
                    let stop = egui::Button::new(text).min_size(vec2(140.0, 32.0));
                    if ui.add_enabled(!stopping, stop).clicked() {
                        job.cancel();
                    }
                    let size = vec2(ui.available_width(), 32.0);
                    crate::result::step_bar(ui, job, size);
                }
                Some(job) => {
                    if ui
                        .add(egui::Button::new(fl!("cancel-waiting")).min_size(vec2(140.0, 32.0)))
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
                    let hint = if current {
                        fl!("convert-again", "hint")
                    } else {
                        fl!("convert-again", "changed")
                    };
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
                                ui.colored_label(theme::bad(ui.visuals()), failed);
                            }
                            Status::Exists(path) => {
                                ui.colored_label(theme::warn(ui.visuals()), fl!("in-the-way"));
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
                let Some(output) = output.as_ref().filter(|_| !converting) else { return };
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
                if let Some(replaces) = &replaces {
                    ui.colored_label(theme::warn(ui.visuals()), format!("⚠ {replaces}"));
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

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use tgradish_core::ffmpeg::FfmpegChoice;

    use super::*;
    use crate::app::App;
    use crate::testing::{convert, finish, harness, made_job, square, video_of};

    /// A window with `count` pixel art files, results going to `out`.
    fn with_art(dir: &std::path::Path, count: usize) -> Harness<'static, App> {
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.join("out"));
        let mut harness = harness(config);
        let arts = (0..count)
            .map(|n| {
                let art = dir.join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        harness.state_mut().add(arts);
        finish(&mut harness);
        harness
    }

    fn select(harness: &mut Harness<App>, index: usize) {
        let id = harness.state().items[index].id;
        harness.state_mut().select(id);
        harness.run_steps(2);
    }

    #[test]
    fn goes_on_past_a_conversion_that_cannot_start() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 3);
        harness.state_mut().items[1].choices.preset = Some("no such preset".into());
        harness.run_steps(2);
        harness.get_by_label_contains("Convert all").click();
        harness.run_steps(2);
        finish(&mut harness);
        let items = &harness.state().items;
        assert!(items[0].made.is_some() && items[2].made.is_some());
        let failed = items[1].failed.as_ref().map(|failed| &failed.job.status);
        assert!(matches!(failed, Some(Status::Failed(_))), "{failed:?}");
        select(&mut harness, 1);
        harness.get_by_label_contains("Failed:");
    }

    #[test]
    fn takes_a_file_out_of_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 2);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        ids.iter().for_each(|&id| harness.state_mut().enqueue(id));
        // the first as if converting, for as long as it takes, so the
        // second waits
        harness.state_mut().queue.pop_front();
        harness.state_mut().items[0].job.as_mut().unwrap().job.status = Status::Running;
        select(&mut harness, 1);
        assert!(harness.query_all_by_label("Next to convert").count() > 0);
        harness.get_by_label("Cancel").click();
        harness.run_steps(2);
        let app = harness.state();
        assert!(app.queue.is_empty() && app.items[1].job.is_none());
        assert!(app.items[1].made.is_none() && app.items[1].failed.is_none());
        assert_eq!(harness.query_all_by_label("Next to convert").count(), 0);
    }

    #[test]
    fn goes_on_once_a_file_converting_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 2);
        harness.get_by_label_contains("Convert all").click();
        harness.run_steps(2);
        let first = harness.state().items[0].id;
        assert!(harness.state().items[0].job.as_ref().is_some_and(|c| c.job.is_running()));
        harness.state_mut().remove(first);
        finish(&mut harness);
        let items = &harness.state().items;
        assert_eq!(items.len(), 1);
        assert!(made_job(&items[0]).output.ends_with("1.sticker.tgs"));
    }

    #[test]
    fn stops_a_conversion() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video_of(dir.path(), "1280x720", 8) else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        harness.get_by_label("Stop").click();
        finish(&mut harness);
        let item = &harness.state().items[0];
        let failed = item.failed.as_ref().map(|failed| &failed.job.status);
        assert!(matches!(failed, Some(Status::Cancelled)), "{failed:?}");
        assert!(item.made.is_none());
        harness.get_by_label("Stopped");
    }

    #[test]
    fn replaces_a_file_in_the_way_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 1);
        let id = harness.state().items[0].id;
        harness.state_mut().enqueue(id);
        // put there after the result's place was worked out
        let output = harness.state().items[0].job.as_ref().unwrap().job.output.clone();
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(&output, b"mine").unwrap();
        finish(&mut harness);
        let failed = harness.state().items[0].failed.as_ref().map(|failed| &failed.job.status);
        assert!(matches!(failed, Some(Status::Exists(path)) if *path == output), "{failed:?}");
        assert_eq!(std::fs::read(&output).unwrap(), b"mine");
        harness.get_by_label("Replace it").click();
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!(made_job(item).output, output);
        assert!(item.failed.is_none() && std::fs::read(&output).unwrap() != b"mine");
    }
}

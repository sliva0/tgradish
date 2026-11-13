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

/// The share of the bar its right, where the result goes, keeps beside
/// what went wrong, at the least where it needs that much.
const RIGHT: f32 = 0.45;
/// The share of the right the result's name takes at most, so the folder
/// it goes in keeps some room.
const NAME: f32 = 0.6;

/// The least room the folder keeps beside a name drawn whole.
const SAVES_LEAST: f32 = 80.0;

/// What `add` adds, within `width` of the row, so text in it is cut short
/// there.
fn within<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.set_max_width(width.max(1.0));
        add(ui)
    })
    .inner
}

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
        // the bar's two halves each keep their own room: its right as much
        // as it needs, but where something is said on its left, no more
        // than its share
        let fallbacks = &self.fallbacks;
        let right = output
            .as_ref()
            .filter(|_| !converting)
            .map(|output| Right::of(ui, fallbacks, output, replaces.as_deref()));
        let gap = ui.spacing().item_spacing.x;
        let bar = ui.available_width();
        let keep = right.as_ref().map_or(0.0, |right| right.width.min(bar * RIGHT) + gap);
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
                    let button = egui::Button::new(widgets::medium(ui, text, 15.0))
                        .min_size(vec2(140.0, 32.0))
                        .shortcut_text(fl!("convert-keys"));
                    let hint = if current {
                        fl!("convert-again", "hint")
                    } else {
                        fl!("convert-again", "changed")
                    };
                    if widgets::primary(ui, true, button).on_hover_text(hint).clicked() {
                        convert = true;
                    }
                    if presets_kept {
                        // an icon alone where the file's name and folder need the room
                        let text = fl!("save-preset");
                        let room = ui.available_width() - keep;
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
                                // cut short, said in full on hover and
                                // beside the preview
                                let failed = fl!("failed", error = message.as_str());
                                let text = RichText::new(&failed).color(theme::bad(ui.visuals()));
                                let room = ui.available_width() - keep;
                                within(ui, room, |ui| ui.add(egui::Label::new(text).truncate()))
                                    .on_hover_text(failed);
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
            if let Some(right) = right {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    right.show(ui, fallbacks);
                });
            }
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

/// The right of the bar: where the result goes, its name, and the button
/// that shows its folder, laid out as they would be drawn whole.
struct Right<'a> {
    output: &'a std::path::Path,
    name: egui::text::LayoutJob,
    /// Characters of the name no font has.
    missing: Vec<char>,
    saves: egui::text::LayoutJob,
    replaces: Option<String>,
    /// How wide it all is, drawn whole.
    width: f32,
}

impl<'a> Right<'a> {
    fn of(
        ui: &egui::Ui,
        fallbacks: &crate::fallback::Fallbacks,
        output: &'a std::path::Path,
        replaces: Option<&str>,
    ) -> Right<'a> {
        let name = output.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let folder = output.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let strong = text_format(ui, ui.visuals().strong_text_color());
        let (name, missing) = drawable(ui, fallbacks, &name, strong);
        let weak = text_format(ui, ui.visuals().weak_text_color());
        let folder = format!("{folder}{}", std::path::MAIN_SEPARATOR);
        let saves = drawable(ui, fallbacks, &fl!("saves-to", folder = folder), weak).0;
        let replaces = replaces.map(|replaces| format!("⚠ {replaces}"));
        let gap = ui.spacing().item_spacing.x;
        let measure = |job: &egui::text::LayoutJob| ui.painter().layout_job(job.clone()).size().x;
        let padding = 2.0 * ui.spacing().button_padding.x;
        let mut width = measure(&name) + gap + measure(&saves);
        if output.exists() {
            width += gap + widgets::text_width(ui, &fl!("show-in-folder"), padding);
        }
        if let Some(replaces) = &replaces {
            width += gap + widgets::text_width(ui, replaces, 0.0);
        }
        Right { output, name, missing, saves, replaces, width }
    }

    /// Draws it, from the right, its words cut short where it has less
    /// room than it would take.
    fn show(self, ui: &mut egui::Ui, fallbacks: &crate::fallback::Fallbacks) {
        let Right { output, name, missing, saves, replaces, .. } = self;
        // the button's short name where the long one leaves the name no
        // room, as in narrow windows or long words
        let named = ui.painter().layout_job(name.clone()).size().x;
        let full = fl!("show-in-folder");
        let padding = 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x;
        let fits = widgets::text_width(ui, &full, padding) + named <= ui.available_width();
        let text = if fits { full } else { fl!("show-folder", "short") };
        if output.exists() && ui.button(text).clicked() {
            system::reveal(output);
        }
        if let Some(replaces) = replaces {
            let room = ui.available_width() / 2.0;
            let warning = RichText::new(&replaces).color(theme::warn(ui.visuals()));
            within(ui, room, |ui| ui.add(egui::Label::new(warning).truncate()))
                .on_hover_text(replaces);
        }
        // whole where the folder keeps a little room, else its share
        let room = ui.available_width();
        let room = named.min((room * NAME).max(room - SAVES_LEAST));
        within(ui, room, |ui| ui.add(egui::Label::new(name).truncate())).on_hover_ui(|ui| {
            let plain = text_format(ui, ui.visuals().text_color());
            ui.label(drawable(ui, fallbacks, &output.display().to_string(), plain).0);
            if let Some(note) = missing_note(&missing) {
                widgets::note(ui, note);
            }
        });
        ui.add(egui::Label::new(saves).truncate()).on_hover_text(fl!("saves-to", "hint"));
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

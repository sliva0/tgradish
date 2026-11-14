//! The bar under the selected item: converting it, stopping or not
//! converting it after all and how far it has got, what went wrong last
//! time, and where its result goes. Where that doesn't fit on a line,
//! where the result goes is on a second, under the rest.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use eframe::egui::{self, RichText, vec2};

use crate::app::App;
use crate::fonts::{drawable, missing_note, text_format};
use crate::jobs::Status;
use crate::l10n::fl;
use crate::outputs::Written;
use crate::{system, theme, widgets};

/// What `add` adds, within `width` of the row, so text in it is cut short
/// there.
fn within<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.set_max_width(width.max(1.0));
        add(ui)
    })
    .inner
}

/// Space over the bar's row, under it, and how high it is, as its first
/// button is: the list of files lays its own bar out level with it.
pub(crate) const BAR_ABOVE: f32 = 8.0;
pub(crate) const BAR_BELOW: f32 = 6.0;
pub(crate) const BAR_ROW: f32 = 32.0;

/// Whether the bar is on two lines, as it last needed, kept under this
/// name.
const LINES: &str = "action bar lines";
/// Asks the column beside the preview to show why converting failed, kept
/// under this name.
pub(crate) const POINT_AT_FAILURE: &str = "point at failure";

impl App {
    pub(crate) fn actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let output = self.item(id).map(|item| self.output_for(item));
        // a file there that this item didn't write, which tgradish made or
        // the settings let any result replace
        let there = output.as_ref().is_some_and(|output| self.probes.exists(output));
        let replaces = output.as_ref().filter(|output| there && !self.wrote(id, output));
        let replaces = replaces.map(|output| match self.made_by_tgradish(output) {
            true => fl!("replaces-earlier"),
            false => fl!("replaces-earlier", "any"),
        });
        let queued = self.queue.iter().position(|&other| other == id);
        let presets_kept = self.presets_dir.is_some();
        let probes = &self.probes;
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        // the result made, there still, which can be copied as a file, and
        // whether it is where the next goes, made as it would be made now
        let made = item.made.as_ref().and_then(|made| match &made.job.status {
            Status::Done(done) => Some(done.output.clone()).filter(|output| probes.exists(output)),
            _ => None,
        });
        // the one the bar names, not one of another format made before
        let made = made.filter(|made| Some(made) == output.as_ref());
        let saved = item.result_is_current() && made.is_some();
        let mut convert = false;
        let mut replace = None;
        let mut copy = None;
        // the room goes to the progress, and the column beside the preview
        // already names the file
        let converting = item.job.as_ref().is_some_and(|conversion| conversion.job.is_running());
        let fallbacks = &self.fallbacks;
        let right = output.as_ref().filter(|_| !converting).map(|output| {
            Right::of(ui, fallbacks, (output, there, made.clone(), saved), replaces.as_deref())
        });
        let gap = ui.spacing().item_spacing.x;
        let bar = ui.available_width();
        // where the result goes on a line of its own, under the rest, while
        // they don't fit on one: as found last time it was drawn
        let lines = egui::Id::new(LINES);
        let two = right.is_some() && ui.data(|data| data.get_temp(lines)).unwrap_or(false);
        // the right's least, kept beside the left on one line
        let keep = match &right {
            Some(right) if !two => right.least + gap,
            _ => 0.0,
        };
        // how wide the left is, all of it drawn whole
        let mut left = 0.0;
        ui.horizontal(|ui| {
            let start = ui.cursor().min.x;
            match item.job.as_ref().map(|conversion| &conversion.job) {
                Some(job) if job.is_running() => {
                    // until the conversion gets to its next check
                    let stopping = job.is_stopping();
                    let text = if stopping { fl!("stopping") } else { fl!("stop") };
                    let stop = egui::Button::new(text).min_size(vec2(140.0, BAR_ROW));
                    if ui.add_enabled(!stopping, stop).clicked() {
                        job.cancel();
                    }
                    let size = vec2(ui.available_width(), BAR_ROW);
                    crate::result::step_bar(ui, job, size);
                }
                Some(job) => {
                    if ui
                        .add(
                            egui::Button::new(fl!("cancel-waiting")).min_size(vec2(140.0, BAR_ROW)),
                        )
                        .clicked()
                    {
                        job.cancel();
                        item.job = None;
                        self.queue.retain(|&other| other != id);
                    } else {
                        let ahead = queued.unwrap_or(0);
                        let text = if ahead == 0 {
                            fl!("next-to-convert")
                        } else {
                            fl!("waiting-for", ahead = ahead)
                        };
                        left = ui.cursor().min.x - start + widgets::text_width(ui, &text, 0.0);
                        widgets::note(ui, text);
                    }
                }
                None => {
                    let current = item.result_is_current();
                    // again only where there is a result of this format
                    let again = item.made.as_ref().is_some_and(|made| made.format == item.format);
                    let text = if again { fl!("convert-again") } else { fl!("convert") };
                    let button = egui::Button::new(widgets::medium(ui, text, 15.0))
                        .min_size(vec2(140.0, BAR_ROW))
                        .shortcut_text(fl!("convert-keys"));
                    let hint = if current {
                        fl!("convert-again", "hint")
                    } else {
                        fl!("convert-again", "changed")
                    };
                    // what keeps it from being converted, said on it and
                    // beside it, under what it is of too
                    let blocked = item
                        .blockers
                        .first()
                        .map(|blocker| fl!("blocked", reason = blocker.reason()));
                    let response = widgets::primary(ui, blocked.is_none(), button);
                    let response = match &blocked {
                        Some(blocked) => response.on_disabled_hover_text(blocked),
                        None => response.on_hover_text(hint),
                    };
                    if response.clicked() {
                        convert = true;
                    }
                    // what is said after the buttons, and how wide it is
                    let status = Said::of(item, blocked);
                    let said = status.width(ui);
                    if presets_kept {
                        // an icon alone where what follows needs the room
                        let text = fl!("save-preset");
                        let room = ui.available_width() - keep - said;
                        let save = if room >= widgets::text_width(ui, &text, 16.0) {
                            egui::Button::new(text)
                        } else {
                            egui::Button::new(RichText::new("💾").size(15.0))
                        };
                        let hint = fl!("save-preset", "hint");
                        let save = save.min_size(vec2(BAR_ROW, BAR_ROW));
                        let save = widgets::named(ui.add(save), &fl!("save-preset"));
                        if save.on_hover_text(hint).clicked() {
                            crate::preset_dialogs::ask(
                                ui.ctx(),
                                crate::preset_dialogs::Ask::Save(id),
                            );
                        }
                    }
                    left = ui.cursor().min.x - start + said;
                    let room = ui.available_width() - keep;
                    if let Some(path) = status.show(ui, room) {
                        replace = Some(path);
                    }
                }
            }
            if let Some(right) = right.as_ref().filter(|_| !two) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    copy = right.show(ui, fallbacks);
                });
            }
        });
        if let Some(right) = right.as_ref().filter(|_| two) {
            ui.add_space(4.0);
            let row = vec2(ui.available_width(), theme::CONTROL);
            let layout = egui::Layout::right_to_left(egui::Align::Center);
            ui.allocate_ui_with_layout(row, layout, |ui| copy = right.show(ui, fallbacks));
        }
        // on two lines from the next frame, or back on one, as they fit
        let needs_two = right.as_ref().is_some_and(|right| left + gap + right.least > bar + 0.5);
        if needs_two != two {
            ui.data_mut(|data| data.insert_temp(lines, needs_two));
            ui.ctx().request_repaint();
        }
        if let Some(path) = copy {
            let now = ui.input(|input| input.time);
            let copied = self.clipboard.copy_file(&path);
            ui.data_mut(|data| data.insert_temp(egui::Id::new(COPIED), (now, copied)));
        }
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

/// What the bar says after its buttons, of the item's last conversion or
/// what keeps it from being converted.
enum Said {
    Nothing,
    Blocked(String),
    /// Converting failed, as said beside the preview.
    Failed(String),
    InTheWay(PathBuf),
    Stopped,
}

impl Said {
    fn of(item: &crate::item::Item, blocked: Option<String>) -> Said {
        if let Some(blocked) = blocked {
            return Said::Blocked(blocked);
        }
        match item.failed.as_ref().map(|failed| &failed.job.status) {
            Some(Status::Failed(message)) => Said::Failed(message.clone()),
            Some(Status::Exists(path)) => Said::InTheWay(path.clone()),
            Some(Status::Cancelled) => Said::Stopped,
            _ => Said::Nothing,
        }
    }

    /// How wide it is, drawn whole, with the space before it.
    fn width(&self, ui: &egui::Ui) -> f32 {
        let gap = ui.spacing().item_spacing.x;
        let padding = 2.0 * ui.spacing().button_padding.x;
        gap + match self {
            Said::Nothing => -gap,
            Said::Blocked(blocked) => widgets::text_width(ui, &format!("⚠ {blocked}"), 0.0),
            Said::Failed(_) => widgets::text_width(ui, &fl!("failed", "short"), padding),
            Said::InTheWay(_) => {
                widgets::text_width(ui, &fl!("in-the-way"), 0.0)
                    + gap
                    + widgets::text_width(ui, &fl!("replace-it"), padding)
            }
            Said::Stopped => widgets::text_width(ui, &fl!("stopped"), 0.0),
        }
    }

    /// Draws it within `room`, cut short where it is longer, said in full on
    /// hover. Gives the file in the way to replace, once asked.
    fn show(self, ui: &mut egui::Ui, room: f32) -> Option<PathBuf> {
        match self {
            Said::Nothing => {}
            Said::Blocked(blocked) => {
                let text = RichText::new(format!("⚠ {blocked}")).color(theme::warn(ui.visuals()));
                within(ui, room, |ui| ui.add(egui::Label::new(text).truncate()))
                    .on_hover_text(blocked);
            }
            // in short: what went wrong is said in full beside the preview,
            // where this points
            Said::Failed(message) => {
                let text = RichText::new(fl!("failed", "short")).color(theme::bad(ui.visuals()));
                let button = egui::Button::new(text).frame(false);
                let response = within(ui, room, |ui| ui.add(button))
                    .on_hover_text(fl!("failed", error = message.as_str()));
                if response.clicked() {
                    ui.data_mut(|data| data.insert_temp(egui::Id::new(POINT_AT_FAILURE), true));
                }
            }
            Said::InTheWay(path) => {
                ui.colored_label(theme::warn(ui.visuals()), fl!("in-the-way"));
                if ui.button(fl!("replace-it")).clicked() {
                    return Some(path);
                }
            }
            Said::Stopped => widgets::note(ui, fl!("stopped")),
        }
        None
    }
}

/// The least room for where the result goes, which is left out with less.
const FOLDER_LEAST: f32 = 60.0;

/// What copying the result last did, and when, kept under this name.
const COPIED: &str = "result copied";
/// How long what copying did is said on its button.
const COPIED_SHOWN: f64 = 2.0;

/// The right of the bar: where the result goes, its name, and the buttons
/// that copy the result made and show its folder.
struct Right<'a> {
    output: &'a Path,
    /// Something is there, whose folder can be shown.
    there: bool,
    /// The result made, which can be copied.
    made: Option<PathBuf>,
    /// Made where it goes, as it would be made now.
    saved: bool,
    name: String,
    replaces: Option<String>,
    /// How wide it all is at the least: its folder as short as it gets.
    least: f32,
}

impl<'a> Right<'a> {
    fn of(
        ui: &egui::Ui,
        fallbacks: &crate::fallback::Fallbacks,
        (output, there, made, saved): (&'a Path, bool, Option<PathBuf>, bool),
        replaces: Option<&str>,
    ) -> Right<'a> {
        let name = output.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let replaces = replaces.map(|replaces| format!("⚠ {replaces}"));
        let mut right = Right { output, there, made, saved, name, replaces, least: 0.0 };
        let gap = ui.spacing().item_spacing.x;
        let shortest = folders(output).pop().unwrap_or_default();
        let mut least = right.buttons_width(ui)
            + gap
            + right.name_width(ui, fallbacks)
            + gap
            + right.saves_width(ui, fallbacks, &shortest);
        if let Some(replaces) = &right.replaces {
            least += gap + widgets::text_width(ui, replaces, 0.0);
        }
        right.least = least;
        right
    }

    fn name_width(&self, ui: &egui::Ui, fallbacks: &crate::fallback::Fallbacks) -> f32 {
        let strong = text_format(ui, ui.visuals().strong_text_color());
        let job = drawable(ui, fallbacks, &self.name, strong).0;
        ui.painter().layout_job(job).size().x
    }

    /// "Saves to" and `folder`, or "Saved to", as it is drawn.
    fn saves(
        &self,
        ui: &egui::Ui,
        fallbacks: &crate::fallback::Fallbacks,
        folder: &str,
    ) -> egui::text::LayoutJob {
        let text = match self.saved {
            true => fl!("result-saved-to", folder = folder),
            false => fl!("saves-to", folder = folder),
        };
        drawable(ui, fallbacks, &text, text_format(ui, ui.visuals().weak_text_color())).0
    }

    fn saves_width(
        &self,
        ui: &egui::Ui,
        fallbacks: &crate::fallback::Fallbacks,
        folder: &str,
    ) -> f32 {
        ui.painter().layout_job(self.saves(ui, fallbacks, folder)).size().x
    }

    /// The buttons' room, with their names as long as they get.
    fn buttons_width(&self, ui: &egui::Ui) -> f32 {
        let gap = ui.spacing().item_spacing.x;
        let mut width = 0.0;
        if self.there {
            width += widgets::text_width(ui, &fl!("show-in-folder"), button_padding(ui)) + gap;
        }
        if self.made.is_some() {
            width += copy_width(ui) + gap;
        }
        (width - gap).max(0.0)
    }

    /// Draws it, from the right: its folder shortened from its middle
    /// where it has less room than it would take, and its name only where
    /// it alone is wider than that, its ending kept. Gives the result to
    /// copy, once asked.
    fn show(&self, ui: &mut egui::Ui, fallbacks: &crate::fallback::Fallbacks) -> Option<PathBuf> {
        if self.there && ui.button(fl!("show-in-folder")).clicked() {
            system::reveal(self.output);
        }
        let mut copy = None;
        if let Some(made) = &self.made {
            // what copying did, said on the button a moment
            let now = ui.input(|input| input.time);
            let copied: Option<(f64, Result<(), String>)> =
                ui.data(|data| data.get_temp(egui::Id::new(COPIED)));
            let (text, hint) = match copied.filter(|(when, _)| now - when < COPIED_SHOWN) {
                Some((when, copied)) => {
                    ui.ctx().request_repaint_after_secs((COPIED_SHOWN - (now - when)) as f32);
                    match copied {
                        Ok(()) => (copied_text(), fl!("copy-result", "hint")),
                        Err(why) => (fl!("copy-result", "failed"), why),
                    }
                }
                None => (fl!("copy-result"), fl!("copy-result", "hint")),
            };
            // as wide whatever it says, so nothing beside it moves
            let button = egui::Button::new(text).min_size(vec2(copy_width(ui), 0.0));
            if ui.add(button).on_hover_text(hint).clicked() {
                copy = Some(made.clone());
            }
        }
        if let Some(replaces) = &self.replaces {
            let warning = RichText::new(replaces).color(theme::warn(ui.visuals()));
            ui.add(egui::Label::new(warning).wrap_mode(egui::TextWrapMode::Extend));
        }
        let gap = ui.spacing().item_spacing.x;
        let room = ui.available_width();
        // the name whole, unless it alone is wider than all the room there
        // is, then its ending kept; the folder as long as the rest leaves
        // room for
        let strong = text_format(ui, ui.visuals().strong_text_color());
        let name = if self.name_width(ui, fallbacks) <= room {
            self.name.clone()
        } else {
            let fits = |text: &str| {
                let job = drawable(ui, fallbacks, text, strong.clone()).0;
                ui.painter().layout_job(job).size().x <= room
            };
            middle_cut(&self.name, ending(&self.name), fits)
        };
        let (job, missing) = drawable(ui, fallbacks, &name, strong);
        let named = ui.painter().layout_job(job.clone()).size().x;
        let output = self.output;
        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend)).on_hover_ui(|ui| {
            let plain = text_format(ui, ui.visuals().text_color());
            ui.label(drawable(ui, fallbacks, &output.display().to_string(), plain).0);
            if let Some(note) = missing_note(&missing) {
                widgets::note(ui, note);
            }
        });
        let room = room - named - gap;
        let folders = folders(self.output);
        let folder = folders
            .iter()
            .find(|folder| self.saves_width(ui, fallbacks, folder) <= room)
            .or(folders.last())
            .cloned()
            .unwrap_or_default();
        // none of it where hardly any would show
        if room < FOLDER_LEAST {
            return copy;
        }
        let saves = self.saves(ui, fallbacks, &folder);
        // shortened: said in full on hover
        let hint = match folders.first().filter(|whole| **whole != folder) {
            Some(whole) => format!("{whole}\n{}", fl!("saves-to", "hint")),
            None => fl!("saves-to", "hint"),
        };
        ui.add(egui::Label::new(saves).truncate()).on_hover_text(hint);
        copy
    }
}

/// The padding of a button's words, both sides.
fn button_padding(ui: &egui::Ui) -> f32 {
    2.0 * ui.spacing().button_padding.x
}

/// What Copy says once it copied.
fn copied_text() -> String {
    format!("✔ {}", fl!("copy-result", "done"))
}

/// How wide Copy is, whatever it says.
fn copy_width(ui: &egui::Ui) -> f32 {
    [fl!("copy-result"), copied_text(), fl!("copy-result", "failed")]
        .iter()
        .map(|text| widgets::text_width(ui, text, button_padding(ui)))
        .fold(0.0, f32::max)
}

/// The folder of `output` as the bar may write it, longest first: whole,
/// then ever less of its start, "…" in its place, down to its own name.
fn folders(output: &Path) -> Vec<String> {
    let folder = output.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let whole = format!("{folder}{MAIN_SEPARATOR}");
    let parts: Vec<&str> = folder.split(MAIN_SEPARATOR).filter(|part| !part.is_empty()).collect();
    let mut all = vec![whole];
    for kept in (1..parts.len()).rev() {
        let end = parts[parts.len() - kept..].join(std::path::MAIN_SEPARATOR_STR);
        all.push(format!("…{MAIN_SEPARATOR}{end}{MAIN_SEPARATOR}"));
    }
    all
}

/// How much of the end of a result's `name` is kept whole as it is cut
/// short: what it is, as `.sticker.webm`, after its name.
fn ending(name: &str) -> &str {
    let dots: Vec<usize> = name.match_indices('.').map(|(at, _)| at).collect();
    match dots[..] {
        [.., before, _] if before > 0 => &name[before..],
        [.., last] => &name[last..],
        [] => "",
    }
}

/// `text` cut short in the middle of what comes before `kept`, its end,
/// where "…" goes, as much of it as `fits`.
fn middle_cut(text: &str, kept: &str, fits: impl Fn(&str) -> bool) -> String {
    let body: Vec<char> = text[..text.len() - kept.len()].chars().collect();
    let cut = |count: usize| {
        let head = count.div_ceil(2);
        let start: String = body[..head].iter().collect();
        let end: String = body[body.len() - (count - head)..].iter().collect();
        format!("{start}…{end}{kept}")
    };
    // the most characters that still fit
    let (mut low, mut high) = (0, body.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        if fits(&cut(middle)) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    cut(low)
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::presets::Presets;

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
        // in short on the bar, which points at it in full beside the preview
        harness.get_by_label(&fl!("failed", "short")).click();
        harness.run_steps(2);
        assert!(harness.query_all_by_label_contains("no such preset").count() > 0);
    }

    #[test]
    fn puts_where_the_result_goes_under_the_rest_where_they_dont_fit() {
        let dir = tempfile::tempdir().unwrap();
        let long = "holiday at the seaside with the whole family, the long cut, final edit (2)";
        let art = dir.path().join(format!("{long}.png"));
        square(&art, [255, 0, 0, 255]);
        let deep = dir.path().join("out").join("for the family").join("Stickers");
        let mut config = Config::default();
        config.gui.output_dir = Some(deep.clone());
        let mut harness =
            Harness::builder().with_size([crate::MIN_WIDTH, 820.0]).build_eframe(move |cc| {
                let app = App::with(config.clone(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        let name = format!("{long}.sticker.tgs");
        let keys = fl!("convert-keys");
        let button = harness.get(egui_kittest::kittest::by().label_contains(&keys)).rect();
        // the name whole, on a line under Convert, its folder cut short
        let shown = harness.get_by_label(&name).rect();
        assert!(shown.top() > button.bottom(), "{shown:?} beside {button:?}");
        let short = fl!("saves-to", folder = format!("…{0}Stickers{0}", std::path::MAIN_SEPARATOR));
        harness.get_by_label(&short);
        // a wide window has room for all of it on one
        harness.set_size(egui::vec2(1920.0, 820.0));
        harness.run_steps(3);
        let button = harness.get(egui_kittest::kittest::by().label_contains(&keys)).rect();
        let shown = harness.get_by_label(&name).rect();
        assert!((shown.center().y - button.center().y).abs() < 2.0, "{shown:?} under {button:?}");
        let folder = format!("{}{}", deep.display(), std::path::MAIN_SEPARATOR);
        harness.get_by_label(&fl!("saves-to", folder = folder));
    }

    #[test]
    fn says_how_far_converting_all_got_and_stops_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 4);
        let all = |harness: &Harness<App>| harness.get_by_label_contains("Convert all").rect();
        let before = all(&harness);
        harness.get_by_label_contains("Convert all").click();
        harness.run_steps(1);
        assert_eq!(harness.state().batch_progress(), Some((1, 4)));
        harness.run_steps(1);
        harness.get_by_label(&fl!("converting-batch", current = 1, total = 4));
        // its button as wide and where it was along, though it counts fewer
        // now, under the line that says how far it got
        let after = all(&harness);
        assert_eq!((after.left(), after.width()), (before.left(), before.width()));
        harness.get_by_label(&fl!("stop-all")).click_accesskit();
        harness.run_steps(2);
        finish(&mut harness);
        let app = harness.state();
        assert!(app.queue.is_empty() && app.batch_progress().is_none());
        // the one at work stopped, the rest never begun
        let made = app.items.iter().filter(|item| item.made.is_some() || item.failed.is_some());
        assert!(made.count() <= 1);
        // and counted afresh next time
        let id = app.items[3].id;
        harness.state_mut().enqueue(id);
        assert_eq!(harness.state().batch_progress(), Some((1, 1)));
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
    fn says_where_a_result_is_saved_and_copies_it() {
        use tgradish_core::presets::Format;
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 1);
        let folder = format!("{}{}", dir.path().join("out").display(), std::path::MAIN_SEPARATOR);
        harness.get_by_label(&fl!("saves-to", folder = folder.as_str()));
        assert!(harness.query_by_label(&fl!("copy-result")).is_none());
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        harness.get_by_label(&fl!("result-saved-to", folder = folder.as_str()));
        harness.get_by_label(&fl!("copy-result")).click();
        harness.run_steps(2);
        let output = made_job(&harness.state().items[0]).output.clone();
        assert!(crate::system::COPIED.lock().unwrap().contains(&output));
        harness.get_by_label(&format!("✔ {}", fl!("copy-result", "done")));
        // another format: none made of it yet, to be saved, and made first
        harness.state_mut().items[0].format = Format::Webp;
        harness.run_steps(2);
        harness.get_by_label(&fl!("saves-to", folder = folder.as_str()));
        let keys = fl!("convert-keys");
        let button = harness.get(egui_kittest::kittest::by().label_contains(&keys));
        let label = button.accesskit_node().label().unwrap_or_default();
        assert!(
            label.starts_with(&fl!("convert")) && !label.contains(&fl!("convert-again")),
            "{label}"
        );
        // nor one of it to copy
        assert!(harness.query_by_label(&format!("✔ {}", fl!("copy-result", "done"))).is_none());
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

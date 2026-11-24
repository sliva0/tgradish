//! Action bar below the preview for the selected item: convert, stop, cancel
//! waiting, progress, copy result, last error, and output filename (middle-elided
//! when tight, or wrapped to a second line when it still does not fit).

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, vec2};

use crate::app::App;
use crate::fonts::{drawable, missing_note, text_format};
use crate::jobs::Status;
use crate::keys::{Command, with_keys};
use crate::l10n::fl;
use crate::outputs::Written;
use crate::{system, theme, widgets};

/// Runs `add` inside a scope capped at `width` so labels inside truncate there.
fn within<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.set_max_width(width.max(1.0));
        add(ui)
    })
    .inner
}

/// Vertical spacing above and below the action bar row, and row height (matching
/// its primary button). The file list aligns its footer to these values. Combined
/// with panel and window margins (and egui's trailing row spacing), [`BAR_BELOW`]
/// places the bar as far from the bottom of the window as the preview bar is from
/// the top.
pub(crate) const BAR_ABOVE: f32 = 8.0;
pub(crate) const BAR_BELOW: f32 = 9.0;

/// Extra space to add after the bar row to reach [`BAR_BELOW`] beyond egui's
/// default row spacing in `style`.
pub(crate) fn below_bar(style: &egui::Style) -> f32 {
    BAR_BELOW - style.spacing.item_spacing.y
}
pub(crate) const BAR_ROW: f32 = 32.0;

/// What the bar keeps from one frame to the next, whichever file it is
/// for: the clipboard results are copied to, what copying did last, and
/// how many lines the bar took.
#[derive(Default)]
pub(crate) struct Bar {
    /// The clipboard results are copied to, as files.
    pub(crate) clipboard: system::Clipboard,
    /// What copying a result last did, and when, which its button says
    /// for [`COPIED_SHOWN`] seconds.
    copied: Option<(f64, Result<(), String>)>,
    /// Where the result goes is on a line of its own, under the rest, as
    /// they didn't fit on one when the bar was last drawn.
    two_lines: bool,
}

/// Where a click on what the bar says of a failure asks the column beside
/// the preview to scroll to the failure in full: written by the bar, taken
/// by the column as it is drawn later in the same frame, which is all the
/// key lives for.
fn failure_pointed_id() -> egui::Id {
    egui::Id::new("point at failure")
}

/// Asks the column beside the preview to scroll to the failure in full.
fn point_at_failure(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(failure_pointed_id(), true));
}

/// Whether [`point_at_failure`] asked for it since this was last asked.
pub(crate) fn pointed_at_failure(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(failure_pointed_id())).unwrap_or(false)
}

impl App {
    /// Returns the output file written by item `id` if it is still unchanged on
    /// disk at `output` (matching the current format's destination rather than an
    /// earlier conversion to another format) and can be copied to the clipboard.
    pub(crate) fn copyable(&self, id: u64, output: Option<&Path>) -> Option<PathBuf> {
        let item = self.item(id)?;
        match &item.made.as_ref()?.job.status {
            Status::Done(done) if Some(done.output.as_path()) == output => {
                Some(done.output.clone()).filter(|made| self.outputs.wrote(id, made))
            }
            _ => None,
        }
    }

    pub(crate) fn actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let output = self.item(id).map(|item| self.output_for(item));
        // a file there that this item didn't write, which tgradish made or
        // the settings let any result replace
        let there = output.as_ref().is_some_and(|output| self.outputs.probes.exists(output));
        let replaces = output.as_ref().filter(|output| there && !self.outputs.wrote(id, output));
        let replaces = replaces.map(|output| match self.outputs.made_by_tgradish(output) {
            true => fl!("replaces-earlier"),
            false => fl!("replaces-earlier", "any"),
        });
        let queued = self.work.queue.iter().position(|&other| other == id);
        let presets_kept = self.presets_dir.is_some();
        let keyed = self.input.keyed;
        // the result to copy, and whether it is where the next goes, made
        // as it would be made now
        let made = self.copyable(id, output.as_deref());
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        let saved = item.result_is_current() && made.is_some();
        let mut convert = false;
        let mut replace = None;
        let mut copy = None;
        // the room goes to the progress, and the column beside the preview
        // already names the file
        let converting = item.job.as_ref().is_some_and(|conversion| conversion.job.is_running());
        let fallbacks = &self.fonts.fallbacks;
        let right = output
            .as_ref()
            .filter(|_| !converting)
            .map(|output| Right::of(ui, fallbacks, (output, there, saved), replaces.as_deref()));
        // the result made, to copy, while nothing is converted in its place
        let made = made.filter(|_| !converting);
        let gap = ui.spacing().item_spacing.x;
        let bar = ui.available_width();
        // where the result goes on a line of its own, under the rest, while
        // they don't fit on one: as found last time it was drawn
        let two = right.is_some() && self.bar.two_lines;
        let copied = &self.bar.copied;
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
                    // there from the start, so nothing beside it moves
                    // once an encode fits
                    if job.settles {
                        keep_best(ui, job, stopping, keyed.keep_best);
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
                        self.work.queue.retain(|&other| other != id);
                    } else {
                        if let Some(made) = &made {
                            copy = copy_button(ui, made, keyed.copy, copied);
                        }
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
                        .shortcut_text(crate::keys::keys_text(crate::keys::Command::Convert));
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
                    // Copy after Save preset, so that nothing moves as it
                    // shows once there is a result
                    let copying = made.as_ref().map_or(0.0, |_| copy_width(ui) + gap);
                    if presets_kept {
                        // an icon alone where what follows needs the room
                        let text = fl!("save-preset");
                        let room = ui.available_width() - keep - copying - said;
                        let save = match widgets::fitting(ui, &[&text], room, 16.0) {
                            Some(_) => egui::Button::new(text),
                            None => egui::Button::new(RichText::new("💾").size(15.0)),
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
                    if let Some(made) = &made {
                        copy = copy_button(ui, made, keyed.copy, copied);
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
                    right.show(ui, fallbacks);
                });
            }
        });
        if let Some(right) = right.as_ref().filter(|_| two) {
            ui.add_space(4.0);
            let row = vec2(ui.available_width(), theme::CONTROL);
            let layout = egui::Layout::right_to_left(egui::Align::Center);
            ui.allocate_ui_with_layout(row, layout, |ui| {
                right.show(ui, fallbacks);
            });
        }
        // on two lines from the next frame, or back on one, as they fit
        let needs_two = right.as_ref().is_some_and(|right| left + gap + right.least > bar + 0.5);
        if needs_two != two {
            self.bar.two_lines = needs_two;
            ui.ctx().request_repaint();
        }
        if let Some(path) = copy {
            let now = ui.input(|input| input.time);
            let copied = self.bar.clipboard.copy_file(&path);
            self.bar.copied = Some((now, copied));
        }
        let shortcut = keyed.convert;
        // the user lets this item replace the file that is there now
        if let Some(path) = replace {
            if let Some(written) = Written::of(id, &path) {
                self.outputs.written.insert(path, written);
            }
            convert = true;
        }
        if convert || (shortcut && self.item(id).is_some_and(|item| item.job.is_none())) {
            self.enqueue(id);
        }
    }
}

/// "Keep best so far" button next to Stop during WebM fitting: stops the search
/// and keeps the best fitting encode once one exists. Fixed to the wider of its
/// two labels and the same height as Stop; also triggered by its shortcut when
/// `keyed` is true.
fn keep_best(ui: &mut egui::Ui, job: &crate::jobs::Job, stopping: bool, keyed: bool) {
    let (keep, keeping) = (fl!("keep-best"), fl!("keeping-best"));
    let width = [&keep, &keeping]
        .iter()
        .map(|text| widgets::text_width(ui, text, button_padding(ui)))
        .fold(0.0, f32::max);
    let settling = job.is_settling();
    let text = if settling { keeping } else { keep };
    let button = egui::Button::new(text).min_size(vec2(width, BAR_ROW));
    let enabled = job.can_settle() && !settling && !stopping;
    // disabled: why, unless it is only as it stops anyway
    let why = match settling || job.can_settle() {
        true => fl!("keep-best", "hint"),
        false => fl!("keep-best", "nothing-fits"),
    };
    let response = ui.add_enabled(enabled, button);
    let hint = with_keys(&fl!("keep-best", "hint"), Command::KeepBest);
    let response = response.on_hover_text(hint).on_disabled_hover_text(why);
    if response.clicked() || (enabled && keyed) {
        job.settle();
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
            Some(Status::Failed(message)) => Said::Failed(message.say()),
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
                    point_at_failure(ui.ctx());
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

/// Characters of a result's name kept, beside its ending, where it is
/// cut shortest; with less room, the bar goes on two lines instead.
const NAME_LEAST: usize = 8;

/// How long what copying did is said on its button.
const COPIED_SHOWN: f64 = 2.0;

/// The right of the bar: the name of the file the result goes to, and
/// the button that shows its folder, its path said on both on hover.
struct Right<'a> {
    output: &'a Path,
    /// Something is there, whose folder can be shown.
    there: bool,
    /// Made where it goes, as it would be made now.
    saved: bool,
    name: String,
    replaces: Option<String>,
    /// How wide the name is as short as it gets.
    shortest: f32,
    /// How wide it all is at the least: its name as short as it gets.
    least: f32,
}

impl<'a> Right<'a> {
    fn of(
        ui: &egui::Ui,
        fallbacks: &crate::fallback::Fallbacks,
        (output, there, saved): (&'a Path, bool, bool),
        replaces: Option<&str>,
    ) -> Right<'a> {
        let name = output.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let replaces = replaces.map(|replaces| format!("⚠ {replaces}"));
        let mut right = Right { output, there, saved, name, replaces, shortest: 0.0, least: 0.0 };
        let gap = ui.spacing().item_spacing.x;
        let kept = ending(&right.name);
        let cut = middle_cut(&right.name, kept, |text| {
            text.chars().count() <= NAME_LEAST + 1 + kept.chars().count()
        });
        right.shortest =
            right.name_width(ui, fallbacks, &cut).min(right.name_width(ui, fallbacks, &right.name));
        let mut least =
            right.buttons_width(ui) + gap + right.shortest + gap + right.saves_width(ui, fallbacks);
        if let Some(replaces) = &right.replaces {
            least += gap + widgets::text_width(ui, replaces, 0.0);
        }
        right.least = least;
        right
    }

    fn name_width(&self, ui: &egui::Ui, fallbacks: &crate::fallback::Fallbacks, name: &str) -> f32 {
        let strong = text_format(ui, ui.visuals().strong_text_color());
        let job = drawable(ui, fallbacks, name, strong).0;
        ui.painter().layout_job(job).size().x
    }

    /// "Saves as", or "Saved as", before the name, as it is drawn.
    fn saves(
        &self,
        ui: &egui::Ui,
        fallbacks: &crate::fallback::Fallbacks,
    ) -> egui::text::LayoutJob {
        let text = match self.saved {
            true => fl!("result-saved-as"),
            false => fl!("saves-as"),
        };
        drawable(ui, fallbacks, &text, text_format(ui, ui.visuals().weak_text_color())).0
    }

    fn saves_width(&self, ui: &egui::Ui, fallbacks: &crate::fallback::Fallbacks) -> f32 {
        ui.painter().layout_job(self.saves(ui, fallbacks)).size().x
    }

    /// The button's room.
    fn buttons_width(&self, ui: &egui::Ui) -> f32 {
        match self.there {
            true => widgets::text_width(ui, &fl!("show-in-folder"), button_padding(ui)),
            false => 0.0,
        }
    }

    /// Draws it, from the right: the name whole where there is room for
    /// it, else cut short in its middle, its ending kept, and what it is
    /// saved as before it where there is room for that too.
    fn show(&self, ui: &mut egui::Ui, fallbacks: &crate::fallback::Fallbacks) {
        let path = self.output.display().to_string();
        let show = self.there.then(|| ui.button(fl!("show-in-folder")).on_hover_text(&path));
        if show.is_some_and(|show| show.clicked()) {
            system::reveal(self.output);
        }
        if let Some(replaces) = &self.replaces {
            let warning = RichText::new(replaces).color(theme::warn(ui.visuals()));
            ui.add(egui::Label::new(warning).wrap_mode(egui::TextWrapMode::Extend));
        }
        let gap = ui.spacing().item_spacing.x;
        let saves_width = self.saves_width(ui, fallbacks) + gap;
        // what it is saved as only where the name has room beside it, as
        // short as it gets
        let said = ui.available_width() - saves_width >= self.shortest - 0.5;
        let room = ui.available_width() - if said { saves_width } else { 0.0 };
        let strong = text_format(ui, ui.visuals().strong_text_color());
        let name = if self.name_width(ui, fallbacks, &self.name) <= room {
            self.name.clone()
        } else {
            let fits = |text: &str| self.name_width(ui, fallbacks, text) <= room;
            middle_cut(&self.name, ending(&self.name), fits)
        };
        let (job, missing) = drawable(ui, fallbacks, &name, strong);
        let hint = format!("{path}\n{}", fl!("saves-as", "hint"));
        let label = egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend);
        ui.add(label).on_hover_ui(|ui| {
            let plain = text_format(ui, ui.visuals().text_color());
            ui.label(drawable(ui, fallbacks, &path, plain).0);
            widgets::note(ui, fl!("saves-as", "hint"));
            if let Some(note) = missing_note(&missing) {
                widgets::note(ui, note);
            }
        });
        if said {
            let saves = self.saves(ui, fallbacks);
            let label = egui::Label::new(saves).wrap_mode(egui::TextWrapMode::Extend);
            ui.add(label).on_hover_text(hint);
        }
    }
}

/// Copy button shown next to Convert once a result exists: copies `made` as a
/// file for pasting into Telegram and briefly shows the outcome. Fixed to the
/// widest of its labels so adjacent controls do not shift. Returns the path to
/// copy when clicked or triggered via shortcut (`keyed`); what copying last
/// did is `copied`, [`Bar`]'s.
fn copy_button(
    ui: &mut egui::Ui,
    made: &Path,
    keyed: bool,
    copied: &Option<(f64, Result<(), String>)>,
) -> Option<PathBuf> {
    let now = ui.input(|input| input.time);
    let (text, hint) = match copied.as_ref().filter(|(when, _)| now - when < COPIED_SHOWN) {
        Some((when, copied)) => {
            ui.ctx().request_repaint_after_secs((COPIED_SHOWN - (now - when)) as f32);
            match copied {
                Ok(()) => (copied_text(), copy_hint()),
                Err(why) => (fl!("copy-result", "failed"), why.clone()),
            }
        }
        None => (fl!("copy-result"), copy_hint()),
    };
    let button = egui::Button::new(text).min_size(vec2(copy_width(ui), BAR_ROW));
    let clicked = ui.add(button).on_hover_text(hint).clicked();
    (clicked || keyed).then(|| made.to_path_buf())
}

/// The padding of a button's words, both sides.
fn button_padding(ui: &egui::Ui) -> f32 {
    2.0 * ui.spacing().button_padding.x
}

/// What Copy says on hover, with its key.
fn copy_hint() -> String {
    with_keys(&fl!("copy-result", "hint"), Command::CopyResult)
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
    use crate::testing::{convert, convert_all, finish, harness, made_job, square, video_of};

    /// The GUI with `count` pixel art files, results going to `out`.
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
        convert_all(&harness).click();
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
    fn keeps_the_name_of_the_result_beside_the_rest_cut_short_only_without_room() {
        let dir = tempfile::tempdir().unwrap();
        let long = "holiday at the seaside with the whole family, the long cut, final edit (2)";
        let art = dir.path().join(format!("{long}.png"));
        square(&art, [255, 0, 0, 255]);
        // a folder as long wherever the temporary one is, never written to
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        let deep = std::path::Path::new(root).join("out").join("for the family").join("Stickers");
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
        let keys = crate::keys::keys_text(crate::keys::Command::Convert);
        let level = |harness: &Harness<App>, label: &str| {
            let button = harness.get(egui_kittest::kittest::by().label_contains(&keys)).rect();
            let shown = harness.get_by_label(label).rect();
            assert!((shown.center().y - button.center().y).abs() < 2.0, "{shown:?} by {button:?}");
            shown
        };
        // on the row of Convert, cut short in its middle, its ending kept,
        // and no folder said
        let cut = harness
            .query_all_by_role(egui::accesskit::Role::Label)
            .filter_map(|node| node.accesskit_node().value().map(|value| value.to_string()))
            .find(|label| label.contains("…") && label.contains(".sticker"))
            .expect("the name cut short");
        assert!(cut.ends_with(".sticker.tgs") && cut.len() < name.len(), "{cut}");
        level(&harness, &cut);
        level(&harness, &fl!("saves-as"));
        assert!(harness.query_by_label_contains("Stickers").is_none());
        // a wide window has room for it whole
        harness.set_size(egui::vec2(1920.0, 820.0));
        harness.run_steps(3);
        let shown = level(&harness, &name);
        let saves = level(&harness, &fl!("saves-as"));
        assert!(saves.right() < shown.left(), "{saves:?} before {shown:?}");
    }

    #[test]
    fn says_how_far_converting_all_got_and_stops_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 4);
        let all = |harness: &Harness<App>| convert_all(harness).rect();
        let before = all(&harness);
        // all four asked for, the first as if converting, for as long as it
        // takes, so the rest wait
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        ids.iter().for_each(|&id| harness.state_mut().enqueue(id));
        harness.state_mut().work.queue.pop_front();
        harness.state_mut().items[0].job.as_mut().unwrap().job.status = Status::Running;
        assert_eq!(harness.state().batch_progress(), Some((1, 4)));
        harness.run_steps(2);
        harness.get_by_label(&fl!("converting-batch", current = 1, total = 4));
        // its button as wide and where it was along, though it counts fewer
        // now, under the line that says how far it got
        let after = all(&harness);
        assert_eq!((after.left(), after.width()), (before.left(), before.width()));
        harness.get_by_label(&fl!("stop-all")).click_accesskit();
        harness.run_steps(2);
        let app = harness.state();
        assert!(app.work.queue.is_empty());
        // the one at work asked to stop, the rest never begun
        assert!(app.items[0].job.as_ref().is_some_and(|conversion| conversion.job.is_stopping()));
        for item in &app.items[1..] {
            assert!(item.job.is_none() && item.made.is_none() && item.failed.is_none());
        }
        // and, once it has stopped, counted afresh next time
        harness.state_mut().items[0].job = None;
        harness.state_mut().enqueue(ids[3]);
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
        harness.state_mut().work.queue.pop_front();
        harness.state_mut().items[0].job.as_mut().unwrap().job.status = Status::Running;
        select(&mut harness, 1);
        assert!(harness.query_all_by_label(&fl!("next-to-convert")).count() > 0);
        harness.get_by_label(&fl!("cancel-waiting")).click();
        harness.run_steps(2);
        let app = harness.state();
        assert!(app.work.queue.is_empty() && app.items[1].job.is_none());
        assert!(app.items[1].made.is_none() && app.items[1].failed.is_none());
        assert_eq!(harness.query_all_by_label(&fl!("next-to-convert")).count(), 0);
    }

    #[test]
    fn goes_on_once_a_file_converting_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 2);
        convert_all(&harness).click();
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
        harness.get_by_label(&fl!("stop")).click();
        finish(&mut harness);
        let item = &harness.state().items[0];
        let failed = item.failed.as_ref().map(|failed| &failed.job.status);
        assert!(matches!(failed, Some(Status::Cancelled)), "{failed:?}");
        assert!(item.made.is_none());
        harness.get_by_label(&fl!("stopped"));
    }

    #[test]
    fn keeps_the_best_encode_so_far() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video_of(dir.path(), "1280x720", 8) else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        // fitting from far under the limit, so it goes on once one fits
        let webm = &mut harness.state_mut().items[0].choices.webm;
        webm.fit = Some(tgradish_core::options::Fit::Bitrate);
        webm.bitrate = Some(20.0);
        convert(&harness);
        harness.run_steps(2);
        let keep = fl!("keep-best");
        let button = |harness: &Harness<App>| {
            let node = harness.get_by_label(&keep);
            (node.rect(), node.accesskit_node().is_disabled())
        };
        let stop = harness.get_by_label(&fl!("stop")).rect();
        // there before anything fits, beside Stop and as high
        let (before, disabled) = button(&harness);
        assert!(disabled);
        assert_eq!((before.top(), before.bottom()), (stop.top(), stop.bottom()));
        assert!(before.left() > stop.right());
        let fits = |harness: &Harness<App>| {
            harness.state().items[0].job.as_ref().is_some_and(|c| c.job.can_settle())
        };
        for _ in 0..3000 {
            if fits(&harness) {
                break;
            }
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(fits(&harness), "nothing fit");
        harness.run_steps(1);
        // where it was, and enabled
        let (after, disabled) = button(&harness);
        assert!(!disabled);
        assert_eq!(after, before);
        // by its key, which presses it as a click does
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
        harness.run_steps(1);
        finish(&mut harness);
        let item = &harness.state().items[0];
        let Status::Done(done) = &made_job(item).status else { panic!("not made") };
        assert!(done.settled, "{done:?}");
        // the first, far under the limit, which fitting would have gone on
        // from: said with the result
        let kept = done.attempt.unwrap();
        assert_eq!(kept, 1);
        harness.get_by_label(&fl!("decided-settled", number = kept));
    }

    #[test]
    fn says_where_a_result_is_saved_and_copies_it() {
        use tgradish_core::presets::Format;
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 1);
        harness.get_by_label(&fl!("saves-as"));
        harness.get_by_label("0.sticker.tgs");
        assert!(harness.query_by_label(&fl!("copy-result")).is_none());
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        harness.get_by_label(&fl!("result-saved-as"));
        // beside Convert again, as high, and pointed at by the result
        let keys = crate::keys::keys_text(crate::keys::Command::Convert);
        let again = harness.get(egui_kittest::kittest::by().label_contains(&keys)).rect();
        let copy = harness.get_by_label(&fl!("copy-result")).rect();
        assert_eq!((copy.top(), copy.bottom()), (again.top(), again.bottom()));
        assert!(copy.left() > again.right(), "{copy:?} beside {again:?}");
        let copy_keys = crate::keys::keys_text(crate::keys::Command::CopyResult);
        harness.get_by_label(&fl!("result-copy", keys = copy_keys));
        harness.get_by_label(&fl!("copy-result")).click();
        harness.run_steps(2);
        let output = made_job(&harness.state().items[0]).output.clone();
        assert!(crate::system::COPIED.lock().unwrap().contains(&output));
        harness.get_by_label(&format!("✔ {}", fl!("copy-result", "done")));
        // another format: none made of it yet, to be saved, and made first
        harness.state_mut().items[0].format = Format::Webp;
        harness.run_steps(2);
        harness.get_by_label(&fl!("saves-as"));
        let keys = crate::keys::keys_text(crate::keys::Command::Convert);
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
    fn copies_a_result_by_its_key_unless_text_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 1);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let output = made_job(&harness.state().items[0]).output.clone();
        let copies =
            || crate::system::COPIED.lock().unwrap().iter().filter(|p| **p == output).count();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
        harness.run_steps(2);
        assert_eq!(copies(), 1);
        harness.get_by_label(&copied_text());
        // as egui-winit passes Ctrl+C on
        harness.event(egui::Event::Copy);
        harness.run_steps(2);
        assert_eq!(copies(), 2);
        // a field's text, while it has the focus
        let field = egui::accesskit::Role::SpinButton;
        harness.query_all_by_role(field).next().expect("a number field").focus();
        harness.run_steps(2);
        harness.event(egui::Event::Copy);
        harness.run_steps(2);
        assert_eq!(copies(), 2);
    }

    #[test]
    fn takes_a_file_put_in_place_of_a_result_for_another() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = with_art(dir.path(), 2);
        let saved = fl!("result-saved-as");
        convert_all(&harness).click();
        harness.run_steps(2);
        finish(&mut harness);
        let id = harness.state().items[0].id;
        harness.state_mut().select(id);
        harness.run_steps(2);
        harness.get_by_label(&saved);
        // another result of tgradish's in its place, as another tgradish
        // running may put there, as like it as can be but for when it was
        // written
        let output = made_job(&harness.state().items[0]).output.clone();
        let other = made_job(&harness.state().items[1]).output.clone();
        std::fs::copy(&other, &output).unwrap();
        let file = std::fs::File::options().write(true).open(&output).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1 << 30)).unwrap();
        drop(file);
        harness.state().outputs.probes.forget();
        harness.run_steps(2);
        // not this item's result, to copy or say it is saved: one it replaces
        assert!(harness.query_by_label(&saved).is_none());
        assert!(harness.query_by_label(&fl!("copy-result")).is_none());
        harness.get_by_label_contains(&fl!("replaces-earlier"));
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
        harness.get_by_label(&fl!("replace-it")).click();
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!(made_job(item).output, output);
        assert!(item.failed.is_none() && std::fs::read(&output).unwrap() != b"mine");
    }
}

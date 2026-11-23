//! User preset dialogs: saving the selected item's settings as a preset, and
//! renaming, describing, or deleting user presets. Triggered from the settings
//! panel or action bar and shown as a modal over the window.
//!
//! When an item is saved as a preset, it adopts that preset: its format
//! overrides (merged with the preset it started from) become the new preset's
//! values, except file-specific fields (title and sprite-sheet layout), which
//! remain on the item. Crop, trim, and Aseprite tag/layer choices live outside
//! the preset in [`crate::item::Choices`] and are unchanged.

use std::path::Path;

use eframe::egui::{self, RichText, vec2};
use tgradish_core::options::{Fit, Range};
use tgradish_core::presets::{self, Presets};
use tgradish_core::tgs;
use tgradish_core::webm::Options;

use crate::app::App;
use crate::banner::{Action, Message, Subject};
use crate::item::Item;
use crate::jobs::failure;
use crate::l10n::fl;
use crate::{theme, widgets};

/// Preset dialog request stored in egui temporary data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Ask {
    /// Save the settings of the item with this ID as a preset.
    Save(u64),
    Manage,
}

fn ask_id() -> egui::Id {
    egui::Id::new("preset dialog asked for")
}

/// Requests that a preset dialog be opened on the next frame.
pub(crate) fn ask(ctx: &egui::Context, ask: Ask) {
    ctx.data_mut(|data| data.insert_temp(ask_id(), ask));
}

/// Active preset dialog state.
pub(crate) enum Dialog {
    Save(Saving),
    Manage(Managing),
}

pub(crate) struct Saving {
    item: u64,
    name: String,
    description: String,
    problem: Option<String>,
    /// Whether the name input has already received initial focus on open.
    focused: bool,
}

/// State of the Manage Presets dialog.
pub(crate) struct Managing {
    rows: Vec<Row>,
    /// Presets deleted while the dialog has been open, restorable via Undo.
    deleted: Vec<Deleted>,
}

#[derive(Debug, Clone)]
struct Row {
    /// Stable row ID across additions and deletions.
    id: u64,
    /// Current preset name on disk.
    name: String,
    description: String,
    /// Uncommitted text in the name and description fields (applied when the
    /// field loses focus).
    typed_name: String,
    typed_description: String,
    /// Parse or read error if the preset file is invalid.
    broken: Option<String>,
    /// Error from the last rename, description, or delete attempt.
    problem: Option<String>,
}

impl Row {
    fn of(id: u64, name: &str, preset: Result<&presets::Preset, String>) -> Row {
        let description =
            preset.as_ref().map(|preset| preset.description.clone()).unwrap_or_default();
        Row {
            id,
            name: name.to_owned(),
            typed_name: name.to_owned(),
            typed_description: description.clone(),
            description,
            broken: preset.err(),
            problem: None,
        }
    }
}

struct Deleted {
    /// Original row ID restored on Undo.
    id: u64,
    name: String,
    /// Raw TOML file contents before deletion.
    text: String,
    /// IDs of items that used this preset, re-associated on Undo.
    items: Vec<u64>,
    /// Whether `config` used this preset as the default for WebM/pictures and
    /// `.tgs`.
    defaults: (bool, bool),
    /// Per-item `fit_range` overrides dropped when falling back to the default
    /// preset changed the fitting unit, paired with the original [`Fit`] mode.
    ranges: Vec<(u64, Range, Fit)>,
}

impl Managing {
    fn of(presets: &Presets) -> Managing {
        // Built-in presets have no file on disk and are never broken.
        let rows = presets
            .iter()
            .filter(|(_, preset)| preset.is_err() || preset.is_ok_and(is_users))
            .enumerate()
            .map(|(id, (name, preset))| Row::of(id as u64, name, preset.map_err(|e| e.to_string())))
            .collect();
        Managing { rows, deleted: Vec::new() }
    }
}

/// Returns true if `preset` is a user preset on disk rather than built-in.
pub(crate) fn is_users(preset: &presets::Preset) -> bool {
    preset.path.is_some()
}

/// Action queued while drawing the Manage Presets dialog, keyed by stable row
/// ID so earlier changes shifting row indices do not misdirect later ones.
enum Change {
    Rename(u64),
    Describe(u64),
    Delete(u64),
    Undo(u64),
}

/// Formats a preset name/rename/delete error for the GUI.
fn problem(err: &tgradish_core::Error) -> String {
    use tgradish_core::Error;
    use tgradish_core::presets::PresetError;
    match err {
        Error::Preset(PresetError::NameLength) => {
            fl!("preset-name-length", most = presets::LONGEST_NAME)
        }
        Error::Preset(PresetError::NameCharacters) => fl!("preset-name-characters"),
        Error::Preset(PresetError::NameTaken(name)) => {
            fl!("preset-name-taken", name = name.as_str())
        }
        Error::Preset(PresetError::Extended(other)) => {
            fl!("preset-extended", other = other.as_str())
        }
        err => failure(err),
    }
}

/// Validates a preset `name` via `tgradish-core` and returns any localised
/// error message.
fn name_problem(name: &str) -> Option<String> {
    presets::check_name(name).err().map(|err| problem(&err))
}

impl App {
    /// Opens any requested preset dialog and draws the currently open one.
    pub(crate) fn preset_dialogs(&mut self, ctx: &egui::Context) {
        let asked = ctx.data_mut(|data| {
            let asked = data.get_temp::<Ask>(ask_id());
            data.remove::<Ask>(ask_id());
            asked
        });
        if let Some(asked) = asked {
            self.preset_dialog = Some(match asked {
                Ask::Save(item) => Dialog::Save(Saving {
                    item,
                    name: String::new(),
                    description: String::new(),
                    problem: None,
                    focused: false,
                }),
                Ask::Manage => Dialog::Manage(Managing::of(&self.presets)),
            });
        }
        let Some(dir) = self.presets_dir.clone() else {
            self.preset_dialog = None;
            return;
        };
        self.preset_dialog = match self.preset_dialog.take() {
            Some(Dialog::Save(saving)) => self.save_dialog(ctx, &dir, saving).map(Dialog::Save),
            Some(Dialog::Manage(managing)) => {
                self.manage_dialog(ctx, &dir, managing).map(Dialog::Manage)
            }
            None => None,
        };
    }

    /// Reloads presets from disk and returns true on success.
    pub(crate) fn reload_presets(&mut self) -> bool {
        match Presets::load(self.presets_dir.as_deref()) {
            Ok(presets) => {
                self.presets = presets;
                self.banner.forget(Subject::Presets);
                true
            }
            Err(err) => {
                let message = Message::warning(fl!("presets-unreadable"));
                let message = match (&self.presets_dir, crate::app::told(message, &err)) {
                    (Some(dir), message) if message.file.is_none() => message.file(dir),
                    (_, message) => message,
                };
                let actions = [Action::ReadPresetsAgain, Action::CopyPath, Action::ShowInFolder];
                self.say(message.actions(&actions).about(Subject::Presets));
                false
            }
        }
    }

    /// Draws the Save Preset dialog; returns `None` once saved or closed.
    fn save_dialog(
        &mut self,
        ctx: &egui::Context,
        dir: &Path,
        mut saving: Saving,
    ) -> Option<Saving> {
        // Close if the target item was removed.
        self.item(saving.item)?;
        let name = saving.name.trim_end().to_owned();
        // Disallow shadowing a built-in preset name across all items.
        let builtin = self.presets.get(&name).is_ok_and(|preset| !is_users(preset));
        let valid = match name_problem(&name) {
            Some(problem) => Err(problem),
            None if builtin => Err(fl!("preset-name-builtin", name = name.as_str())),
            None => Ok(()),
        };
        let exists = valid.is_ok() && dir.join(format!("{name}.toml")).exists();
        let (mut close, mut save) = (false, false);
        let modal = widgets::dialog(ctx, "save preset").width(400.0).show(ctx, |ui| {
            close = widgets::dialog_title(ui, widgets::title_text(ui, fl!("save-preset-title")));
            widgets::note(ui, fl!("save-preset-note"));
            ui.add_space(8.0);
            egui::Grid::new("save preset fields").num_columns(2).spacing([10.0, 8.0]).show(
                ui,
                |ui| {
                    ui.label(fl!("preset-name"));
                    let field = widgets::text_field(&mut saving.name)
                        .hint_text(fl!("preset-name", "placeholder"))
                        .desired_width(270.0);
                    let field = ui.add(field);
                    if !saving.focused {
                        field.request_focus();
                        saving.focused = true;
                    }
                    // Pressing Enter in the name field submits the dialog.
                    if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        save = valid.is_ok();
                    }
                    ui.end_row();
                    ui.label(fl!("preset-description"));
                    let field = widgets::text_field(&mut saving.description)
                        .hint_text(fl!("preset-description", "placeholder"))
                        .desired_width(270.0);
                    ui.add(field);
                    ui.end_row();
                },
            );
            if exists {
                widgets::note(ui, fl!("preset-exists"));
            }
            if let Some(problem) = &saving.problem {
                ui.colored_label(theme::bad(ui.visuals()), problem);
            }
            widgets::dialog_buttons(ui, |ui| {
                close |= ui.add(widgets::dialog_button(fl!("cancel"))).clicked();
                let text = if exists { fl!("replace") } else { fl!("save") };
                let button = widgets::dialog_button(widgets::medium(ui, text, 14.0));
                let button = widgets::primary(ui, valid.is_ok(), button);
                let button = match &valid {
                    Err(err) if !name.is_empty() => button.on_disabled_hover_text(err),
                    _ => button.on_disabled_hover_text(fl!("name-it-first")),
                };
                save |= button.clicked();
            });
        });
        if close || modal.should_close() {
            return None;
        }
        if !save {
            return Some(saving);
        }
        let item = self.item(saving.item)?;
        let (presets, config) = (&self.presets, &self.config);
        let choices = &item.choices;
        // the item's layer over its preset, as converting takes them, so
        // the preset makes what the item made, from the command line too
        let webm = choices.webm_layer(presets, config);
        let webm = choices.webm_base(presets, config).unwrap_or_default().merged(&webm);
        let tgs =
            choices.tgs_base(presets, config).unwrap_or_default().merged(&choices.tgs_layer());
        let description = saving.description.trim().to_owned();
        match presets::save(dir, &name, &description, (&webm, &tgs), exists) {
            Ok(_) => {
                // Adopting the newly saved preset keeps effective settings identical,
                // so do not record an undo step.
                self.settle_edits();
                if let Some(item) = self.item_mut(saving.item) {
                    take_up(item, &name);
                }
                self.take_in_edits();
                self.reload_presets();
                None
            }
            Err(err) => {
                saving.problem = Some(failure(&err));
                Some(saving)
            }
        }
    }

    /// Draws the Manage Presets dialog (renaming, describing, or deleting user
    /// presets immediately as requested); returns `None` once closed.
    fn manage_dialog(
        &mut self,
        ctx: &egui::Context,
        dir: &Path,
        mut managing: Managing,
    ) -> Option<Managing> {
        let mut close = false;
        let mut changes = Vec::new();
        let modal = widgets::dialog(ctx, "manage presets").width(560.0).show(ctx, |ui| {
            close = widgets::dialog_title(ui, widgets::title_text(ui, fl!("your-presets")));
            widgets::note(ui, fl!("your-presets-note"));
            ui.add_space(8.0);
            if managing.rows.is_empty() && managing.deleted.is_empty() {
                widgets::note(ui, fl!("no-presets"));
            }
            // Scroll the preset list so the header and Close button stay visible.
            let room = (ctx.content_rect().height() - 240.0).max(120.0);
            widgets::scrolled(ui, egui::ScrollArea::vertical().max_height(room), |ui| {
                egui::Grid::new("presets").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                    if !managing.rows.is_empty() {
                        ui.label(RichText::new(fl!("preset-name")).weak());
                        ui.label(RichText::new(fl!("preset-description")).weak());
                        ui.end_row();
                    }
                    // Pressing Escape in a field reverts its text and consumes the
                    // key so the dialog stays open until a second Escape.
                    let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
                    for row in &mut managing.rows {
                        // Explicit size prevents the grid from shrinking
                        // columns to their header width.
                        let line = ui.spacing().interact_size.y;
                        let name = widgets::text_field(&mut row.typed_name);
                        if ui.add_sized(vec2(170.0, line), name).lost_focus() {
                            if escaped {
                                row.typed_name.clone_from(&row.name);
                                widgets::escape_taken(ui);
                            } else if row.typed_name.trim_end() != row.name {
                                changes.push(Change::Rename(row.id));
                            }
                        }
                        match &row.broken {
                            Some(why) => {
                                ui.colored_label(theme::bad(ui.visuals()), fl!("preset-broken"))
                                    .on_hover_text(why);
                            }
                            None => {
                                let field = widgets::text_field(&mut row.typed_description)
                                    .hint_text(fl!("preset-description", "none"));
                                if ui.add_sized(vec2(300.0, line), field).lost_focus() {
                                    if escaped {
                                        row.typed_description.clone_from(&row.description);
                                        widgets::escape_taken(ui);
                                    } else if row.typed_description.trim() != row.description {
                                        changes.push(Change::Describe(row.id));
                                    }
                                }
                            }
                        }
                        let cross =
                            RichText::new("✖").size(12.0).color(ui.visuals().weak_text_color());
                        let delete = egui::Button::new(cross).frame(false);
                        let delete = widgets::named(ui.add(delete), &fl!("delete-it"));
                        if delete.on_hover_text(fl!("delete-it")).clicked() {
                            changes.push(Change::Delete(row.id));
                        }
                        ui.end_row();
                        if let Some(problem) = &row.problem {
                            ui.colored_label(theme::bad(ui.visuals()), problem);
                            ui.end_row();
                        }
                    }
                });
                for deleted in &managing.deleted {
                    ui.horizontal(|ui| {
                        widgets::note(ui, fl!("deleted", name = deleted.name.as_str()));
                        if ui.button(fl!("undo")).on_hover_text(fl!("undo", "hint")).clicked() {
                            changes.push(Change::Undo(deleted.id));
                        }
                    });
                }
            });
            widgets::dialog_buttons(ui, |ui| {
                close |= ui.add(widgets::dialog_button(fl!("close"))).clicked();
            });
        });
        // Apply changes in order: clicking a button while editing a field
        // leaves the field first, queueing its rename/describe before the
        // click action.
        for change in changes {
            self.change_preset(dir, &mut managing, change);
        }
        (!close && !modal.should_close()).then_some(managing)
    }

    /// Applies `change` without recording an item undo step for items that
    /// follow a renamed or deleted preset; the new settings are simply how
    /// those items are from then on.
    fn change_preset(&mut self, dir: &Path, managing: &mut Managing, change: Change) {
        self.settle_edits();
        self.make_change(dir, managing, change);
        self.take_in_edits();
    }

    fn make_change(&mut self, dir: &Path, managing: &mut Managing, change: Change) {
        let file = |name: &str| dir.join(format!("{name}.toml"));
        let row = |id: u64| managing.rows.iter().position(|row| row.id == id);
        match change {
            Change::Rename(id) => {
                let Some(index) = row(id) else { return };
                let row = &mut managing.rows[index];
                let to = row.typed_name.trim_end().to_owned();
                match presets::rename(dir, &row.name, &to, &self.presets) {
                    Ok(_) => {
                        let from = std::mem::replace(&mut row.name, to.clone());
                        row.problem = None;
                        self.follow_preset(&from, Some(&to));
                    }
                    Err(err) => row.problem = Some(problem(&err)),
                }
            }
            Change::Describe(id) => {
                let Some(index) = row(id) else { return };
                let row = &mut managing.rows[index];
                let description = row.typed_description.trim().to_owned();
                match presets::set_description(&file(&row.name), &description) {
                    Ok(()) => (row.description, row.problem) = (description, None),
                    Err(err) => row.problem = Some(failure(&err)),
                }
            }
            Change::Delete(id) => {
                let Some(index) = row(id) else { return };
                match presets::remove(dir, &managing.rows[index].name, &self.presets) {
                    Ok(text) => {
                        let name = managing.rows.remove(index).name;
                        let items = self
                            .items
                            .iter()
                            .filter(|item| item.choices.preset.as_deref() == Some(name.as_str()))
                            .map(|item| item.id)
                            .collect();
                        let defaults = (
                            self.config.preset.as_deref() == Some(name.as_str()),
                            self.config.tgs_preset.as_deref() == Some(name.as_str()),
                        );
                        let ranges = self.follow_preset(&name, None);
                        let deleted = Deleted { id, name, text, items, defaults, ranges };
                        managing.deleted.push(deleted);
                    }
                    Err(err) => managing.rows[index].problem = Some(problem(&err)),
                }
            }
            Change::Undo(id) => {
                let Some(index) = managing.deleted.iter().position(|deleted| deleted.id == id)
                else {
                    return;
                };
                let deleted = &managing.deleted[index];
                match presets::restore(dir, &deleted.name, &deleted.text) {
                    Ok(_) => {
                        let deleted = managing.deleted.remove(index);
                        let name = Some(deleted.name.clone());
                        for item in &mut self.items {
                            if deleted.items.contains(&item.id) && item.choices.preset.is_none() {
                                item.choices.preset = name.clone();
                            }
                        }
                        let (webm, tgs) = deleted.defaults;
                        self.save_config_setting(|config| {
                            if webm {
                                config.preset = name.clone();
                            }
                            if tgs {
                                config.tgs_preset = name.clone();
                            }
                        });
                        self.reload_presets();
                        // Restore fit ranges dropped when the fitting unit changed,
                        // provided the restored preset uses that unit again and no
                        // new range was set in the meantime.
                        for &(id, range, fit) in &deleted.ranges {
                            let Some(item) = self.items.iter_mut().find(|item| item.id == id)
                            else {
                                continue;
                            };
                            let now = crate::file_settings::fit_of(
                                &item.choices,
                                &self.presets,
                                &self.config,
                            );
                            let webm = &mut item.choices.webm;
                            if now.unit() == fit.unit() && webm.fit_range.is_none() {
                                webm.fit_range = Some(range);
                            }
                        }
                        let row = match self.presets.get(&deleted.name) {
                            Ok(preset) => Row::of(id, &deleted.name, Ok(preset)),
                            Err(err) => Row::of(id, &deleted.name, Err(err.to_string())),
                        };
                        let at = managing.rows.partition_point(|other| other.name < row.name);
                        managing.rows.insert(at, row);
                        return;
                    }
                    Err(err) => {
                        let message = fl!("not-put-back", name = deleted.name.as_str());
                        self.say(crate::app::told(Message::warning(message), &err));
                    }
                }
            }
        }
        self.reload_presets();
    }

    /// Updates items and `config` references from preset `from` to `to` (or to
    /// the default preset when `to` is `None`). Returns any per-item fit
    /// ranges dropped due to a fitting-unit change, paired with their previous
    /// [`Fit`] mode.
    fn follow_preset(&mut self, from: &str, to: Option<&str>) -> Vec<(u64, Range, Fit)> {
        // Deleting a preset falls back to the default, which may use a
        // different fitting unit and invalidate an item's custom fit_range
        // (renaming keeps the same fitting mode). Evaluating against
        // `self.presets` before reload is fine because the fallback preset is
        // already loaded.
        let fits: Option<Vec<Fit>> = to.is_none().then(|| {
            let fit = |item: &Item| {
                crate::file_settings::fit_of(&item.choices, &self.presets, &self.config)
            };
            self.items.iter().map(fit).collect()
        });
        for item in &mut self.items {
            if item.choices.preset.as_deref() == Some(from) {
                item.choices.preset = to.map(str::to_owned);
            }
            // Update undo history on rename so undoing does not reference the
            // old name; on delete, leave history entries intact in case the
            // dialog's Undo restores the preset.
            if let Some(to) = to {
                item.history.rename_preset(from, to);
            }
        }
        // Rename it in recently removed items too (they may be put back);
        // after a delete they keep its name, as undo history does.
        if let Some(to) = to {
            for item in self.removed.iter_mut().flat_map(crate::app::Removed::items_mut) {
                if item.choices.preset.as_deref() == Some(from) {
                    item.choices.preset = Some(to.to_owned());
                }
                item.history.rename_preset(from, to);
            }
        }
        let names = |name: &Option<String>| name.as_deref() == Some(from);
        if names(&self.config.preset) || names(&self.config.tgs_preset) {
            self.save_config_setting(|config| {
                let config: &mut tgradish_core::config::Config = config;
                for name in [&mut config.preset, &mut config.tgs_preset] {
                    if name.as_deref() == Some(from) {
                        *name = to.map(str::to_owned);
                    }
                }
            });
        }
        let mut dropped = Vec::new();
        if let Some(fits) = fits {
            for (item, before) in self.items.iter_mut().zip(fits) {
                let after =
                    crate::file_settings::fit_of(&item.choices, &self.presets, &self.config);
                let range = item.choices.webm.fit_range;
                crate::file_settings::follow_fit(&mut item.choices.webm, before, after);
                if let (Some(range), None) = (range, item.choices.webm.fit_range) {
                    dropped.push((item.id, range, before));
                }
            }
        }
        dropped
    }
}

/// Switches `item` to preset `name` and clears per-item format overrides that
/// are now stored in the preset, keeping only file-specific fields (title and
/// sprite-sheet layout).
fn take_up(item: &mut Item, name: &str) {
    let choices = &mut item.choices;
    choices.preset = Some(name.to_owned());
    choices.webm = Options { title: choices.webm.title.take(), ..Options::default() };
    let tgs = &mut choices.tgs;
    *tgs = tgs::Options {
        title: tgs.title.take(),
        sheet: tgs.sheet.take(),
        sheet_frames: tgs.sheet_frames.take(),
        ..tgs::Options::default()
    };
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::kittest::Queryable;
    use tgradish_core::options::{Fit, Resize};
    use tgradish_core::presets::{self, Format, Presets};
    use tgradish_core::tgs;
    use tgradish_core::webm::Options;

    use super::{Change, Dialog};
    use crate::l10n::fl;
    use crate::testing::{finish, harness, square};

    #[test]
    fn saves_settings_as_a_preset_and_takes_it_up() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().presets_dir = Some(dir.path().join("presets"));
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        let item = &mut harness.state_mut().items[0];
        item.format = Format::Webm;
        item.choices.webm.fit = Some(Fit::Crf);
        item.choices.webm.resize = Some(Resize::Crop);
        item.choices.webm.title = Some("only this one".into());
        harness.run_steps(2);
        harness.get_by_label(&fl!("save-preset")).click();
        harness.run_steps(2);
        // into its name field, which has the focus
        harness
            .query_all_by_role(eframe::egui::accesskit::Role::TextInput)
            .next()
            .unwrap()
            .type_text("by quality");
        harness.run_steps(2);
        harness.get_by_label(&fl!("save")).click();
        harness.run_steps(3);
        // the item takes it up, its title its own still
        let state = harness.state();
        assert!(state.preset_dialog.is_none());
        let choices = &state.items[0].choices;
        assert_eq!(choices.preset.as_deref(), Some("by quality"));
        assert_eq!(
            (choices.webm.fit, choices.webm.title.as_deref()),
            (None, Some("only this one"))
        );
        // what it makes, and shows, comes from the preset now
        let effective =
            choices.recipe(Format::Webm, &state.presets, &state.config).unwrap().into_webm();
        assert_eq!(crate::file_settings::resize_of(&effective), Resize::Crop);
        let saved = state.presets.webm("by quality").unwrap();
        assert_eq!((saved.fit, saved.title), (Some(Fit::Crf), None));
        assert!(dir.path().join("presets/by quality.toml").is_file());
    }

    /// The GUI's twin of the command line's test of the same name in
    /// crates/cli/src/convert.rs, which takes up the preset saved here.
    #[test]
    fn integer_scale_of_a_preset_the_gui_saved() {
        use tgradish_core::options::Scaling;
        use tgradish_core::telegram::Target;
        let dir = tempfile::tempdir().unwrap();
        let presets_dir = dir.path().join("presets");
        std::fs::create_dir(&presets_dir).unwrap();
        std::fs::write(presets_dir.join("scaled.toml"), "[webm]\ninteger-scale = '2'\n").unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        let state = harness.state_mut();
        state.presets_dir = Some(presets_dir.clone());
        state.presets = Presets::load(Some(&presets_dir)).unwrap();
        state.add(vec![art]);
        finish(&mut harness);
        let item = &mut harness.state_mut().items[0];
        item.format = Format::Webm;
        item.choices.preset = Some("scaled".into());
        item.choices.webm.scaling = Some(Scaling::Smooth);
        // the scale set aside, as only integer scaling scales by it
        // (divergence 2)
        let smooth = Options {
            target: Some(Target::Sticker),
            scaling: Some(Scaling::Smooth),
            ..Options::default()
        };
        let recipe = |harness: &egui_kittest::Harness<crate::app::App>| {
            let state = harness.state();
            state.items[0].choices.recipe(Format::Webm, &state.presets, &state.config).unwrap()
        };
        assert_eq!(recipe(&harness), crate::item::Recipe::Webm(smooth.clone()));
        harness.run_steps(2);
        harness.get_by_label(&fl!("save-preset")).click();
        harness.run_steps(2);
        harness
            .query_all_by_role(eframe::egui::accesskit::Role::TextInput)
            .next()
            .unwrap()
            .type_text("mine");
        harness.run_steps(2);
        harness.get_by_label(&fl!("save")).click();
        harness.run_steps(3);
        // and saved so, the scale cleared, as the command line takes it
        // up too
        let saved = std::fs::read_to_string(presets_dir.join("mine.toml")).unwrap();
        assert_eq!(saved, "[tgs]\n\n[webm]\nscaling = \"smooth\"\n");
        assert_eq!(harness.state().items[0].choices.preset.as_deref(), Some("mine"));
        assert_eq!(recipe(&harness), crate::item::Recipe::Webm(smooth));
    }

    #[test]
    fn leaves_the_first_escape_to_a_name_typed_in() {
        let dir = tempfile::tempdir().unwrap();
        let presets_dir = dir.path().join("presets");
        let webm = Options { crf: Some(20), fit: Some(Fit::Crf), ..Options::default() };
        presets::save(&presets_dir, "mine", "", (&webm, &tgs::Options::default()), false).unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        let state = harness.state_mut();
        state.presets_dir = Some(presets_dir.clone());
        state.presets = Presets::load(Some(&presets_dir)).unwrap();
        state.add(vec![art]);
        finish(&mut harness);
        harness.get_by_label(&fl!("manage-presets")).click();
        harness.run_steps(2);
        let name = egui_kittest::kittest::by()
            .role(eframe::egui::accesskit::Role::TextInput)
            .value("mine");
        harness.get(name).focus();
        harness.run_steps(2);
        harness.event(eframe::egui::Event::Text("s".into()));
        harness.run_steps(2);
        let managing = |harness: &egui_kittest::Harness<crate::app::App>| {
            matches!(harness.state().preset_dialog, Some(Dialog::Manage(_)))
        };
        // the name put back, not taken, the presets still open
        harness.key_press(eframe::egui::Key::Escape);
        harness.run_steps(3);
        assert!(managing(&harness));
        assert!(harness.state().presets.get("mine").is_ok());
        let name = egui_kittest::kittest::by()
            .role(eframe::egui::accesskit::Role::TextInput)
            .value("mine");
        assert!(harness.query(name).is_some());
        // and closed by the next
        harness.key_press(eframe::egui::Key::Escape);
        harness.run_steps(3);
        assert!(!managing(&harness));
        assert!(harness.state().presets.get("mine").is_ok());
    }

    #[test]
    fn renames_and_deletes_presets_and_puts_them_back() {
        let dir = tempfile::tempdir().unwrap();
        let presets_dir = dir.path().join("presets");
        let webm = Options { crf: Some(20), fit: Some(Fit::Crf), ..Options::default() };
        presets::save(&presets_dir, "mine", "", (&webm, &tgs::Options::default()), false).unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let config =
            tgradish_core::config::Config { preset: Some("mine".into()), ..Default::default() };
        let config = Config::from(config);
        let mut harness = harness(config);
        let state = harness.state_mut();
        state.presets_dir = Some(presets_dir.clone());
        state.presets = Presets::load(Some(&presets_dir)).unwrap();
        state.add(vec![art]);
        finish(&mut harness);
        // a range of qualities to search, as the preset fits by quality
        let range = tgradish_core::options::Range { min: 24.0, max: 40.0 };
        let choices = &mut harness.state_mut().items[0].choices;
        (choices.preset, choices.webm.fit_range) = (Some("mine".into()), Some(range));
        harness.run_steps(2);
        harness.get_by_label(&fl!("manage-presets")).click();
        harness.run_steps(2);
        harness.get_by_label(&fl!("your-presets"));
        let state = harness.state_mut();
        let Some(Dialog::Manage(mut managing)) = state.preset_dialog.take() else { panic!() };
        assert_eq!(managing.rows.len(), 1);

        // renamed: what named it follows
        managing.rows[0].typed_name = "yours".into();
        state.change_preset(&presets_dir, &mut managing, Change::Rename(0));
        assert_eq!(managing.rows[0].problem, None);
        assert_eq!(state.items[0].choices.preset.as_deref(), Some("yours"));
        assert_eq!(state.config.preset.as_deref(), Some("yours"));
        assert_eq!(state.presets.webm("yours").unwrap().crf, Some(20));

        // deleted: back to the default, which fits by bitrate, until Undo
        state.change_preset(&presets_dir, &mut managing, Change::Delete(0));
        assert!(managing.rows.is_empty() && state.presets.get("yours").is_err());
        assert_eq!(
            (state.items[0].choices.preset.as_deref(), state.config.preset.as_deref()),
            (None, None)
        );
        assert_eq!(state.items[0].choices.webm.fit_range, None);
        state.change_preset(&presets_dir, &mut managing, Change::Undo(0));
        assert_eq!(managing.rows.len(), 1);
        assert_eq!(state.items[0].choices.preset.as_deref(), Some("yours"));
        assert_eq!(state.items[0].choices.webm.fit_range, Some(range));
        assert_eq!(state.config.preset.as_deref(), Some("yours"));
        assert_eq!(state.presets.webm("yours").unwrap().crf, Some(20));

        // a field left by clicking Delete: both are done, in turn
        managing.rows[0].typed_name = "ours".into();
        for change in [Change::Rename(0), Change::Delete(0)] {
            state.change_preset(&presets_dir, &mut managing, change);
        }
        assert!(managing.rows.is_empty());
        assert_eq!(managing.deleted[0].name, "ours");
        // not onto a built-in preset's name
        state.change_preset(&presets_dir, &mut managing, Change::Undo(0));
        managing.rows[0].typed_name = "fast".into();
        state.change_preset(&presets_dir, &mut managing, Change::Rename(0));
        assert!(managing.rows[0].problem.is_some());
        assert_eq!(state.presets.webm("fast").unwrap().crf, None);
    }

    #[test]
    fn renames_a_preset_in_files_removed_a_moment_ago_too() {
        let dir = tempfile::tempdir().unwrap();
        let presets_dir = dir.path().join("presets");
        let webm = Options { crf: Some(20), fit: Some(Fit::Crf), ..Options::default() };
        presets::save(&presets_dir, "mine", "", (&webm, &tgs::Options::default()), false).unwrap();
        let arts = ["first.png", "second.png"].map(|name| dir.path().join(name));
        arts.iter().for_each(|art| square(art, [255, 0, 0, 255]));
        let mut harness = harness(Config::default());
        let state = harness.state_mut();
        state.presets_dir = Some(presets_dir.clone());
        state.presets = Presets::load(Some(&presets_dir)).unwrap();
        state.add(arts.to_vec());
        finish(&mut harness);
        for item in &mut harness.state_mut().items {
            item.choices.preset = Some("mine".into());
        }
        harness.run_steps(2);
        // the second, with a step back that names it too, removed
        let removed = harness.state().items[1].id;
        harness.state_mut().select(removed);
        finish(&mut harness);
        harness.get_by_label(&fl!("target-emoji")).click();
        harness.run_steps(3);
        assert!(harness.state().item(removed).unwrap().history.can_undo());
        harness.state_mut().remove(removed);
        harness.run_steps(2);
        harness.get_by_label(&fl!("manage-presets")).click();
        harness.run_steps(2);
        let state = harness.state_mut();
        let Some(Dialog::Manage(mut managing)) = state.preset_dialog.take() else { panic!() };
        managing.rows[0].typed_name = "yours".into();
        state.change_preset(&presets_dir, &mut managing, Change::Rename(0));
        assert_eq!(state.items[0].choices.preset.as_deref(), Some("yours"));
        // put back, it names the preset as it is called now, and so does
        // each step back
        state.undo_removing();
        let item = state.item(removed).unwrap();
        assert_eq!(item.choices.preset.as_deref(), Some("yours"));
        let id = item.id;
        harness.state_mut().select(id);
        harness.run_steps(2);
        let item = harness.state_mut().item_mut(id).unwrap();
        while crate::history::step(item, true) {}
        assert_eq!(item.choices.preset.as_deref(), Some("yours"));
    }

    #[cfg(unix)]
    #[test]
    fn says_presets_are_unreadable_under_what_it_said_before() {
        let dir = tempfile::tempdir().unwrap();
        let Some(unlistable) = crate::testing::unlistable(dir.path()) else { return };
        let mut app = crate::app::App::with(Config::default(), None, Presets::builtin());
        app.presets_dir = Some(unlistable.clone());
        app.say(crate::banner::Message::info("Something said before"));
        app.reload_presets();
        crate::testing::listable(&unlistable);
        let said = crate::testing::said(&app);
        assert!(said.starts_with("Something said before\n"), "{said}");
        assert!(said.contains("presets couldn't be read"), "{said}");
    }

    #[test]
    fn keeps_a_preset_another_extends() {
        let dir = tempfile::tempdir().unwrap();
        let webm = Options { crf: Some(20), ..Options::default() };
        presets::save(dir.path(), "base", "", (&webm, &tgs::Options::default()), false).unwrap();
        std::fs::write(dir.path().join("child.toml"), "extends = 'base'\n").unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let mut app = crate::app::App::with(Config::default(), None, presets);
        app.presets_dir = Some(dir.path().to_path_buf());
        let mut managing = super::Managing::of(&app.presets);
        let id = managing.rows.iter().find(|row| row.name == "base").unwrap().id;
        // neither renamed nor deleted, and said why in the GUI's words
        let said = crate::l10n::fl!("preset-extended", other = "child");
        let row = |managing: &super::Managing| {
            managing.rows.iter().find(|row| row.id == id).cloned().unwrap()
        };
        managing.rows.iter_mut().find(|row| row.id == id).unwrap().typed_name = "other".into();
        app.change_preset(dir.path(), &mut managing, Change::Rename(id));
        assert_eq!(
            (row(&managing).name, row(&managing).problem),
            ("base".into(), Some(said.clone()))
        );
        app.change_preset(dir.path(), &mut managing, Change::Delete(id));
        assert_eq!(row(&managing).problem, Some(said));
        assert!(app.presets.get("base").is_ok() && dir.path().join("base.toml").exists());
    }

    #[test]
    fn drops_a_range_the_default_fits_by_another_unit_once_a_preset_is_gone() {
        use tgradish_core::options::Range;
        use tgradish_core::telegram::Target;

        let dir = tempfile::tempdir().unwrap();
        let webm = Options { fit: Some(Fit::Crf), ..Options::default() };
        presets::save(dir.path(), "by quality", "", (&webm, &tgs::Options::default()), false)
            .unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let mut app = crate::app::App::with(Config::default(), None, presets);
        let qualities = Some(Range { min: 20.0, max: 40.0 });
        for id in [1, 2] {
            let mut item =
                crate::item::Item::new(id, vec!["pig.mp4".into()], false, Target::Sticker);
            item.choices.preset = Some("by quality".into());
            item.choices.webm.fit_range = qualities;
            app.items.push(item);
        }
        // the second fits by quality itself
        app.items[1].choices.webm.fit = Some(Fit::Crf);
        let mut managing = super::Managing::of(&app.presets);
        let id = managing.rows.iter().find(|row| row.name == "by quality").unwrap().id;
        app.change_preset(dir.path(), &mut managing, Change::Delete(id));
        // the default fits by bitrate, which a range of qualities isn't
        let ranges: Vec<_> = app.items.iter().map(|item| item.choices.webm.fit_range).collect();
        assert_eq!(ranges, [None, qualities]);
    }
}

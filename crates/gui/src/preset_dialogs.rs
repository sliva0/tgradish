//! The user's own presets: the selected item's settings saved as one, and
//! a dialog to rename, describe or delete them. The settings and the bar
//! under them ask for them; the window opens them over everything.
//!
//! An item saved as a preset takes it up: its own values over the preset
//! it started from, of every format, but none that only make sense for its
//! file: its title and sprite sheet layout, which the item keeps as its
//! own. Its crop, cut, Aseprite tag and layers are kept apart from the
//! preset's values in [`crate::item::Choices`] anyway, so they stay as
//! they are.

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
use crate::widgets;

/// A dialog the settings ask the window for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Ask {
    /// Saving the settings of the item with this id.
    Save(u64),
    Manage,
}

fn ask_id() -> egui::Id {
    egui::Id::new("preset dialog asked for")
}

/// Asks the window to open a preset dialog.
pub(crate) fn ask(ctx: &egui::Context, ask: Ask) {
    ctx.data_mut(|data| data.insert_temp(ask_id(), ask));
}

/// An open preset dialog and what is typed into it.
pub(crate) enum Dialog {
    Save(Saving),
    Manage(Managing),
}

pub(crate) struct Saving {
    item: u64,
    name: String,
    description: String,
    problem: Option<String>,
    /// The name field has had the focus, as it gets when the dialog opens.
    focused: bool,
}

/// The user's presets as the dialog edits them.
pub(crate) struct Managing {
    rows: Vec<Row>,
    /// Presets deleted while it was open, which Undo puts back.
    deleted: Vec<Deleted>,
}

#[derive(Debug, Clone)]
struct Row {
    /// Which row it is, as rows come and go.
    id: u64,
    /// Its name, as its file is called.
    name: String,
    description: String,
    /// What is typed into its fields, kept as they lose the focus.
    typed_name: String,
    typed_description: String,
    /// Why its file couldn't be read.
    broken: Option<String>,
    /// Why the last change couldn't be made.
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
    /// The id of its row, which Undo brings back.
    id: u64,
    name: String,
    /// What its file held.
    text: String,
    /// The items that were made with it, which take it up again with Undo.
    items: Vec<u64>,
    /// Whether the config made it the default for WebM and pictures, and
    /// for `.tgs`.
    defaults: (bool, bool),
    /// The items' own ranges to search that went with it, by item, with
    /// the way of fitting they were for.
    ranges: Vec<(u64, Range, Fit)>,
}

impl Managing {
    fn of(presets: &Presets) -> Managing {
        // built-in presets have no file, and are never broken
        let rows = presets
            .iter()
            .filter(|(_, preset)| preset.is_err() || preset.is_ok_and(is_users))
            .enumerate()
            .map(|(id, (name, preset))| Row::of(id as u64, name, preset.map_err(|e| e.to_string())))
            .collect();
        Managing { rows, deleted: Vec::new() }
    }
}

/// Whether the user made `preset`, rather than it being built in.
pub(crate) fn is_users(preset: &presets::Preset) -> bool {
    preset.path.is_some()
}

/// What the manage dialog asks to change, once it is drawn: a row, by its
/// id, which stays as other changes move the rows.
enum Change {
    Rename(u64),
    Describe(u64),
    Delete(u64),
    Undo(u64),
}

/// What keeps a preset from being named, renamed or deleted as asked, in
/// the window's words.
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

/// What is wrong with `name` as a preset's, as tgradish-core checks it.
fn name_problem(name: &str) -> Option<String> {
    presets::check_name(name).err().map(|err| problem(&err))
}

/// A dialog's title, with a cross in the corner, level with it, that
/// closes it.
fn title(ui: &mut egui::Ui, text: &str) -> bool {
    ui.horizontal(|ui| {
        ui.heading(text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let side = ui.spacing().interact_size.y;
            let cross = egui::Button::new("✖").frame(false).min_size(vec2(side, side));
            ui.add(cross).on_hover_text(fl!("close")).clicked()
        })
        .inner
    })
    .inner
}

/// A dialog's main button: framed and strong, so it reads as the way on.
fn main_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong()).min_size(vec2(90.0, 28.0))
}

impl App {
    /// Opens a preset dialog the settings asked for, and shows the one
    /// open.
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

    /// Reads the presets again, as after one changed; gives whether they
    /// could be.
    pub(crate) fn reload_presets(&mut self) -> bool {
        match Presets::load(self.presets_dir.as_deref()) {
            Ok(presets) => {
                self.presets = presets;
                self.banner.forget(Subject::Presets);
                true
            }
            Err(err) => {
                let (file, detail) = crate::app::error_parts(&err);
                let message = Message::warning(fl!("presets-unreadable")).detail(detail);
                let message = match file.or_else(|| self.presets_dir.clone()) {
                    Some(file) => message.file(file),
                    None => message,
                };
                let actions = [Action::ReadPresetsAgain, Action::CopyPath, Action::ShowInFolder];
                self.say(message.actions(&actions).about(Subject::Presets));
                false
            }
        }
    }

    /// The dialog that saves an item's settings as a preset; `None` once
    /// it is done or closed.
    fn save_dialog(
        &mut self,
        ctx: &egui::Context,
        dir: &Path,
        mut saving: Saving,
    ) -> Option<Saving> {
        // the item is gone
        self.item(saving.item)?;
        let name = saving.name.trim_end().to_owned();
        // a built-in one's name would stand in for it, for every file
        let builtin = self.presets.get(&name).is_ok_and(|preset| !is_users(preset));
        let valid = match name_problem(&name) {
            Some(problem) => Err(problem),
            None if builtin => Err(fl!("preset-name-builtin", name = name.as_str())),
            None => Ok(()),
        };
        let exists = valid.is_ok() && dir.join(format!("{name}.toml")).exists();
        let (mut close, mut save) = (false, false);
        let modal = egui::Modal::new(egui::Id::new("save preset")).show(ctx, |ui| {
            ui.set_width(400.0);
            close = title(ui, &fl!("save-preset-title"));
            widgets::note(ui, fl!("save-preset-note"));
            ui.add_space(8.0);
            egui::Grid::new("save preset fields").num_columns(2).spacing([10.0, 8.0]).show(
                ui,
                |ui| {
                    ui.label(fl!("preset-name"));
                    let field = egui::TextEdit::singleline(&mut saving.name)
                        .hint_text(fl!("preset-name", "placeholder"))
                        .desired_width(270.0);
                    let field = ui.add(field);
                    if !saving.focused {
                        field.request_focus();
                        saving.focused = true;
                    }
                    // Enter in it saves, as the button would
                    if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        save = valid.is_ok();
                    }
                    ui.end_row();
                    ui.label(fl!("preset-description"));
                    let field = egui::TextEdit::singleline(&mut saving.description)
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
                ui.colored_label(widgets::BAD, problem);
            }
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                let text = if exists { fl!("replace") } else { fl!("save") };
                let button = ui.add_enabled(valid.is_ok(), main_button(&text));
                let button = match &valid {
                    Err(err) if !name.is_empty() => button.on_disabled_hover_text(err),
                    _ => button.on_disabled_hover_text(fl!("name-it-first")),
                };
                save |= button.clicked();
                let cancel = egui::Button::new(fl!("cancel")).min_size(vec2(0.0, 28.0));
                close |= ui.add(cancel).clicked();
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
        let webm = choices.webm_base(presets, config).unwrap_or_default().merged(&choices.webm);
        let tgs = choices.tgs_base(presets, config).unwrap_or_default().merged(&choices.tgs);
        let description = saving.description.trim().to_owned();
        match presets::save(dir, &name, &description, (&webm, &tgs), exists) {
            Ok(_) => {
                if let Some(item) = self.item_mut(saving.item) {
                    take_up(item, &name);
                }
                self.reload_presets();
                None
            }
            Err(err) => {
                saving.problem = Some(failure(&err));
                Some(saving)
            }
        }
    }

    /// The dialog that renames, describes or deletes the user's presets;
    /// `None` once closed. Each change is made as it is asked for.
    fn manage_dialog(
        &mut self,
        ctx: &egui::Context,
        dir: &Path,
        mut managing: Managing,
    ) -> Option<Managing> {
        let mut close = false;
        let mut changes = Vec::new();
        let modal = egui::Modal::new(egui::Id::new("manage presets")).show(ctx, |ui| {
            ui.set_width(560.0);
            close = title(ui, &fl!("your-presets"));
            widgets::note(ui, fl!("your-presets-note"));
            ui.add_space(8.0);
            if managing.rows.is_empty() && managing.deleted.is_empty() {
                widgets::note(ui, fl!("no-presets"));
            }
            // the list scrolls, its title and Close staying in sight
            let room = (ctx.content_rect().height() - 240.0).max(120.0);
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                egui::Grid::new("presets").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                    if !managing.rows.is_empty() {
                        ui.label(RichText::new(fl!("preset-name")).weak());
                        ui.label(RichText::new(fl!("preset-description")).weak());
                        ui.end_row();
                    }
                    for row in &mut managing.rows {
                        // sized, as a grid keeps a column as narrow as its header
                        let line = ui.spacing().interact_size.y;
                        let name = egui::TextEdit::singleline(&mut row.typed_name)
                            .vertical_align(egui::Align::Center);
                        if ui.add_sized(vec2(170.0, line), name).lost_focus()
                            && row.typed_name.trim_end() != row.name
                        {
                            changes.push(Change::Rename(row.id));
                        }
                        match &row.broken {
                            Some(why) => {
                                ui.colored_label(widgets::BAD, fl!("preset-broken"))
                                    .on_hover_text(why);
                            }
                            None => {
                                let field = egui::TextEdit::singleline(&mut row.typed_description)
                                    .hint_text(fl!("preset-description", "none"))
                                    .vertical_align(egui::Align::Center);
                                if ui.add_sized(vec2(300.0, line), field).lost_focus()
                                    && row.typed_description.trim() != row.description
                                {
                                    changes.push(Change::Describe(row.id));
                                }
                            }
                        }
                        let cross =
                            RichText::new("✖").size(12.0).color(ui.visuals().weak_text_color());
                        let delete = egui::Button::new(cross).frame(false);
                        if ui.add(delete).on_hover_text(fl!("delete-it")).clicked() {
                            changes.push(Change::Delete(row.id));
                        }
                        ui.end_row();
                        if let Some(problem) = &row.problem {
                            ui.colored_label(widgets::BAD, problem);
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
            ui.add_space(4.0);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = egui::Button::new(fl!("close")).min_size(vec2(90.0, 28.0));
                close |= ui.add(button).clicked();
            });
        });
        // in the order asked, as a field left by clicking a button asks
        // for its own change first
        for change in changes {
            self.change_preset(dir, &mut managing, change);
        }
        (!close && !modal.should_close()).then_some(managing)
    }

    fn change_preset(&mut self, dir: &Path, managing: &mut Managing, change: Change) {
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
                        // ranges dropped as fitting changed, where it is as
                        // it was again and nothing took their place
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
                        let message = Message::warning(fl!("not-put-back"));
                        self.say(message.detail(failure(&err)));
                    }
                }
            }
        }
        self.reload_presets();
    }

    /// Items and the config that named the preset `from` name `to` now, or
    /// the default preset when it is gone. Gives the items' own ranges
    /// that dropped, with the way of fitting they were for.
    fn follow_preset(&mut self, from: &str, to: Option<&str>) -> Vec<(u64, Range, Fit)> {
        // the default may fit another way than a deleted preset, so that an
        // item's own range no longer suits it, as when another is chosen; a
        // renamed one fits as before. The presets are read again only
        // later, which doesn't matter: what the items name now is in them
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

/// The item made with the preset `name`, which now holds its values, but
/// for those of its file alone.
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
        harness.get_by_label("Save current settings as preset…").click();
        harness.run_steps(2);
        // into its name field, which has the focus
        harness
            .query_all_by_role(eframe::egui::accesskit::Role::TextInput)
            .next()
            .unwrap()
            .type_text("by quality");
        harness.run_steps(2);
        harness.get_by_label("Save").click();
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
        let effective = choices.webm_options(&state.presets, &state.config).unwrap();
        assert_eq!(crate::file_settings::resize_of(&effective), Resize::Crop);
        let saved = state.presets.webm("by quality").unwrap();
        assert_eq!((saved.fit, saved.title), (Some(Fit::Crf), None));
        assert!(dir.path().join("presets/by quality.toml").is_file());
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
        harness.get_by_label("Manage…").click();
        harness.run_steps(2);
        harness.get_by_label("Your presets");
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
        // neither renamed nor deleted, and said why in the window's words
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

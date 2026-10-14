//! The item's settings saved as a preset of the user's own, which the item
//! takes up then: its own values over the preset it started from, of every
//! format, but none that only make sense for its file: its title and
//! sprite sheet layout, which the item keeps as its own. Its crop, cut,
//! Aseprite tag and layers are kept apart from the preset's values in
//! [`crate::item::Choices`] anyway, so they stay as they are.

use eframe::egui;
use eframe::egui::containers::menu::{MenuButton, MenuConfig};
use tgradish_core::options::Options;
use tgradish_core::presets;
use tgradish_core::tgs::TgsOptions;

use super::Context;
use crate::item::Item;

/// Asks the window to read the presets again, as one was saved.
pub fn presets_saved(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(saved_id())).unwrap_or(false)
}

fn saved_id() -> egui::Id {
    egui::Id::new("preset saved")
}

/// Width of the fields of a new preset.
const FIELD: f32 = 200.0;

/// A button that saves the item's settings as a preset, named and
/// described in what it opens.
pub(super) fn save_preset(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let Some(dir) = context.presets_dir.map(std::path::Path::to_path_buf) else { return };
    // typing in it keeps it open
    let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    MenuButton::new("Save as preset…").config(config).ui(ui, |ui| {
        let id = ui.id().with(("new preset", item.id));
        let (mut name, mut description, mut problem): (String, String, Option<String>) =
            ui.data(|data| data.get_temp(id)).unwrap_or_default();
        egui::Grid::new(id.with("fields")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Name");
            // as wide when it first opens, before it is measured
            ui.add(
                egui::TextEdit::singleline(&mut name).hint_text("my stickers").desired_width(FIELD),
            );
            ui.end_row();
            ui.label("Description");
            let field = egui::TextEdit::singleline(&mut description).hint_text("optional");
            ui.add(field.desired_width(FIELD));
            ui.end_row();
        });
        let name = name.trim_end().to_owned();
        let valid = presets::check_name(&name);
        let exists = valid.is_ok() && dir.join(format!("{name}.toml")).exists();
        let label = if exists { "Replace it" } else { "Save" };
        let button = ui.add_enabled(valid.is_ok(), egui::Button::new(label));
        let button = match &valid {
            Err(err) if !name.is_empty() => button.on_disabled_hover_text(err.to_string()),
            _ => button.on_disabled_hover_text("Name it first"),
        };
        if exists {
            ui.label(egui::RichText::new("A preset of this name is there already").weak());
        }
        let mut done = false;
        if button.clicked() {
            let (presets, config) = (context.presets, context.config);
            let choices = &item.choices;
            let webm = choices.webm_base(presets, config).unwrap_or_default().merged(&choices.webm);
            let tgs = choices.tgs_base(presets, config).unwrap_or_default().merged(&choices.tgs);
            match presets::save(&dir, &name, description.trim(), (&webm, &tgs), exists) {
                Ok(_) => {
                    take_up(item, &name);
                    ui.ctx().data_mut(|data| data.insert_temp(saved_id(), true));
                    done = true;
                }
                Err(err) => problem = Some(err.to_string()),
            }
        }
        if let Some(problem) = &problem {
            ui.colored_label(crate::widgets::BAD, problem);
        }
        if done {
            ui.data_mut(|data| data.remove::<(String, String, Option<String>)>(id));
            ui.close();
        } else {
            ui.data_mut(|data| data.insert_temp(id, (name, description, problem)));
        }
    });
}

/// The item made with the preset `name`, which now holds its values, but
/// for those of its file alone.
fn take_up(item: &mut Item, name: &str) {
    let choices = &mut item.choices;
    choices.preset = Some(name.to_owned());
    choices.webm = Options { title: choices.webm.title.take(), ..Options::default() };
    let tgs = &mut choices.tgs;
    *tgs = TgsOptions {
        title: tgs.title.take(),
        sheet: tgs.sheet.take(),
        sheet_frames: tgs.sheet_frames.take(),
        ..TgsOptions::default()
    };
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tgradish_core::config::Config;
    use tgradish_core::options::{Fit, Resize};
    use tgradish_core::presets::Format;

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
        harness.get_by_label("Save as preset…").click();
        harness.run_steps(2);
        let role = eframe::egui::accesskit::Role::TextInput;
        harness.query_all_by_role(role).next().unwrap().click();
        harness.run_steps(2);
        harness.query_all_by_role(role).next().unwrap().type_text("by quality");
        harness.run_steps(2);
        harness.get_by_label("Save").click();
        harness.run_steps(3);
        // the item takes it up, its title its own still
        let state = harness.state();
        let choices = &state.items[0].choices;
        assert_eq!(choices.preset.as_deref(), Some("by quality"));
        assert_eq!(
            (choices.webm.fit, choices.webm.title.as_deref()),
            (None, Some("only this one"))
        );
        // what it makes, and shows, comes from the preset now
        let effective = choices.webm_options(&state.presets, &state.config).unwrap();
        assert_eq!(super::super::resize_of(&effective), Resize::Crop);
        let saved = state.presets.webm("by quality").unwrap();
        assert_eq!((saved.fit, saved.title), (Some(Fit::Crf), None));
        assert!(dir.path().join("presets/by quality.toml").is_file());
    }
}

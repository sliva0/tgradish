//! The window's main scenes, drawn without a GPU, for tests that look over
//! all of them at once: that every button has a name screen readers can
//! say, and that no button's words are cut short.

use std::path::Path;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
use tgradish_core::presets::{Format, Presets};

use crate::app::App;
use crate::config::Config;
use crate::item::Show;
use crate::l10n::fl;
use crate::testing::{convert, finish, has_ffmpeg, square, video};
use crate::{inspect, preset_dialogs, settings_dialog};

/// A window as it opens, with the window's fonts and look, `width` wide.
fn window(config: Config, width: f32) -> Harness<'static, App> {
    Harness::builder().with_size([width, 820.0]).build_eframe(move |cc| {
        let app = App::with(config.clone(), None, Presets::builtin());
        app.style(&cc.egui_ctx);
        app
    })
}

/// Settles what was just asked of the window, the pointer over `at`.
fn settle(harness: &mut Harness<App>, at: Option<egui::Pos2>) {
    harness.event(at.map_or(egui::Event::PointerGone, egui::Event::PointerMoved));
    for item in &mut harness.state_mut().items {
        item.view.playing = false;
    }
    harness.run_steps(4);
}

/// Takes a window `width` wide through its main scenes, handing each to
/// `visit` by name: empty, with files, one converted and then changed, its
/// folded settings open, a video's frame picked, the menus, the dialogs,
/// and files removed. The video's need ffmpeg, and are left out without.
pub(crate) fn walk(width: f32, mut visit: impl FnMut(&mut Harness<App>, &str)) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    let mut harness = window(config, width);
    settle(&mut harness, None);
    visit(&mut harness, "empty");

    // pixel art, and frames of an animation, with its timeline
    let art = dir.path().join("pixel heart.png");
    square(&art, [220, 30, 60, 255]);
    let frames = dir.path().join("dance");
    std::fs::create_dir(&frames).unwrap();
    square(&frames.join("1.png"), [255, 0, 0, 255]);
    square(&frames.join("2.png"), [0, 0, 255, 255]);
    harness.state_mut().add(vec![art, frames]);
    finish(&mut harness);
    // the pointer over the first row, which shows its own buttons
    let row = first_row(&harness);
    settle(&mut harness, Some(row));
    visit(&mut harness, "files");
    harness.get_by_label(&fl!("add-more")).click();
    settle(&mut harness, None);
    visit(&mut harness, "add-menu");
    harness.key_press(egui::Key::Escape);
    settle(&mut harness, None);

    // made, with its result's small picture over the input
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    let item = harness.state_mut().items.last_mut().unwrap();
    (item.view.show, item.view.inset) = (Show::Input, true);
    settle(&mut harness, None);
    visit(&mut harness, "made");
    let item = harness.state_mut().items.last_mut().unwrap();
    item.view.show = Show::Result;
    settle(&mut harness, Some(row));
    visit(&mut harness, "result");
    // changed since, so made again from the list or the bar
    harness.state_mut().items.last_mut().unwrap().choices.tgs.keep_canvas = Some(true);
    settle(&mut harness, Some(row));
    visit(&mut harness, "outdated");
    folds(&mut harness);
    visit(&mut harness, "folds");

    let sticker = dir.path().join("out").join("dance.sticker.tgs");
    harness.state_mut().inspection = Some(inspect::Inspection::of(&sticker));
    settle(&mut harness, None);
    visit(&mut harness, "inspect");
    harness.state_mut().inspection = None;
    dialogs(&mut harness, dir.path(), &mut visit);

    if has_ffmpeg()
        && let Some(clip) = video(dir.path(), "640x360")
    {
        harness.state_mut().config.ffmpeg.choice = tgradish_core::ffmpeg::FfmpegChoice::System;
        harness.state_mut().add(vec![clip]);
        finish(&mut harness);
        settle(&mut harness, None);
        visit(&mut harness, "video");
        folds(&mut harness);
        if let Some(add) = harness.query_by_label(&fl!("add-option")) {
            add.click_accesskit();
        }
        settle(&mut harness, None);
        visit(&mut harness, "video-folds");
        // a frame of it, picked on the bar
        harness.state_mut().items.last_mut().unwrap().format = Format::Webp;
        finish(&mut harness);
        settle(&mut harness, None);
        visit(&mut harness, "picker");
    }

    // all removed, with a way back
    let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
    harness.state_mut().remove_all(&ids);
    settle(&mut harness, None);
    visit(&mut harness, "removed");
}

/// The middle of the first row of the list of files.
fn first_row(harness: &Harness<App>) -> egui::Pos2 {
    let name = harness.state().items[0].name();
    let row = harness.get_by(|node| node.label().as_deref() == Some(name.as_str()));
    let rect = row.accesskit_node().bounding_box().expect("drawn");
    egui::pos2(((rect.x0 + rect.x1) / 2.0) as f32, ((rect.y0 + rect.y1) / 2.0) as f32)
}

/// Opens the folded settings of the selected file.
fn folds(harness: &mut Harness<App>) {
    for fold in [fl!("commands"), fl!("encoder-options")] {
        if let Some(fold) = harness.query_by_label(&fold) {
            fold.click_accesskit();
            harness.run_steps(2);
        }
    }
    settle(harness, None);
}

/// Settings, About, and the dialogs of presets of the user's own, kept in
/// `dir`.
fn dialogs(
    harness: &mut Harness<App>,
    dir: &Path,
    visit: &mut impl FnMut(&mut Harness<App>, &str),
) {
    harness.state_mut().settings_dialog.open();
    settle(harness, None);
    visit(harness, "settings");
    harness.state_mut().settings_dialog = settings_dialog::SettingsDialog::default();
    harness.state_mut().settings_dialog.about = true;
    settle(harness, None);
    visit(harness, "about");
    harness.state_mut().settings_dialog.about = false;
    let presets_dir = dir.join("presets");
    let options = (&Default::default(), &Default::default());
    tgradish_core::presets::save(&presets_dir, "my stickers", "Mine", options, false).unwrap();
    let state = harness.state_mut();
    state.presets = Presets::load(Some(&presets_dir)).unwrap();
    state.presets_dir = Some(presets_dir);
    settle(harness, None);
    visit(harness, "presets");
    preset_dialogs::ask(&harness.ctx, preset_dialogs::Ask::Manage);
    settle(harness, None);
    visit(harness, "manage-presets");
    harness.state_mut().preset_dialog = None;
    let id = harness.state().items[0].id;
    preset_dialogs::ask(&harness.ctx, preset_dialogs::Ask::Save(id));
    settle(harness, None);
    visit(harness, "save-preset");
    harness.state_mut().preset_dialog = None;
    settle(harness, None);
}

/// The roles of what can be pressed or picked.
fn pressable(role: egui::accesskit::Role) -> bool {
    use egui::accesskit::Role;
    matches!(role, Role::Button | Role::CheckBox | Role::RadioButton | Role::Link)
}

#[test]
fn names_every_button_for_screen_readers() {
    let mut nameless = Vec::new();
    walk(1280.0, |harness, scene| {
        for node in harness.query_all(by()) {
            let node = node.accesskit_node();
            if !pressable(node.role()) {
                continue;
            }
            // a symbol alone, such as ✖, says nothing
            let label = node.label().unwrap_or_default();
            if !label.chars().any(char::is_alphanumeric) {
                nameless.push(format!("{scene}: {:?} {label:?}", node.role()));
            }
        }
    });
    assert!(nameless.is_empty(), "nameless:\n{}", nameless.join("\n"));
}

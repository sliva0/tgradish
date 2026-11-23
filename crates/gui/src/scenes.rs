//! Headless traversal of the GUI's main scenes for layout and accessibility
//! tests: verifies that every interactive control has a screen-reader label,
//! no two spin buttons in a row share a label, no English button text is
//! truncated, and the file list header/footer align with the main panel's bars.

use std::path::Path;

use eframe::egui::{self, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
use tgradish_core::presets::{Format, Presets};

use crate::app::App;
use crate::config::Config;
use crate::item::Show;
use crate::l10n::fl;
use crate::testing::{finish, has_ffmpeg, square, video};
use crate::{inspect, preset_dialogs, settings_dialog};

/// The GUI as it opens, with its fonts and look, in a window of `size`.
fn window(config: Config, size: egui::Vec2) -> Harness<'static, App> {
    Harness::builder().with_size(size).build_eframe(move |cc| {
        let app = App::with(config.clone(), None, Presets::builtin());
        app.style(&cc.egui_ctx);
        app
    })
}

/// Settles what was just asked of the GUI, the pointer over `at`.
fn settle(harness: &mut Harness<App>, at: Option<egui::Pos2>) {
    harness.event(at.map_or(egui::Event::PointerGone, egui::Event::PointerMoved));
    for item in &mut harness.state_mut().items {
        item.view.playing = false;
    }
    harness.run_steps(4);
}

/// Takes the GUI, in a window `width` wide, through its main scenes,
/// handing each to `visit` by name: empty, with files, one converted and
/// then changed, its folded settings open, a picture read as a sprite
/// sheet, a video's frame picked, the menus, the dialogs, and files
/// removed. The video's need ffmpeg, and are left out without.
pub(crate) fn walk(width: f32, visit: impl FnMut(&mut Harness<App>, &str)) {
    walk_in(egui::vec2(width, 820.0), visit);
}

/// As [`walk`], in a window of `size`.
fn walk_in(size: egui::Vec2, mut visit: impl FnMut(&mut Harness<App>, &str)) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    let mut harness = window(config, size);
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
    harness.get_by_label(&fl!("main-menu")).click();
    settle(&mut harness, None);
    visit(&mut harness, "main-menu");
    harness.key_press(egui::Key::Escape);
    settle(&mut harness, None);

    // made, with its result's small picture over the input; pressed as a
    // screen reader would, as in a narrow window it is past the edge
    let keys = crate::keys::keys_text(crate::keys::Command::Convert);
    harness.get(by().label_contains(&keys)).click_accesskit();
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
    // changed since, so made again from the list or the bar; as large as
    // a sticker gets, which says the most under its name
    let item = harness.state_mut().items.last_mut().unwrap();
    item.choices.tgs.keep_canvas = Some(true);
    if let Some(crate::jobs::Status::Done(done)) =
        item.made.as_mut().map(|made| &mut made.job.status)
    {
        done.bytes = 255 * 1024 + 900;
    }
    settle(&mut harness, Some(row));
    visit(&mut harness, "outdated");
    folds(&mut harness);
    visit(&mut harness, "folds");
    // the picture as a sticker, read as a sprite sheet, then as it was
    let (id, selected) = (harness.state().items[0].id, harness.state().selected);
    harness.state_mut().select(id);
    let item = &mut harness.state_mut().items[0];
    let was = (item.format, item.choices.tgs.sheet);
    item.format = Format::Tgs;
    item.choices.tgs.sheet = Some(tgradish_core::options::Sheet { columns: 2, rows: 2 });
    finish(&mut harness);
    settle(&mut harness, None);
    visit(&mut harness, "sheet");
    let item = &mut harness.state_mut().items[0];
    (item.format, item.choices.tgs.sheet) = was;
    if let Some(selected) = selected {
        harness.state_mut().select(selected);
    }
    finish(&mut harness);
    settle(&mut harness, None);

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
        // as it converts, Keep best so far beside Stop, as long as it gets
        let id = harness.state().items.last().unwrap().id;
        harness.state_mut().enqueue(id);
        harness.state_mut().queue.retain(|&other| other != id);
        let item = harness.state_mut().items.last_mut().unwrap();
        let job = &mut item.job.as_mut().expect("queued").job;
        (job.status, job.settles) = (crate::jobs::Status::Running, true);
        settle(&mut harness, None);
        assert!(harness.query_by_label(&fl!("keep-best")).is_some(), "no Keep best so far");
        visit(&mut harness, "converting");
        harness.state_mut().items.last_mut().unwrap().job = None;
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
    harness.state_mut().about = true;
    settle(harness, None);
    visit(harness, "about");
    harness.state_mut().about = false;
    harness.state_mut().shortcuts = true;
    settle(harness, None);
    visit(harness, "shortcuts");
    harness.state_mut().shortcuts = false;
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

/// Each text the GUI drew last, with where, and whether it was cut
/// short.
fn texts(harness: &Harness<App>) -> Vec<(String, Rect, bool)> {
    fn walk(shape: &egui::Shape, found: &mut Vec<(String, Rect, bool)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
            egui::Shape::Text(text) => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                found.push((text.galley.text().to_owned(), rect, text.galley.elided));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &harness.output().shapes {
        walk(&shape.shape, &mut found);
    }
    found
}

/// The roles of what can be pressed or picked.
fn pressable(role: egui::accesskit::Role) -> bool {
    use egui::accesskit::Role;
    matches!(role, Role::Button | Role::CheckBox | Role::RadioButton | Role::Link)
}

/// The roles of what screen readers say by its name: what can be pressed
/// or picked, number fields, and how far something goes.
fn named(role: egui::accesskit::Role) -> bool {
    use egui::accesskit::Role;
    pressable(role) || matches!(role, Role::SpinButton | Role::ProgressIndicator)
}

#[test]
fn names_every_button_for_screen_readers() {
    let mut nameless = Vec::new();
    let mut alike = Vec::new();
    walk(1280.0, |harness, scene| {
        // each number field's name, and the containers it is in, innermost
        // first
        let mut fields = Vec::new();
        for found in harness.query_all(by()) {
            let node = found.accesskit_node();
            if !named(node.role()) {
                continue;
            }
            // a symbol alone, such as ✖, says nothing
            let label = node.label().unwrap_or_default();
            if !label.chars().any(char::is_alphanumeric) {
                nameless.push(format!("{scene}: {:?} {label:?}", node.role()));
            }
            if node.role() == egui::accesskit::Role::SpinButton {
                let containers: Vec<_> = std::iter::successors(found.parent(), |up| up.parent())
                    .map(|up| up.accesskit_node().id())
                    .collect();
                fields.push((label, containers));
            }
        }
        // a row's fields, named alike, can't be told apart
        for (at, (label, containers)) in fields.iter().enumerate() {
            let of_one_row =
                |(other, theirs): &(String, Vec<_>)| other == label && one_row(containers, theirs);
            if fields[at + 1..].iter().any(of_one_row) {
                alike.push(format!("{scene}: {label:?}"));
            }
        }
    });
    assert!(nameless.is_empty(), "nameless:\n{}", nameless.join("\n"));
    assert!(alike.is_empty(), "named alike in one row:\n{}", alike.join("\n"));
}

/// How far up from two fields of one row the container they share is, at
/// the most: from those of different rows, or sections, it is further.
const ONE_ROW: usize = 3;

/// Whether what is in the containers `a`, and what is in `b`, innermost
/// first, are of one row: within [`ONE_ROW`] of the container they share.
fn one_row<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    let shared = a.iter().position(|container| b.contains(container));
    shared.is_some_and(|up| up < ONE_ROW && b.iter().position(|c| *c == a[up]) < Some(ONE_ROW))
}

/// Every dialog fits a window as short as the GUI goes, and one shorter
/// still, as a larger scale makes it: what closes it in sight, its frame
/// within the window side to side, what it holds within the frame's
/// margins, and no bar's words cut short. Inspect, About and Save preset
/// fit so a window as narrow as the narrowest at twice the scale, and at
/// three times; Settings, Keys and Manage presets have rows that don't
/// wrap, and are left out at those widths.
#[test]
fn keeps_what_closes_each_dialog_in_a_short_window() {
    // each dialog's scene, and the id of its area
    let all = [
        ("inspect", "inspect"),
        ("settings", "settings"),
        ("about", "about"),
        ("shortcuts", "shortcuts"),
        ("manage-presets", "manage presets"),
        ("save-preset", "save preset"),
    ];
    let narrow = [all[0], all[2], all[5]];
    let sizes = [
        (egui::vec2(crate::MIN_WIDTH, crate::MIN_HEIGHT), &all[..]),
        (egui::vec2(crate::MIN_WIDTH, crate::MIN_HEIGHT / 1.5), &all[..]),
        (egui::vec2(crate::MIN_WIDTH / 2.0, crate::MIN_HEIGHT), &narrow[..]),
        (egui::vec2(crate::MIN_WIDTH / 3.0, crate::MIN_HEIGHT), &narrow[..]),
    ];
    let mut seen = Vec::new();
    let mut hidden = Vec::new();
    for (size, dialogs) in sizes {
        walk_in(size, |harness, scene| {
            let Some(&(_, id)) = dialogs.iter().find(|(name, _)| *name == scene) else { return };
            seen.push(scene.to_owned());
            let screen = harness.ctx.content_rect();
            let within = |from: f64, to: f64, range: egui::Rangef| {
                from >= f64::from(range.min) - 0.5 && to <= f64::from(range.max) + 0.5
            };
            let closes = [fl!("close"), fl!("cancel")];
            let mut closer = None;
            for button in harness.query_all(by().role(egui::accesskit::Role::Button)) {
                let node = button.accesskit_node();
                let Some(label) = node.label().filter(|label| closes.contains(label)) else {
                    continue;
                };
                let rect = node.bounding_box().expect("drawn");
                if !within(rect.x0, rect.x1, screen.x_range())
                    || !within(rect.y0, rect.y1, screen.y_range())
                {
                    hidden.push(format!("{size:?}, {scene}: {label} at {rect:?}"));
                }
                closer = Some(button);
            }
            let closer = closer.unwrap_or_else(|| panic!("{size:?}, {scene}: nothing closes it"));
            // the frame in the window, side to side
            let frame = harness.ctx.memory(|memory| memory.area_rect(egui::Id::new(id)));
            let frame = frame.expect("shown");
            if !within(frame.min.x.into(), frame.max.x.into(), screen.x_range()) {
                hidden.push(format!("{size:?}, {scene}: its frame at {frame:?}"));
            }
            // what it holds, the area's own under the window's, within the
            // frame's margins, and no bar's words cut short
            let up: Vec<_> = std::iter::successors(Some(closer), |node| node.parent()).collect();
            let area = up[up.len() - 2].accesskit_node().id();
            let inside = frame.shrink(f32::from(crate::widgets::DIALOG_MARGIN)).x_range();
            let texts = texts(harness);
            for node in harness.query_all(by()) {
                if !std::iter::successors(node.parent(), |up| up.parent())
                    .any(|up| up.accesskit_node().id() == area)
                {
                    continue;
                }
                let node = node.accesskit_node();
                let Some(rect) = node.bounding_box() else { continue };
                let label = node.label().unwrap_or_default();
                if !within(rect.x0, rect.x1, inside) {
                    hidden.push(format!(
                        "{size:?}, {scene}: {:?} {label:?} at {rect:?}",
                        node.role()
                    ));
                }
                if node.role() == egui::accesskit::Role::ProgressIndicator {
                    let rect = Rect::from_min_max(
                        egui::pos2(rect.x0 as f32, rect.y0 as f32),
                        egui::pos2(rect.x1 as f32, rect.y1 as f32),
                    );
                    let on = |(_, text, _): &&(String, Rect, bool)| rect.contains(text.center());
                    if texts.iter().filter(on).any(|(_, _, elided)| *elided) {
                        hidden.push(format!("{size:?}, {scene}: {label:?} cut short"));
                    }
                }
            }
        });
    }
    let expected: usize = sizes.iter().map(|(_, dialogs)| dialogs.len()).sum();
    assert_eq!(seen.len(), expected, "{seen:?}");
    assert!(hidden.is_empty(), "out of sight:\n{}", hidden.join("\n"));
}

#[test]
fn lays_the_lists_bars_level_with_those_beside_them() {
    // where each is, top and bottom, by its name
    let place = |harness: &Harness<App>, by: egui_kittest::kittest::By<'_>| {
        let node = harness.get(by.role(egui::accesskit::Role::Button));
        let rect = node.accesskit_node().bounding_box().expect("drawn");
        (rect.y0, rect.y1)
    };
    let mut seen = 0;
    for width in [crate::MIN_WIDTH, 1280.0] {
        walk(width, |harness, scene| {
            if scene != "files" {
                return;
            }
            seen += 1;
            // the buttons over the list and over the preview, edge to edge
            let (add, input) = (fl!("add-files"), fl!("show-input"));
            assert_eq!(place(harness, by().label(&add)), place(harness, by().label(&input)));
            // those under them about the same middle, Convert being taller
            let (remove, keys) =
                (fl!("remove-all"), crate::keys::keys_text(crate::keys::Command::Convert));
            let all = place(harness, by().label(&remove));
            let convert = place(harness, by().label_contains(&keys));
            assert_eq!(all.0 + all.1, convert.0 + convert.1, "{all:?} {convert:?}");
            // as far from the window's bottom as those over them are from
            // its top
            let screen = harness.ctx.content_rect();
            let over = place(harness, by().label(&input)).0 - f64::from(screen.top());
            let under = f64::from(screen.bottom()) - convert.1;
            assert_eq!(over, under, "{over} over the one, {under} under the other");
        });
    }
    assert_eq!(seen, 2);
}

/// In British English, no button's words are cut short, at the narrowest
/// the GUI goes or wider. The rule is English's alone: other languages'
/// words, often longer, may be cut short where there is little room.
#[test]
fn cuts_no_buttons_english_words_short() {
    crate::l10n::english();
    let mut cut = Vec::new();
    for width in [crate::MIN_WIDTH, 1280.0] {
        walk(width, |harness, scene| {
            // names of files, and where they are, may be cut short
            let names: Vec<String> = harness.state().items.iter().map(|item| item.name()).collect();
            let buttons: Vec<(Rect, String)> = harness
                .query_all(by())
                .filter(|node| pressable(node.accesskit_node().role()))
                .filter_map(|node| {
                    let node = node.accesskit_node();
                    let rect = node.bounding_box()?;
                    let rect = Rect::from_min_max(
                        egui::pos2(rect.x0 as f32, rect.y0 as f32),
                        egui::pos2(rect.x1 as f32, rect.y1 as f32),
                    );
                    Some((rect, node.label().unwrap_or_default()))
                })
                .collect();
            for (text, rect, elided) in texts(harness) {
                if !elided {
                    continue;
                }
                // on a file's row, its name alone, not what is said under it
                let on = buttons.iter().find(|(button, _)| button.contains(rect.center()));
                if let Some((_, label)) = on
                    && !(names.contains(label) && label.starts_with(text.trim_end_matches('…')))
                {
                    cut.push(format!("{width}, {scene}: {text:?} of {label:?}"));
                }
            }
        });
    }
    assert!(cut.is_empty(), "cut short:\n{}", cut.join("\n"));
}

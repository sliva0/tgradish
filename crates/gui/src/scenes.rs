//! Headless traversal of the GUI's main scenes for layout and accessibility
//! tests: verifies that every interactive control has a screen-reader label,
//! no two spin buttons in a row share a label, every language's words fit
//! where they are shown and every dialog within the window, and the file
//! list header/footer align with the main panel's bars.

use std::collections::{BTreeMap, HashMap};
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
    notices(&mut harness, dir.path());
    visit(&mut harness, "notices");
    harness.state_mut().banner = Default::default();
    settle(&mut harness, None);

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
    let was = (item.format, item.choices.sheet);
    item.format = Format::Tgs;
    item.choices.sheet = Some(tgradish_core::options::Sheet { columns: 2, rows: 2 });
    finish(&mut harness);
    settle(&mut harness, None);
    visit(&mut harness, "sheet");
    let item = &mut harness.state_mut().items[0];
    (item.format, item.choices.sheet) = was;
    if let Some(selected) = selected {
        harness.state_mut().select(selected);
    }
    finish(&mut harness);
    settle(&mut harness, None);

    let sticker = dir.path().join("out").join("dance.sticker.tgs");
    harness.state_mut().dialogs.inspection = Some(inspect::Inspection::of(&sticker));
    settle(&mut harness, None);
    visit(&mut harness, "inspect");
    harness.state_mut().dialogs.inspection = None;
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
        harness.state_mut().work.queue.retain(|&other| other != id);
        let item = harness.state_mut().items.last_mut().unwrap();
        let job = &mut item.job.as_mut().expect("queued").job;
        (job.status, job.settles) = (crate::jobs::Status::Running, true);
        job.progress = encodes();
        settle(&mut harness, None);
        assert!(harness.query_by_label(&fl!("keep-best")).is_some(), "no Keep best so far");
        visit(&mut harness, "converting");
        harness.state_mut().items.last_mut().unwrap().job = None;
        // made, its encodes listed beside the preview; quickly, as the
        // fast preset makes it
        harness.state_mut().items.last_mut().unwrap().choices.preset = Some("fast".into());
        settle(&mut harness, None);
        let keys = crate::keys::keys_text(crate::keys::Command::Convert);
        harness.get(by().label_contains(&keys)).click_accesskit();
        harness.run_steps(2);
        finish(&mut harness);
        settle(&mut harness, None);
        visit(&mut harness, "video-made");
        // a frame of it, picked on the bar
        harness.state_mut().items.last_mut().unwrap().format = Format::Webp;
        finish(&mut harness);
        settle(&mut harness, None);
        visit(&mut harness, "picker");
    }

    // an Aseprite file, its tags and layers among its settings
    let ase = dir.path().join("walker.ase");
    let walker = tgradish_corpus::sprite("walker_2x");
    let tags = [("idle", 0, 1), ("walk", 1, 7)];
    std::fs::write(&ase, tgradish_corpus::encode::aseprite(&walker, &tags)).unwrap();
    harness.state_mut().add(vec![ase]);
    finish(&mut harness);
    settle(&mut harness, None);
    visit(&mut harness, "aseprite");

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

/// Notices over the list, with all their links: a settings file that can't
/// be read, the files open last time not kept, and files gone since.
fn notices(harness: &mut Harness<App>, dir: &Path) {
    use crate::banner::Message;
    let old = dir.join("config.toml");
    std::fs::write(&old, "[ffmpeg]\nuse = \"auto\"\n").unwrap();
    let Err(tgradish_core::Error::Config { message, .. }) = Config::load(&old) else {
        panic!("an old settings file read")
    };
    let err = tgradish_core::Error::Config { path: old, message };
    let unreadable = crate::app::settings_unreadable(fl!("setting-unsaved", "unreadable"), &err);
    let unsaved = Message::warning(fl!("session-unsaved"))
        .file(dir.join("session.json"))
        .literal("Permission denied (os error 13)");
    for message in [unreadable, unsaved, Message::info(fl!("files-gone", count = 2))] {
        harness.state_mut().say(message);
    }
    settle(harness, None);
}

/// A video's encodes as fitting it goes: the first too big, the second
/// under way.
fn encodes() -> crate::jobs::Progress {
    use crate::jobs::{Attempt, Progress, WebmPlan};
    let params = |kbps| tgradish_core::webm::Params {
        fps: 30.0,
        length: 1.0,
        rate: tgradish_core::webm::Rate::Bitrate(kbps),
    };
    let attempt = |number, kbps, bytes| Attempt {
        number,
        params: params(kbps),
        bytes,
        fits: false,
        ssim: None,
    };
    Progress {
        webm: Some(WebmPlan {
            width: 512,
            height: 288,
            fps: 30.0,
            length: 1.0,
            attempts: 8,
            limit: 256 * 1024,
        }),
        attempts: vec![attempt(1, 2400.0, Some(300_000)), attempt(2, 1900.0, None)],
        fraction: Some(0.4),
        ..Progress::default()
    }
}

/// Paths as long as some computers have them, without a space to wrap
/// at: ffmpeg as Chocolatey installs it on Windows, and as one unpacks it
/// in a folder of one's own, an example folder for results, and where the
/// settings are kept.
const LONG_PATHS: [&str; 4] = [
    r"C:\ProgramData\Chocolatey\bin\ffmpeg.exe",
    "/home/someone/.local/share/long-directory-name/ffmpeg-8.0-full_build/bin/ffmpeg",
    r"C:\Users\someone-with-a-long-name\OneDrive\Pictures\Telegram-Stickers",
    "/home/someone-with-a-long-name/.config/tgradish-settings-kept-here/config.toml",
];

/// Settings, with the ffmpeg found on PATH and the example folder at
/// [`LONG_PATHS`] whatever this computer has, then with ffmpeg missing at
/// the longer one and the settings saved to a long path; About, and the
/// dialogs of presets of the user's own, kept in `dir`.
fn dialogs(
    harness: &mut Harness<App>,
    dir: &Path,
    visit: &mut impl FnMut(&mut Harness<App>, &str),
) {
    let [found, unpacked, folder, saved] = LONG_PATHS.map(Path::new);
    // the folder for results as long as a temporary one is on Windows,
    // longer than its field
    let long = dir.join("out").join("results-kept-in-a-folder-with-a-long-name");
    let output_dir = harness.state_mut().config.gui.output_dir.replace(long);
    let settings = &mut harness.state_mut().dialogs.settings;
    settings.open();
    settings.as_if_found(found, folder);
    settle(harness, None);
    visit(harness, "settings");
    // the results saved next to their files, which leaves the field of
    // the folder for them empty, showing the example
    harness.state_mut().config.gui.output_dir = None;
    let settings = &mut harness.state_mut().dialogs.settings;
    settings.open();
    settings.as_if_found(unpacked, folder);
    settings.as_if_missing(unpacked, saved);
    settle(harness, None);
    visit(harness, "settings-long-paths");
    harness.state_mut().dialogs.settings = settings_dialog::SettingsDialog::default();
    harness.state_mut().config.gui.output_dir = output_dir;
    harness.state_mut().dialogs.about = true;
    settle(harness, None);
    visit(harness, "about");
    harness.state_mut().dialogs.about = false;
    harness.state_mut().dialogs.shortcuts = true;
    settle(harness, None);
    visit(harness, "shortcuts");
    harness.state_mut().dialogs.shortcuts = false;
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
    harness.state_mut().dialogs.presets = None;
    let id = harness.state().items[0].id;
    preset_dialogs::ask(&harness.ctx, preset_dialogs::Ask::Save(id));
    settle(harness, None);
    visit(harness, "save-preset");
    harness.state_mut().dialogs.presets = None;
    settle(harness, None);
}

/// A text the GUI drew.
struct Drawn {
    /// What it says whole, as it was asked to be drawn.
    text: String,
    /// What it showed of that, its lines one after another, with the "…"
    /// it was cut short at.
    shown: String,
    rect: Rect,
    /// What it was drawn within: what lies outside is hidden.
    clip: Rect,
    /// Cut short by egui, with an ellipsis.
    elided: bool,
    /// Where each of its lines is.
    lines: Vec<Rect>,
    /// How wide it could go before wrapping.
    wrap: f32,
}

/// Each text the GUI drew last.
fn drawn(harness: &Harness<App>) -> Vec<Drawn> {
    fn walk(shape: &egui::Shape, clip: Rect, found: &mut Vec<Drawn>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, clip, found)),
            egui::Shape::Text(text) => {
                let galley = &text.galley;
                let at = text.pos.to_vec2();
                let shown: Vec<String> = galley.rows.iter().map(|row| row.text()).collect();
                found.push(Drawn {
                    text: galley.text().to_owned(),
                    shown: shown.join("\n"),
                    rect: galley.rect.translate(at),
                    clip,
                    elided: galley.elided,
                    lines: galley
                        .rows
                        .iter()
                        .map(|row| row.rect_without_leading_space().translate(at))
                        .collect(),
                    wrap: galley.job.wrap.max_width,
                });
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &harness.output().shapes {
        walk(&shape.shape, shape.clip_rect, &mut found);
    }
    found
}

/// Where what can be pressed or picked is, with its name.
fn pressables(harness: &Harness<App>) -> Vec<(Rect, String)> {
    harness
        .query_all(by())
        .filter(|node| pressable(node.accesskit_node().role()))
        .filter_map(|node| {
            let node = node.accesskit_node();
            Some((rect_of(node.bounding_box()?), node.label().unwrap_or_default()))
        })
        .collect()
}

fn rect_of(rect: egui::accesskit::Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.x0 as f32, rect.y0 as f32),
        egui::pos2(rect.x1 as f32, rect.y1 as f32),
    )
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

/// The window as the GUI usually opens, as narrow as it goes, and as
/// narrow but tall enough for all of the settings to show without
/// scrolling.
const USUAL: egui::Vec2 = egui::vec2(1280.0, 820.0);
const NARROW: egui::Vec2 = egui::vec2(crate::MIN_WIDTH, 820.0);
const TALL: egui::Vec2 = egui::vec2(crate::MIN_WIDTH, 3000.0);

/// Each dialog's scene, and the id of its area.
const DIALOGS: [(&str, &str); 7] = [
    ("inspect", "inspect"),
    ("settings", "settings"),
    ("settings-long-paths", "settings"),
    ("about", "about"),
    ("shortcuts", "shortcuts"),
    ("manage-presets", "manage presets"),
    ("save-preset", "save preset"),
];

/// The scales a dialog is held to the window at: scaled up to three times,
/// the window as small as the GUI goes is that much smaller.
const SCALES: [f32; 5] = [1.0, 1.5, 2.0, 2.5, 3.0];

/// Lines of a translation may wrap where English's don't, as a paragraph's
/// do, where they have room, as what a shortcut does has at least 200
/// points; in a column narrower than this, as a row's label, or on what is
/// pressed, a word on a line of its own looks broken.
const TIGHT: f32 = 180.0;

/// How near the window's edge what is pressed may be: its panels keep it
/// twice as far in.
const EDGE: f32 = 4.0;

/// Where the check of one language, as `TGRADISH_LANG` asks for it, writes
/// what it finds, run in a process of its own: the GUI's language is the
/// whole process's, which the other tests would see changed.
const FIT_REPORT: &str = "TGRADISH_FIT_REPORT";

/// Every language's words fit where the GUI shows them, at the narrowest
/// it goes and as it usually opens, in every scene [`walk`] goes through:
/// none cut short, cut off where it is drawn, drawn over another, or,
/// where English's go on one line, wrapped in a narrow column or on what
/// is pressed, and nothing pressed pushed to the window's edge. Every dialog also fits a window as small as the GUI goes at
/// each of [`SCALES`]: its frame within the window, what it holds within
/// the frame's margins side to side, its buttons and the cross that
/// closes it in sight, what doesn't fit scrolling, and no bar's words cut
/// short. Each language is checked in a process of its own, some at once;
/// `TGRADISH_LANG=de` checks German alone (with English, which it is held
/// to), and `TGRADISH_LANG=pseudo` longer, accented English.
#[test]
fn fits_every_language_into_the_window() {
    if let Some(report) = std::env::var_os(FIT_REPORT) {
        crate::l10n::choose(None);
        std::fs::write(report, fit().join("\n")).unwrap();
        return;
    }
    let english = i18n_embed::LanguageLoader::fallback_language(&*crate::l10n::LOADER).to_string();
    let languages: Vec<String> = match crate::l10n::variable() {
        Some(asked) if asked == english => vec![english.clone()],
        Some(asked) => vec![english.clone(), asked],
        None => crate::l10n::languages().iter().map(|(language, _)| language.to_string()).collect(),
    };
    let reports = check_apart(&languages);
    let mut failed = Vec::new();
    // what doesn't fit, by language and what is wrong, and where
    let mut unfit: BTreeMap<(&str, String), Vec<&str>> = BTreeMap::new();
    // the lines each message took, by language, then by where and its id
    let mut lines: BTreeMap<&str, BTreeMap<(&str, &str), Lines>> = BTreeMap::new();
    for (language, report) in languages.iter().zip(&reports) {
        let report = match report {
            Ok(report) => report,
            Err(output) => {
                failed.push(format!("{language}: the check didn't finish:\n{output}"));
                continue;
            }
        };
        for line in report.lines() {
            match line.split_once('\t') {
                Some(("unfit", said)) => {
                    let (at, wrong) = said.split_once('\t').unwrap_or(("", said));
                    unfit.entry((language, wrong.to_owned())).or_default().push(at);
                }
                Some(("lines", said)) => {
                    let Some((at, id, drawn)) = Lines::read(said) else { continue };
                    let had = lines.entry(language).or_default().entry((at, id)).or_insert(drawn);
                    if drawn.lines > had.lines {
                        *had = drawn;
                    }
                }
                _ => {}
            }
        }
    }
    let empty = BTreeMap::new();
    let english_lines = lines.get(english.as_str()).unwrap_or(&empty);
    for (language, drawn) in lines.iter().filter(|(language, _)| **language != english) {
        for (&(at, id), drawn) in drawn {
            let Some(theirs) = english_lines.get(&(at, id)) else { continue };
            if theirs.lines == 1 && drawn.lines > 1 && (drawn.pressed || drawn.wrap <= TIGHT) {
                let (text, lines, wrap) = (drawn.text, drawn.lines, drawn.wrap);
                let wrong = format!(
                    "{id} {text:?} goes on {lines} lines {wrap:.0} points wide, where \
                     English's goes on {}",
                    theirs.lines
                );
                unfit.entry((language, wrong)).or_default().push(at);
            }
        }
    }
    // each once, where it was found first
    for ((language, wrong), at) in unfit {
        let more = match at.len() {
            1 => String::new(),
            more => format!(" (and {} more)", more - 1),
        };
        failed.push(format!("{language}, {}{more}: {wrong}", at[0]));
    }
    assert!(failed.is_empty(), "{} don't fit:\n{}\n", failed.len(), failed.join("\n"));
}

/// How many lines a message took where it was drawn, how wide they could
/// go, whether it is on what is pressed, and what it said.
#[derive(Clone, Copy)]
struct Lines<'a> {
    lines: usize,
    wrap: f32,
    pressed: bool,
    text: &'a str,
}

impl<'a> Lines<'a> {
    /// The record of a message's lines `said` writes, as `fit` writes it,
    /// with where it was drawn and its id.
    fn read(said: &'a str) -> Option<(&'a str, &'a str, Lines<'a>)> {
        let mut parts = said.splitn(6, '\t');
        let (at, id) = (parts.next()?, parts.next()?);
        let lines = parts.next()?.parse().ok()?;
        let wrap = parts.next()?.parse().ok()?;
        let pressed = parts.next()? == "pressed";
        Some((at, id, Lines { lines, wrap, pressed, text: parts.next()? }))
    }
}

/// Runs the check of each of `languages` in a process of its own, some at
/// once, and gives what each found, or what it said where it failed.
fn check_apart(languages: &[String]) -> Vec<Result<String, String>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::tempdir().unwrap();
    let test = module_path!().split_once("::").map_or("", |(_, path)| path).to_owned()
        + "::fits_every_language_into_the_window";
    let exe = std::env::current_exe().unwrap();
    let next = AtomicUsize::new(0);
    let reports = std::sync::Mutex::new(vec![Err(String::new()); languages.len()]);
    // a walk is mostly one thread's work, much of it waiting on what it
    // converts, and other tests run meanwhile
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 2);
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, languages.len()) {
            scope.spawn(|| {
                while let Some(language) = languages.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let report = dir.path().join(language);
                    let output = std::process::Command::new(&exe)
                        .args(["--exact", &test, "--test-threads=1"])
                        .env(crate::l10n::VARIABLE, language)
                        .env(FIT_REPORT, &report)
                        .output()
                        .unwrap();
                    let found = match std::fs::read_to_string(&report) {
                        Ok(found) if output.status.success() => Ok(found),
                        // where it panicked and why, without all the GUI
                        // its harness may print
                        _ => {
                            let said = String::from_utf8_lossy(&output.stdout).into_owned()
                                + &String::from_utf8_lossy(&output.stderr);
                            let from = said.find("panicked at").unwrap_or(0);
                            Err(said[from..].lines().take(20).collect::<Vec<_>>().join("\n"))
                        }
                    };
                    let at = languages.iter().position(|had| had == language).unwrap();
                    reports.lock().unwrap()[at] = found;
                }
            });
        }
    });
    reports.into_inner().unwrap()
}

/// Walks the GUI in the language shown, at each scene checking its words
/// as it usually opens and at its narrowest, and the dialogs at each of
/// [`SCALES`]: what doesn't fit, each a line starting `unfit`, and the
/// lines each message took, each a line starting `lines`.
fn fit() -> Vec<String> {
    crate::l10n::record::remember();
    let mut found = Vec::new();
    let mut dialogs = 0;
    // what remembers its size from the frame before, as a grid's columns,
    // settles in the next
    let resize = |harness: &mut Harness<App>, size: egui::Vec2| {
        harness.set_size(size);
        harness.run_steps(3);
    };
    walk_in(USUAL, |harness, scene| {
        for size in [USUAL, NARROW, TALL] {
            // the walk shows each scene as it usually opens, settled
            if size != USUAL {
                resize(harness, size);
            }
            unfit(harness, &format!("{} by {}, {scene}", size.x, size.y), &mut found);
        }
        if let Some(&(_, id)) = DIALOGS.iter().find(|(name, _)| *name == scene) {
            dialogs += 1;
            for scale in SCALES {
                let size = egui::vec2(crate::MIN_WIDTH, crate::MIN_HEIGHT) / scale;
                resize(harness, size);
                let at = format!("{} by {} at {scale}× scale, {scene}", size.x, size.y);
                out_of_sight(harness, id, &at, &mut found);
            }
        }
        resize(harness, USUAL);
    });
    assert_eq!(dialogs, DIALOGS.len(), "not every dialog was shown");
    found
}

/// `text` with its spaces and line breaks each one space, as its message
/// is laid out.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Notes in `found` what of the words drawn last doesn't fit, `at` saying
/// where: cut short, cut off at the edge of what it is drawn within, or
/// drawn over other words, and what is pressed pushed to the window's
/// edge; and the lines each message drawn took. Names
/// of files may be cut short, as may what isn't a message, as numbers and
/// paths, but on what is pressed.
fn unfit(harness: &Harness<App>, at: &str, found: &mut Vec<String>) {
    let mut said: Vec<(&str, String)> = crate::l10n::record::since(0)
        .into_iter()
        .map(|(id, text)| (id, words(&text)))
        .filter(|(_, text)| !text.is_empty())
        .collect();
    said.sort_unstable();
    said.dedup();
    let whole: HashMap<&str, &str> = said.iter().map(|(id, text)| (text.as_str(), *id)).collect();
    // the message `text` is, else the one it starts, cut short, else the
    // longest it holds
    let message = |text: &str| -> Option<(&str, &str)> {
        let text = words(text);
        if let Some((text, id)) = whole.get_key_value(text.as_str()) {
            return Some((*id, *text));
        }
        let shown = text.split('…').next().unwrap_or_default().trim_end();
        let starts = said.iter().filter(|(_, said)| !shown.is_empty() && said.starts_with(shown));
        let held = said.iter().filter(|(_, said)| {
            said.chars().filter(|c| c.is_alphabetic()).count() >= 4 && text.contains(said.as_str())
        });
        let shortest = starts.min_by_key(|(_, said)| said.len());
        shortest
            .or_else(|| held.max_by_key(|(_, said)| said.len()))
            .map(|(id, said)| (*id, said.as_str()))
    };
    let mut names: Vec<String> = harness.state().items.iter().map(|item| item.name()).collect();
    // what is typed for the results' folder too: a field scrolls a path
    // longer than itself, which a temporary folder's is on some machines
    names.extend(harness.state().config.gui.output_dir.iter().map(|dir| dir.display().to_string()));
    let of_name = |text: &str| {
        let (first, last) = (text.split('…').next().unwrap_or_default(), text.rsplit('…').next());
        names
            .iter()
            .any(|name| name.starts_with(first) && last.is_none_or(|last| name.ends_with(last)))
    };
    let pressables = pressables(harness);
    let drawn: Vec<Drawn> = drawn(harness)
        .into_iter()
        // in sight, or out of it only to a side
        .filter(|drawn| {
            !drawn.text.trim().is_empty() && drawn.rect.y_range().intersects(drawn.clip.y_range())
        })
        .collect();
    for drawn in &drawn {
        let text = drawn.text.as_str();
        let pressed = pressables.iter().any(|(rect, _)| rect.contains(drawn.rect.center()));
        let clipped = drawn.rect.left() < drawn.clip.left() - 1.0
            || drawn.rect.right() > drawn.clip.right() + 1.0;
        if (drawn.elided || clipped) && !of_name(text) {
            let how = match drawn.elided {
                true => format!("cut short, shown as {:?}", words(&drawn.shown)),
                false => "cut off where it is drawn".to_owned(),
            };
            match message(text) {
                Some((id, said)) => found.push(format!("unfit\t{at}\t{id} {said:?} is {how}")),
                None if pressed => found.push(format!("unfit\t{at}\t{text:?} is {how}")),
                None => {}
            }
        }
        if let Some(id) = whole.get(words(text).as_str()) {
            let (lines, wrap, text) = (drawn.lines.len(), drawn.wrap, words(text));
            // what is pressed says it, as a button does its words
            let pressed = pressables
                .iter()
                .any(|(rect, label)| rect.contains(drawn.rect.center()) && words(label) == text);
            let pressed = if pressed { "pressed" } else { "-" };
            found.push(format!("lines\t{at}\t{id}\t{lines}\t{wrap}\t{pressed}\t{text}"));
        }
    }
    // words drawn over others, within the same part of the window
    let over = |one: &Drawn, other: &Drawn| {
        one.lines.iter().any(|line| {
            other.lines.iter().any(|other| {
                let over = line.shrink(1.0).intersect(other.shrink(1.0));
                over.is_positive() && over.intersects(one.clip)
            })
        })
    };
    for (index, one) in drawn.iter().enumerate() {
        for other in &drawn[index + 1..] {
            if one.clip == other.clip && over(one, other) {
                let name = |text: &str| match message(text) {
                    Some((id, said)) => format!("{id} {said:?}"),
                    None => format!("{text:?}"),
                };
                let (one, other) = (name(&one.text), name(&other.text));
                found.push(format!("unfit\t{at}\t{one} is drawn over {other}"));
            }
        }
    }
    // what is pressed within the window's margins, not pushed past them,
    // as a row too wide for its panel pushes what is in it
    let screen = harness.ctx.content_rect().shrink(EDGE);
    for (rect, label) in &pressables {
        if rect.left() < screen.left() || rect.right() > screen.right() {
            let label = words(label);
            let name = match whole.get(label.as_str()) {
                Some(id) => format!("{id} {label:?}"),
                None => format!("{label:?}"),
            };
            found.push(format!("unfit\t{at}\t{name} is pushed to the edge of the window"));
        }
    }
}

/// Notes in `found` what of the dialog shown, the one of the area `id`,
/// is out of sight, `at` saying where: its frame not within the window,
/// what it holds not within the frame's margins side to side, its buttons
/// or the cross that closes it not in sight, and a bar's words cut short.
fn out_of_sight(harness: &Harness<App>, id: &str, at: &str, found: &mut Vec<String>) {
    let buttons =
        [fl!("close"), fl!("cancel"), fl!("save"), fl!("reset-settings"), fl!("inspect-another")];
    let mut hidden = Vec::new();
    let screen = harness.ctx.content_rect();
    let within = |from: f64, to: f64, range: egui::Rangef| {
        from >= f64::from(range.min) - 0.5 && to <= f64::from(range.max) + 0.5
    };
    let mut closer = None;
    for button in harness.query_all(by().role(egui::accesskit::Role::Button)) {
        let node = button.accesskit_node();
        let Some(label) = node.label().filter(|label| buttons.contains(label)) else {
            continue;
        };
        let rect = node.bounding_box().expect("drawn");
        if !within(rect.x0, rect.x1, screen.x_range())
            || !within(rect.y0, rect.y1, screen.y_range())
        {
            hidden.push(format!("{label} at {rect:?} is out of sight"));
        }
        if [&buttons[0], &buttons[1]].contains(&&label) {
            closer = Some(button);
        }
    }
    let Some(closer) = closer else {
        found.push(format!("unfit\t{at}\tnothing closes it"));
        return;
    };
    // the frame in the window
    let frame = harness.ctx.memory(|memory| memory.area_rect(egui::Id::new(id)));
    let frame = frame.expect("shown");
    if !within(frame.min.x.into(), frame.max.x.into(), screen.x_range())
        || !within(frame.min.y.into(), frame.max.y.into(), screen.y_range())
    {
        hidden.push(format!("its frame at {frame:?} is out of sight"));
    }
    // what it holds, the area's own under the window's, within the frame's
    // margins, and no bar's words cut short
    let up: Vec<_> = std::iter::successors(Some(closer), |node| node.parent()).collect();
    let area = up[up.len() - 2].accesskit_node().id();
    let inside = frame.shrink(f32::from(crate::widgets::DIALOG_MARGIN)).x_range();
    let texts = drawn(harness);
    for node in harness.query_all(by()) {
        if !std::iter::successors(node.parent(), |up| up.parent())
            .any(|up| up.accesskit_node().id() == area)
        {
            continue;
        }
        let node = node.accesskit_node();
        let Some(rect) = node.bounding_box() else { continue };
        let label = node.label().unwrap_or_default();
        // a field's text runs on past it where it is longer, scrolled
        // within it, which is what is held to the frame
        let run = node.role() == egui::accesskit::Role::TextRun;
        if !run && !within(rect.x0, rect.x1, inside) {
            hidden.push(format!("{:?} {label:?} at {rect:?} is out of sight", node.role()));
        }
        if node.role() == egui::accesskit::Role::ProgressIndicator {
            let rect = rect_of(rect);
            if texts.iter().any(|text| rect.contains(text.rect.center()) && text.elided) {
                hidden.push(format!("{label:?} is cut short"));
            }
        }
    }
    found.extend(hidden.into_iter().map(|hidden| format!("unfit\t{at}\t{hidden}")));
}

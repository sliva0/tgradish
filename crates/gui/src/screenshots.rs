//! Pictures of the window, for a look at its layout and for the README.
//! Ignored unless asked for, as they need a GPU wgpu can use and somewhere
//! to put them. This makes both:
//! `TGRADISH_SCREENSHOTS=/tmp/x cargo test -p tgradish-gui -- --ignored screenshots`
//! With `TGRADISH_LANG` they are in that language, as `pseudo` for English
//! made longer, to see where longer words would not fit.

use std::path::PathBuf;
use std::time::Duration;

use crate::config::{Config, ResultPreview};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{Crop, Fit, Scaling};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::tgs;
use tgradish_core::webm::Options;

use crate::app::{App, set_theme};
use crate::item::{self, Show, Zoom};
use crate::jobs::{self, Job, Status};
use crate::testing::{convert, finish, square, video};
use crate::{inspect, preset_dialogs, settings_dialog};

/// Renders the window to `$TGRADISH_SCREENSHOTS/*.png` for a look at
/// the layout: `TGRADISH_SCREENSHOTS=/tmp cargo test -p tgradish-gui
/// screenshots -- --ignored`. Needs a GPU wgpu can use, and ffmpeg.
#[test]
#[ignore]
fn screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    let mut save = |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    };
    walk(&mut save);
    edges(&mut save);
    accents(&mut save);
}

/// Only the pictures of [`accents`], to `$TGRADISH_SCREENSHOTS` as
/// [`screenshots`] makes them.
#[test]
#[ignore]
fn accent_screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    accents(&mut |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    });
}

/// Takes the window through each state worth a picture, handing each to
/// `save` by name. Needs a GPU wgpu can use, and ffmpeg.
pub(crate) fn walk(mut save: impl FnMut(&mut Harness<App>, &str)) {
    let dir = tempfile::tempdir().unwrap();
    let frames = dir.path().join("dance");
    std::fs::create_dir(&frames).unwrap();
    square(&frames.join("1.png"), [255, 0, 0, 255]);
    square(&frames.join("2.png"), [0, 0, 255, 255]);
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    // filtering as the window does it, so pixel art stays sharp
    let filtering = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    harness.run_steps(3);
    save(&mut harness, "empty");
    banners(&mut harness, &mut save, dir.path());
    if let Some(video) = video(dir.path(), "640x360") {
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        harness.state_mut().items[0].choices.crop =
            Some(Crop { x: 120, y: 60, width: 320, height: 200 });
        harness.state_mut().items[0].view.playing = false;
        harness.run_steps(3);
        save(&mut harness, "video");
        // the menus of adding files and of the folders
        harness.get_by_label("⏷").click();
        harness.run_steps(2);
        save(&mut harness, "add-menu");
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        // its short name where longer words leave no room for the long one
        let (full, short) =
            (crate::l10n::fl!("show-folder"), crate::l10n::fl!("show-folder", "short"));
        match harness.query_by_label(&full) {
            Some(folder) => folder.click(),
            None => harness.get_by_label(&short).click(),
        }
        harness.run_steps(2);
        save(&mut harness, "folder-menu");
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        // made, its encodes listed beside the preview
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        harness.state_mut().items[0].view.playing = false;
        harness.set_size(egui::vec2(1280.0, 1600.0));
        harness.run_steps(3);
        save(&mut harness, "video-made");
        harness.set_size(egui::vec2(1280.0, 820.0));
        // every setting of a picture and its time, on a tall window: a
        // typed shape, a fixed integer scale, and a faster tempo
        let item = &mut harness.state_mut().items[0];
        item.view.aspect = item::Aspect::Ratio(4.0, 3.0);
        item.choices.webm.scaling = Some(Scaling::PixelPerfect);
        item.choices.webm.exact_scale = Some("1/2".parse().unwrap());
        item.choices.webm.tempo = Some(2.0);
        harness.set_size(egui::vec2(1280.0, 1500.0));
        harness.run_steps(3);
        save(&mut harness, "settings-webm");
        // lossless: the ways of fitting it rules out greyed, yet live
        let webm = &mut harness.state_mut().items[0].choices.webm;
        (webm.lossless, webm.fit) = (Some(true), Some(Fit::Fps));
        harness.set_size(egui::vec2(1280.0, 2800.0));
        harness.run_steps(3);
        save(&mut harness, "settings-lossless");
        harness.set_size(egui::vec2(1280.0, 820.0));
        let item = &mut harness.state_mut().items[0];
        (item.view.aspect, item.choices.webm) = (item::Aspect::Free, Default::default());
        // a frame of it as an image sticker, picked on the bar
        (item.format, item.choices.frame) = (Format::Webp, Some(0.5));
        harness.run_steps(3);
        for _ in 0..100 {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        save(&mut harness, "image");
        // the last frame, lit to the end of the bar
        let item = &mut harness.state_mut().items[0];
        item.choices.frame = item.input_frames().map(|frames| frames.times[frames.count() - 1]);
        harness.run_steps(3);
        save(&mut harness, "image-last");
        // a video made an animation, which it doesn't suit
        harness.state_mut().items[0].format = Format::Tgs;
        finish(&mut harness);
        harness.run_steps(3);
        save(&mut harness, "tgs-not-art");
        harness.state_mut().items[0].format = Format::Webm;
        harness.run_steps(3);
        // a preset of your own: its button, and the dialogs that save
        // and manage them
        let presets_dir = dir.path().join("presets");
        let quick = Options { fit: Some(Fit::Bitrate), ..Options::default() };
        let mine = (&quick, &tgs::Options::default());
        tgradish_core::presets::save(&presets_dir, "my stickers", "Quick", mine, false).unwrap();
        let state = harness.state_mut();
        state.presets = Presets::load(Some(&presets_dir)).unwrap();
        state.presets_dir = Some(presets_dir);
        state.items[0].choices.preset = Some("my stickers".into());
        harness.run_steps(3);
        save(&mut harness, "presets");
        preset_dialogs::ask(&harness.ctx, preset_dialogs::Ask::Manage);
        harness.run_steps(3);
        save(&mut harness, "manage-presets");
        harness.state_mut().preset_dialog = None;
        let id = harness.state().items[0].id;
        preset_dialogs::ask(&harness.ctx, preset_dialogs::Ask::Save(id));
        harness.run_steps(3);
        save(&mut harness, "save-preset");
        harness.state_mut().preset_dialog = None;
        harness.state_mut().items[0].choices.preset = None;
        harness.run_steps(3);
    }
    // names in scripts the window's fonts lack, drawn with the system's
    // with characters no font has, an unassigned one and a newer
    // emoji, and a zero width joiner
    let named = dir.path().join("日本語 العربية ภาษาไทย \u{2fe0}\u{200d}\u{1faf9}.png");
    square(&named, [0, 160, 0, 255]);
    harness.state_mut().add(vec![named]);
    finish(&mut harness);
    for _ in 0..200 {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.state_mut().add(vec![frames]);
    finish(&mut harness);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    harness.state_mut().items.last_mut().unwrap().view.playing = false;
    harness.run_steps(3);
    save(&mut harness, "tgs");
    harness.state_mut().items.last_mut().unwrap().view.show = Show::Input;
    harness.run_steps(3);
    save(&mut harness, "tgs-input");
    harness.state_mut().settings_dialog.open();
    harness.run_steps(3);
    save(&mut harness, "settings");
    harness.state_mut().settings_dialog = settings_dialog::SettingsDialog::default();
    let sticker = dir.path().join("out").join("dance.sticker.tgs");
    harness.state_mut().inspection = Some(inspect::Inspection::of(&sticker));
    harness.run_steps(3);
    save(&mut harness, "inspect");
    harness.state_mut().inspection = None;
    // a .tgs halfway through fitting
    let mut job = Job::waiting(dir.path().join("progress.tgs"));
    job.status = Status::Running;
    job.progress = jobs::Progress {
        tgs: Some(jobs::TgsStage::Fitting),
        unchanged_bytes: Some(200_000),
        steps: vec![tgradish_core::tgradish_tgs::sticker::Step::new(
            tgradish_core::tgradish_tgs::reduce::Reduction::MergeColours { distance: 0.02 },
            110_000,
            0.01,
        )],
        ..jobs::Progress::default()
    };
    let item = harness.state_mut().items.last_mut().unwrap();
    item.job = Some(crate::item::Conversion::of(job, Format::Tgs, item.choices.clone()));
    harness.run_steps(3);
    save(&mut harness, "progress");
    harness.state_mut().settings_dialog.about = true;
    harness.run_steps(3);
    save(&mut harness, "about");
    harness.state_mut().settings_dialog.about = false;
    set_theme(&harness.ctx, crate::config::Theme::Light);
    let item = harness.state_mut().items.last_mut().unwrap();
    item.job = None;
    item.view.inset = true;
    harness.run_steps(3);
    save(&mut harness, "light");
    // zoomed in, the input grows over the result's small picture's
    // strip; then it larger, in another corner
    let item = harness.state_mut().items.last_mut().unwrap();
    item.view.input_zoom.level = 1.6;
    harness.run_steps(3);
    save(&mut harness, "zoomed");
    harness.state_mut().config.gui.result_preview =
        ResultPreview { size: 5, corner: crate::config::Corner::TopLeft };
    harness.step();
    std::thread::sleep(Duration::from_millis(300));
    harness.run_steps(3);
    save(&mut harness, "large-preview");
    harness.state_mut().items.last_mut().unwrap().view.input_zoom = Zoom::default();
    // an Aseprite file's tag and layers, its sketch shown, as a video
    // with groups of layers, one hidden, as artists keep them
    use tgradish_corpus::encode::Layer;
    let layer = |name, group, level, visible| Layer { name, group, level, visible };
    let layers = [
        layer("Body", true, 0, true),
        layer("shading", false, 1, true),
        layer("outline", false, 1, true),
        layer("Effects", true, 0, false),
        layer("sparkles", false, 1, true),
        layer("sketch", false, 0, false),
    ];
    let walker = tgradish_corpus::sprite("walker_2x");
    let tags = [("idle", 0, 1), ("wave", 1, 3), ("walk", 2, 7)];
    let bytes = tgradish_corpus::encode::aseprite_with(&walker, &tags, &layers);
    let ase = dir.path().join("walker.ase");
    std::fs::write(&ase, bytes).unwrap();
    harness.state_mut().add(vec![ase]);
    finish(&mut harness);
    let item = harness.state_mut().items.last_mut().unwrap();
    item.choices.show_layers = Some(vec!["sketch".into()]);
    item.format = Format::Webm;
    finish(&mut harness);
    harness.state_mut().items.last_mut().unwrap().view.playing = false;
    harness.run_steps(3);
    save(&mut harness, "aseprite");
    // full screen, and all of its settings, as a video and as a .tgs
    harness.set_size(egui::vec2(1920.0, 1080.0));
    harness.run_steps(3);
    save(&mut harness, "wide");
    harness.set_size(egui::vec2(1280.0, 1900.0));
    harness.run_steps(3);
    save(&mut harness, "settings-aseprite");
    harness.state_mut().items.last_mut().unwrap().format = Format::Tgs;
    finish(&mut harness);
    harness.run_steps(3);
    save(&mut harness, "settings-tgs");
    harness.set_size(egui::vec2(1280.0, 820.0));
    // cut square: what is cut off greyed out
    let item = harness.state_mut().items.last_mut().unwrap();
    item.choices.tgs.resize = Some(tgradish_core::options::Resize::Crop);
    item.choices.show_layers = None;
    item.view.show = Show::Input;
    finish(&mut harness);
    harness.run_steps(3);
    save(&mut harness, "tgs-crop");
    let last = harness.state().items.len() - 1;
    harness.state_mut().items.remove(last);
    let shown = harness.state().items.last().unwrap().id;
    harness.state_mut().select(shown);
    harness.run_steps(3);
    // the narrowest list, its buttons leaving no room for the name
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 640.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    harness.run_steps(3);
    save(&mut harness, "narrow");
    // a WebM on its second encode, there and at the usual size
    let mut job = Job::waiting(dir.path().join("progress.webm"));
    job.status = Status::Running;
    let params = |kbps| tgradish_core::webm::Params {
        fps: 30.0,
        length: 1.0,
        rate: tgradish_core::webm::Rate::Bitrate(kbps),
    };
    job.progress = jobs::Progress {
        webm: Some(jobs::WebmPlan {
            width: 512,
            height: 288,
            fps: 30.0,
            length: 1.0,
            attempts: 8,
            limit: 256 * 1024,
        }),
        attempts: vec![
            jobs::Attempt {
                number: 1,
                params: params(2400.0),
                bytes: Some(300_000),
                fits: false,
                ssim: None,
            },
            jobs::Attempt {
                number: 2,
                params: params(1900.0),
                bytes: None,
                fits: false,
                ssim: None,
            },
        ],
        fraction: Some(0.4),
        ..jobs::Progress::default()
    };
    let item = harness.state_mut().items.last_mut().unwrap();
    item.job = Some(crate::item::Conversion::of(job, Format::Webm, item.choices.clone()));
    harness.run_steps(3);
    save(&mut harness, "narrow-converting");
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.run_steps(3);
    save(&mut harness, "converting");
    // a fractional scale, where the icon keeps to whole screen pixels
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.25)
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(|cc| {
            let app = App::with(Config::default(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    harness.run_steps(3);
    save(&mut harness, "scaled");
}

/// The window where it is most likely to break: a result in a short
/// window, the keyboard's focus, a result out of date, a conversion that
/// failed, at the usual width and the narrowest, a long name, many presets
/// of the user's own, and every section open. Needs ffmpeg.
fn edges(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    use crate::config::Theme;
    use crate::l10n::fl;
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let filtering = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    // a video under `name`, each made in a folder of its own
    let named = |name: &str| {
        let made = dir.path().join(format!("made {name}"));
        std::fs::create_dir_all(&made).unwrap();
        let named = dir.path().join(name);
        std::fs::rename(video(&made, "640x360")?, &named).unwrap();
        Some(named)
    };
    let shot = |harness: &mut Harness<App>, save: &mut dyn FnMut(&mut Harness<App>, &str), name| {
        // the pointer away from what it would hover
        harness.event(egui::Event::PointerGone);
        harness.run_steps(3);
        save(harness, name);
    };
    let pause = |harness: &mut Harness<App>| {
        for item in &mut harness.state_mut().items {
            item.view.playing = false;
        }
        harness.run_steps(3);
    };
    let (Some(clip), Some(other)) = (named("clip.mp4"), named("cat jumps.mp4")) else { return };
    let art = dir.path().join("pixel heart.png");
    square(&art, [220, 30, 60, 255]);
    harness.state_mut().add(vec![clip, other, art]);
    finish(&mut harness);
    let first = harness.state().items[0].id;
    harness.state_mut().select(first);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    pause(&mut harness);
    shot(&mut harness, save, "edge-made");
    harness.get_by_label("☀").focus();
    shot(&mut harness, save, "edge-focus");
    harness.ctx.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    set_theme(&harness.ctx, Theme::Light);
    shot(&mut harness, save, "edge-made-light");
    set_theme(&harness.ctx, Theme::Dark);
    // every section open, on a tall window
    harness.set_size(egui::vec2(1280.0, 2400.0));
    harness.run_steps(3);
    for label in [fl!("encoder-options"), fl!("commands")] {
        if let Some(node) = harness.query_by_label(&label) {
            node.click();
            harness.run_steps(3);
        }
    }
    shot(&mut harness, save, "edge-all-open");
    harness.set_size(egui::vec2(1280.0, 820.0));
    // out of date: played faster since
    harness.state_mut().items[0].choices.webm.tempo = Some(2.0);
    shot(&mut harness, save, "edge-outdated");
    // a crop outside the picture, which fails
    harness.state_mut().items[0].choices.crop =
        Some(Crop { x: 700, y: 400, width: 40, height: 40 });
    harness.run_steps(2);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    shot(&mut harness, save, "edge-failed");
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "edge-narrow-failed");
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.run_steps(3);
    // a long name, made
    let long = "holiday at the seaside with the whole family, the long cut, final edit (2).mp4";
    let Some(long) = named(long) else { return };
    harness.state_mut().add(vec![long]);
    finish(&mut harness);
    let last = harness.state().items.last().unwrap().id;
    harness.state_mut().select(last);
    harness.run_steps(2);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    pause(&mut harness);
    shot(&mut harness, save, "edge-long-name");
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "edge-narrow-long-name");
    harness.set_size(egui::vec2(1280.0, 820.0));
    // five presets of the user's own, one with a long name, and the dialog
    // that manages them, in the light theme
    set_theme(&harness.ctx, Theme::Light);
    let presets_dir = dir.path().join("presets");
    let quick = Options { fit: Some(Fit::Bitrate), ..Options::default() };
    let mine = (&quick, &tgs::Options::default());
    let long = "my stickers for the whole family summer holiday";
    for name in ["my stickers", "pixel art crisp", "long loops", "emoji pack", long] {
        tgradish_core::presets::save(&presets_dir, name, "Quick", mine, false).unwrap();
    }
    let state = harness.state_mut();
    state.presets = Presets::load(Some(&presets_dir)).unwrap();
    state.presets_dir = Some(presets_dir);
    shot(&mut harness, save, "edge-presets");
    preset_dialogs::ask(&harness.ctx, preset_dialogs::Ask::Manage);
    shot(&mut harness, save, "edge-manage-presets");
    harness.state_mut().preset_dialog = None;
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "edge-narrow-presets");
}

/// The banner of messages over the bottom of the window: a settings file
/// of an older version that can't be read, its details folded and open,
/// in the light theme, the dark one and the narrowest window; a line of
/// information; three messages; and one said three times.
fn banners(
    harness: &mut Harness<App>,
    save: &mut impl FnMut(&mut Harness<App>, &str),
    dir: &std::path::Path,
) {
    use crate::banner::Message;
    use crate::config::Theme;
    use crate::l10n::fl;
    // the error of such a file, said of one where settings are kept
    let old = dir.join("config.toml");
    std::fs::write(&old, "[ffmpeg]\nuse = \"auto\"\n").unwrap();
    let Err(tgradish_core::Error::Config { message, .. }) = Config::load(&old) else {
        panic!("an old settings file read")
    };
    let home = std::env::home_dir().unwrap_or_else(|| dir.to_path_buf());
    let path = home.join(".config").join("tgradish").join("config.toml");
    let err = tgradish_core::Error::Config { path: path.clone(), message };
    let unreadable = crate::app::settings_unreadable(fl!("setting-unsaved", "unreadable"), &err);
    let show = |harness: &mut Harness<App>, messages: &[Message]| {
        harness.state_mut().banner = Default::default();
        for message in messages {
            harness.state_mut().say(message.clone());
        }
        harness.run_steps(3);
    };
    let details = |harness: &mut Harness<App>| {
        harness.get_by_label(&fl!("details")).click();
        // the pointer away from what it would hover
        harness.event(egui::Event::PointerGone);
        harness.run_steps(3);
    };
    set_theme(&harness.ctx, Theme::Light);
    show(harness, std::slice::from_ref(&unreadable));
    save(harness, "banner-config");
    details(harness);
    save(harness, "banner-config-details");
    show(harness, &[Message::info(fl!("files-gone", count = 2))]);
    save(harness, "banner-info");
    let state = home.join(".local").join("state").join("tgradish").join("session.json");
    let unsaved = Message::warning(fl!("session-unsaved"))
        .file(state)
        .detail("Permission denied (os error 13)");
    let gone = Message::info(fl!("files-gone", count = 1));
    show(harness, &[unreadable.clone(), unsaved, gone]);
    save(harness, "banner-stacked");
    let pasted = Message::warning(fl!("paste-failed"))
        .detail("The clipboard holds neither files nor an image");
    show(harness, &[pasted.clone(), pasted.clone(), pasted]);
    save(harness, "banner-repeated");
    set_theme(&harness.ctx, Theme::Dark);
    show(harness, std::slice::from_ref(&unreadable));
    save(harness, "banner-config-dark");
    details(harness);
    save(harness, "banner-config-dark-details");
    set_theme(&harness.ctx, Theme::Light);
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 640.0));
    show(harness, std::slice::from_ref(&unreadable));
    details(harness);
    save(harness, "banner-narrow");
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.state_mut().banner = Default::default();
    set_theme(&harness.ctx, harness.state().config.gui.theme);
    harness.run_steps(3);
}

/// The window at work, for the README: `TGRADISH_SCREENSHOTS` says
/// where it goes.
#[test]
#[ignore]
fn readme_screenshot() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    // a folder whose name reads well where results are said to go
    let dir = std::env::temp_dir().join("tgradish-readme");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // the synthetic art the tests use
    let names = ["hammer_4x.gif", "walker_2x.gif", "dance.gif", "slime_jittered.gif"];
    let mut inputs = Vec::new();
    for file in tgradish_corpus::files() {
        if names.contains(&file.name.as_str()) {
            let path = dir.join(&file.name);
            std::fs::write(&path, &file.bytes).unwrap();
            inputs.push(path);
        }
    }
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.join("Stickers"));
    config.gui.theme = crate::config::Theme::Dark;
    let filtering = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    harness.state_mut().add(inputs);
    finish(&mut harness);
    // one made, another chosen, with the result's small picture
    let first = harness.state().items[0].id;
    harness.state_mut().select(first);
    harness.run_steps(2);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    let shown = harness.state().items[3].id;
    harness.state_mut().select(shown);
    let item = &mut harness.state_mut().items[3];
    item.view.inset = true;
    // a square crop, its handles out
    item.choices.crop = Some(Crop { x: 0, y: 2, width: 40, height: 40 });
    harness.event(egui::Event::PointerGone);
    harness.run_steps(3);
    std::thread::sleep(Duration::from_millis(300));
    harness.run_steps(3);
    let image = harness.render().expect("rendering needs a GPU");
    image.save(out.join("tgradish-readme.png")).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The window in accents of its own, in both themes: one of Plasma's, a
/// light one and a dark one, a file chosen, a button with the keyboard's
/// focus and one hovered; then Settings, where they are chosen, at the
/// usual width and the narrowest, and the picker of one's own.
fn accents(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    use crate::config::{Accent, Theme};
    use crate::l10n::fl;
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    let mut harness =
        Harness::builder().with_size([1280.0, 820.0]).wgpu().build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    let art = dir.path().join("pixel heart.png");
    square(&art, [220, 30, 60, 255]);
    let other = dir.path().join("pixel leaf.png");
    square(&other, [30, 160, 60, 255]);
    harness.state_mut().add(vec![art, other]);
    finish(&mut harness);
    let first = harness.state().items[0].id;
    harness.state_mut().select(first);
    let shot =
        |harness: &mut Harness<App>, save: &mut dyn FnMut(&mut Harness<App>, &str), name: &str| {
            harness.run_steps(3);
            save(harness, name);
        };
    let own = |r, g, b| Accent::Own(egui::Color32::from_rgb(r, g, b));
    for (name, accent) in [
        ("orange", Accent::Named("orange")),
        ("light", own(246, 240, 170)),
        ("dark", own(28, 36, 64)),
    ] {
        harness.state_mut().config.gui.accent = accent;
        for (theme, switch) in [(Theme::Dark, "☀"), (Theme::Light, "🌓")] {
            set_theme(&harness.ctx, theme);
            harness.run_steps(2);
            // the switch of themes with the keyboard's focus, the other
            // file hovered
            harness.get_by_label(switch).focus();
            harness.hover_at(egui::pos2(150.0, 190.0));
            let theme = if theme == Theme::Dark { "dark" } else { "light" };
            shot(&mut harness, save, &format!("accent-{name}-{theme}"));
        }
        harness.ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }
    harness.event(egui::Event::PointerGone);
    harness.state_mut().config.gui.accent = Accent::Named("teal");
    harness.state_mut().settings_dialog.open();
    for width in [1280.0, crate::MIN_WIDTH] {
        harness.set_size(egui::vec2(width, 820.0));
        for theme in [Theme::Dark, Theme::Light] {
            set_theme(&harness.ctx, theme);
            let theme = if theme == Theme::Dark { "dark" } else { "light" };
            shot(&mut harness, save, &format!("accent-settings-{width}-{theme}"));
        }
    }
    // the chosen one with the keyboard's focus, and the picker of one's own
    harness.set_size(egui::vec2(1280.0, 820.0));
    set_theme(&harness.ctx, Theme::Dark);
    harness.state_mut().config.gui.accent = own(200, 90, 160);
    harness.run_steps(2);
    harness.get_by_label(&fl!("accent-violet")).focus();
    shot(&mut harness, save, "accent-settings-focus");
    let name = fl!("accent-own", "chosen", colour = "#c85aa0");
    harness.get_by_label(&name).click();
    shot(&mut harness, save, "accent-picker");
}

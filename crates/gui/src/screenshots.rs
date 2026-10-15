//! Pictures of the window, for a look at its layout and for the README.
//! Ignored unless asked for, as they need a GPU wgpu can use and somewhere
//! to put them. This makes both:
//! `TGRADISH_SCREENSHOTS=/tmp/x cargo test -p tgradish-gui -- --ignored screenshots`
//! With `TGRADISH_LANG` they are in that language, as `pseudo` for English
//! made longer, to see where longer words would not fit.

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use tgradish_core::config::{Config, ResultPreview};
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{Crop, Fit, Options, Scaling};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::tgs::TgsOptions;

use crate::app::{App, set_theme};
use crate::item::{self, Show, Zoom};
use crate::jobs::{self, Job, Status};
use crate::testing::{convert, finish, square, video};
use crate::{inspect, prefs, preset_dialogs};

/// Renders the window to `$TGRADISH_SCREENSHOTS/*.png` for a look at
/// the layout: `TGRADISH_SCREENSHOTS=/tmp cargo test -p tgradish-gui
/// screenshots -- --ignored`. Needs a GPU wgpu can use, and ffmpeg.
#[test]
#[ignore]
fn screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
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
            let app = App::with(config.clone(), None, Presets::builtin(), None);
            app.style(&cc.egui_ctx);
            app
        });
    harness.run_steps(3);
    let save = |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    };
    save(&mut harness, "empty");
    if let Some(video) = video(dir.path(), "640x360") {
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        harness.state_mut().items[0].choices.crop =
            Some(Crop { x: 120, y: 60, width: 320, height: 200 });
        harness.state_mut().items[0].view.playing = false;
        harness.run_steps(3);
        save(&mut harness, "video");
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
        (item.format, item.choices.frame) = (Format::Image, Some(0.5));
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
        let mine = (&quick, &TgsOptions::default());
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
    harness.state_mut().prefs.open();
    harness.run_steps(3);
    save(&mut harness, "settings");
    harness.state_mut().prefs = prefs::Prefs::default();
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
        lossless_bytes: Some(200_000),
        steps: vec![tgradish_tgs::sticker::Step {
            reduction: tgradish_tgs::reduce::Reduction::MergeColours { distance: 0.02 },
            bytes: 110_000,
            error: 0.01,
        }],
        ..jobs::Progress::default()
    };
    let item = harness.state_mut().items.last_mut().unwrap();
    item.job = Some((job, Format::Tgs, item.choices.clone()));
    harness.run_steps(3);
    save(&mut harness, "progress");
    harness.state_mut().prefs.about = true;
    harness.run_steps(3);
    save(&mut harness, "about");
    harness.state_mut().prefs.about = false;
    set_theme(&harness.ctx, tgradish_core::config::Theme::Light);
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
        ResultPreview { size: 5, corner: tgradish_core::config::Corner::TopLeft };
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
    harness.set_size(egui::vec2(900.0, 640.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    harness.run_steps(3);
    save(&mut harness, "narrow");
    // a fractional scale, where the icon keeps to whole screen pixels
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.25)
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(|cc| {
            let app = App::with(Config::default(), None, Presets::builtin(), None);
            app.style(&cc.egui_ctx);
            app
        });
    harness.run_steps(3);
    save(&mut harness, "scaled");
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
    config.gui.theme = tgradish_core::config::Theme::Dark;
    let filtering = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin(), None);
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

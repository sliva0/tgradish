//! Pictures of the window, for a look at its layout and for the README.
//! Ignored unless asked for, as they need a GPU wgpu can use and somewhere
//! to put them. This makes both:
//! `TGRADISH_SCREENSHOTS=/tmp/x cargo test -p tgradish-gui -- --ignored screenshots`

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use tgradish_core::config::{Config, ResultPreview};
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{Crop, Scaling};
use tgradish_core::presets::{Format, Presets};

use crate::app::{App, set_theme};
use crate::item::{self, Show, Zoom};
use crate::jobs::{self, Job, Status};
use crate::testing::{convert, finish, square, video};
use crate::{inspect, prefs};

/// Renders the window to `$TGRADISH_SCREENSHOTS/*.png` for a look at
/// the layout: `TGRADISH_SCREENSHOTS=/tmp cargo test -p tgradish-gui
/// screenshots -- --ignored`. Needs a GPU wgpu can use, and ffmpeg.
#[test]
#[ignore]
fn screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
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
        harness.state_mut().items[0].format = Format::Webm;
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

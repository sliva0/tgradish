//! Pictures of the GUI, for a look at its layout and for the README.
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
use crate::timeline::PastLimit;
use crate::{inspect, preset_dialogs, settings_dialog};

/// Renders the GUI to `$TGRADISH_SCREENSHOTS/*.png` for a look at
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
    empties(&mut save);
    accents(&mut save);
    manipulation(&mut save);
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

/// Only the pictures of [`states`], to `$TGRADISH_SCREENSHOTS` as
/// [`screenshots`] makes them.
#[test]
#[ignore]
fn state_screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    states(&mut |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    });
}

/// Only the pictures of [`empties`], to `$TGRADISH_SCREENSHOTS` as
/// [`screenshots`] makes them.
#[test]
#[ignore]
fn empty_screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    empties(&mut |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    });
}

/// The settings of a video made into each format, in one column at the
/// usual size and the smallest, and in two on a wide screen, scrolled down
/// to them too, to see how they share the columns: to
/// `$TGRADISH_SCREENSHOTS` as [`screenshots`] makes them. Needs ffmpeg.
#[test]
#[ignore]
fn settings_screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let Some(clip) = video(dir.path(), "640x360") else { return };
    let sizes = [
        ("wide", egui::vec2(1280.0, 900.0)),
        ("smallest", egui::vec2(crate::MIN_WIDTH, crate::MIN_HEIGHT)),
        ("widest", egui::vec2(1920.0, 1080.0)),
    ];
    let formats = [(Format::Webp, "static"), (Format::Webm, "video"), (Format::Tgs, "animated")];
    for ((size_name, size), (format, format_name)) in
        sizes.into_iter().flat_map(|size| formats.map(|format| (size, format)))
    {
        let mut harness = window(config.clone(), size);
        harness.state_mut().add(vec![clip.clone()]);
        finish(&mut harness);
        harness.state_mut().items[0].format = format;
        finish(&mut harness);
        for item in &mut harness.state_mut().items {
            item.view.playing = false;
        }
        harness.event(egui::Event::PointerGone);
        harness.run_steps(6);
        let save = |harness: &mut Harness<App>, shot: &str| {
            let image = harness.render().expect("rendering needs a GPU");
            let name = format!("tgradish-settings-{format_name}-{size_name}{shot}.png");
            image.save(out.join(name)).unwrap();
        };
        save(&mut harness, "");
        // the settings, under the preview
        let screen = harness.ctx.content_rect();
        let under = egui::pos2(screen.center().x, screen.max.y - 120.0);
        harness.event(egui::Event::PointerMoved(under));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -2000.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.event(egui::Event::PointerGone);
        harness.run_steps(6);
        save(&mut harness, "-scrolled");
    }
}

/// The GUI without files, and its icon over the list: at the smallest
/// size, a little larger and the usual one, at whole and fractional
/// scales, in both themes.
fn empties(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    use crate::config::Theme;
    let sizes = [
        ("smallest", egui::vec2(crate::MIN_WIDTH, crate::MIN_HEIGHT)),
        ("small", egui::vec2(1024.0, 640.0)),
        ("wide", egui::vec2(1280.0, 820.0)),
    ];
    for (size_name, size) in sizes {
        for ppp in [1.0, 1.25, 2.0] {
            for theme in [Theme::Dark, Theme::Light] {
                let theme_name = if theme == Theme::Dark { "dark" } else { "light" };
                let mut harness = Harness::builder()
                    .with_size(size)
                    .with_pixels_per_point(ppp)
                    .wgpu()
                    .build_eframe(|cc| {
                        let app = App::with(Config::default(), None, Presets::builtin());
                        app.style(&cc.egui_ctx);
                        app
                    });
                set_theme(&harness.ctx, theme);
                harness.run_steps(3);
                save(&mut harness, &format!("empty-{size_name}-{ppp}x-{theme_name}"));
            }
        }
    }
}

/// Only the pictures of [`notices`], to `$TGRADISH_SCREENSHOTS` as
/// [`screenshots`] makes them.
#[test]
#[ignore]
fn notice_screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    notices(&mut |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    });
}

/// What passes in the corner over the list, and what stays at the
/// bottom: files removed, with a short list and a full one; two and more
/// at once; nothing to paste, and the open files not kept, which say why
/// in tgradish's words and in the system's; at the usual size and the
/// smallest, in both themes.
fn notices(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    use crate::banner::Message;
    use crate::config::Theme;
    use crate::l10n::fl;
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let arts: Vec<PathBuf> = (0..24)
        .map(|n| {
            let art = dir.path().join(format!("sticker {n:02}.png"));
            square(&art, [10 * n as u8, 160, 200 - 8 * n as u8, 255]);
            art
        })
        .collect();
    let shot =
        |harness: &mut Harness<App>, save: &mut dyn FnMut(&mut Harness<App>, &str), name: &str| {
            // the pointer away from what it would hover
            harness.event(egui::Event::PointerGone);
            for item in &mut harness.state_mut().items {
                item.view.playing = false;
            }
            harness.run_steps(3);
            save(harness, name);
        };
    let sizes =
        [("wide", egui::vec2(1280.0, 820.0)), ("small", egui::vec2(crate::MIN_WIDTH, 560.0))];
    for (size_name, size) in sizes {
        for theme in [Theme::Dark, Theme::Light] {
            let theme_name = if theme == Theme::Dark { "dark" } else { "light" };
            let name = |scene: &str| format!("notice-{scene}-{size_name}-{theme_name}");
            let mut harness = window(config.clone(), size);
            set_theme(&harness.ctx, theme);
            // a short list, one of it removed
            harness.state_mut().add(arts[..3].to_vec());
            finish(&mut harness);
            let first = harness.state().items[0].id;
            harness.state_mut().remove(first);
            shot(&mut harness, save, &name("one-short"));
            // two at once, and more than are shown
            harness.state_mut().say(Message::info(fl!("files-gone", count = 2)));
            shot(&mut harness, save, &name("two"));
            let settings = dir.path().join("config.toml");
            harness.state_mut().say(Message::info(fl!("settings-read")).file(settings));
            let session = dir.path().join("session.json.unreadable");
            let unreadable = Message::info(fl!("files-unreadable", count = 3)).file(session);
            let show = [crate::banner::Action::ShowInFolder];
            harness.state_mut().say(unreadable.actions(&show));
            shot(&mut harness, save, &name("many"));
            // a list longer than the window, one removed
            harness.state_mut().banner = Default::default();
            harness.state_mut().add(arts[3..].to_vec());
            finish(&mut harness);
            let some = harness.state().items[5].id;
            harness.state_mut().remove(some);
            shot(&mut harness, save, &name("one-full"));
            // why in tgradish's words: nothing to paste
            harness.state_mut().banner = Default::default();
            let empty = tgradish_core::Error::Clipboard(tgradish_core::ClipboardError::Empty);
            harness.state_mut().say(crate::app::not_pasted(&empty));
            shot(&mut harness, save, &name("paste"));
            // and as the system said it: permission denied, on Linux
            harness.state_mut().banner = Default::default();
            let session = dir.path().join("session.json");
            let denied = std::io::Error::from_raw_os_error(13);
            harness.state_mut().say(crate::app::session_unsaved(&session, &denied));
            shot(&mut harness, save, &name("session"));
        }
    }
}

/// The timeline of a video whose used part runs past Telegram's 3
/// seconds, and of one within them, in each of Plasma's accents and two of
/// one's own, a light and a dark one, in both themes: as a WebM spoofed
/// and one cut at 3 s, and as a `.tgs` sped up to fit and one at a speed
/// set, so cut; and the limit's mark hovered in each. To
/// `$TGRADISH_SCREENSHOTS` as [`screenshots`] makes them. Needs ffmpeg.
#[test]
#[ignore]
fn limit_screenshots() {
    use crate::config::{Accent, Theme};
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let mut harness = window(config, egui::vec2(1280.0, 820.0));
    let Some(clip) = crate::testing::video_of(dir.path(), "640x360", 6) else { return };
    harness.state_mut().add(vec![clip]);
    finish(&mut harness);
    let own = |r, g, b| Accent::Own(egui::Color32::from_rgb(r, g, b));
    let accents = crate::theme::ACCENTS
        .iter()
        .map(|(name, _)| (*name, Accent::Named(name)))
        .chain([("own-light", own(246, 240, 170)), ("own-dark", own(28, 36, 64))]);
    // hovers at once
    harness.ctx.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let accents: Vec<_> = accents.collect();
    let never = Some(tgradish_core::options::Spoof::Never);
    let handlings = [
        ("spoofed", Format::Webm, None, None),
        ("cut", Format::Webm, never, None),
        ("sped-up", Format::Tgs, None, None),
        ("tgs-cut", Format::Tgs, None, Some(1.0)),
    ];
    for (handling, format, spoof, tempo) in handlings {
        let item = &mut harness.state_mut().items[0];
        (item.format, item.choices.webm.spoof, item.choices.tgs.tempo) = (format, spoof, tempo);
        finish(&mut harness);
        for &(name, accent) in &accents {
            harness.state_mut().config.gui.accent = accent;
            // from half a second to five, and to three, the playhead in it
            for (part, used) in [("over", 4.5), ("within", 2.5)] {
                let item = &mut harness.state_mut().items[0];
                (item.choices.start, item.choices.length) = (Some(0.5), Some(used));
                (item.view.time, item.view.playing, item.view.show) = (1.5, false, Show::Input);
                for theme in [Theme::Dark, Theme::Light] {
                    set_theme(&harness.ctx, theme);
                    harness.event(egui::Event::PointerGone);
                    harness.run_steps(3);
                    let theme = if theme == Theme::Dark { "dark" } else { "light" };
                    let image = harness.render().expect("rendering needs a GPU");
                    let file = format!("limit-{handling}-{part}-{name}-{theme}.png");
                    image.save(out.join(file)).unwrap();
                }
            }
        }
        // the limit's mark hovered, on the used part running past it
        harness.state_mut().config.gui.accent = Accent::default();
        let item = &mut harness.state_mut().items[0];
        (item.choices.start, item.choices.length) = (Some(0.5), Some(4.5));
        for theme in [Theme::Dark, Theme::Light] {
            set_theme(&harness.ctx, theme);
            harness.event(egui::Event::PointerGone);
            harness.run_steps(3);
            // the tick, as far along the bar as the limit is from the start
            let app = harness.state();
            let (item, presets, config) = (&app.items[0], &app.presets, &app.config);
            let at = (0.5 + 3.0 * item.tempo(presets, config)) / item.input_length().unwrap();
            let webm = item.choices.webm_options(presets, config).unwrap_or_default();
            let tgs = item.choices.tgs_options(presets, config).unwrap_or_default();
            let said = Some(PastLimit::of(item.format, &webm, &tgs).text());
            let bar = harness.query_all(egui_kittest::kittest::by()).find(|node| {
                use egui_kittest::kittest::NodeT;
                node.accesskit_node().description() == said
            });
            let (bar, at) =
                (bar.expect("the timeline says what is past the limit").rect(), at as f32);
            let track = bar.shrink2(egui::vec2(8.0, 0.0));
            harness.hover_at(egui::pos2(track.left() + at * track.width(), bar.center().y));
            harness.run_steps(3);
            let theme = if theme == Theme::Dark { "dark" } else { "light" };
            let image = harness.render().expect("rendering needs a GPU");
            image.save(out.join(format!("limit-{handling}-hovered-{theme}.png"))).unwrap();
        }
    }
}

/// Only the pictures of [`manipulation`], to `$TGRADISH_SCREENSHOTS` as
/// [`screenshots`] makes them.
#[test]
#[ignore]
fn manipulation_screenshots() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    manipulation(&mut |harness: &mut Harness<App>, name: &str| {
        let image = harness.render().expect("rendering needs a GPU");
        image.save(out.join(format!("tgradish-{name}.png"))).unwrap();
    });
}

/// The bar of the timeline under the preview, which says what is past the
/// limit, as long as the limit is on it.
fn timeline_bar(harness: &Harness<App>) -> egui::Rect {
    use egui_kittest::kittest::NodeT;
    let said = [PastLimit::Spoofed, PastLimit::SpedUp, PastLimit::Cut].map(PastLimit::text);
    let bar = harness
        .query_all(egui_kittest::kittest::by())
        .find(|node| node.accesskit_node().description().is_some_and(|text| said.contains(&text)));
    bar.expect("the timeline says what is past the limit").rect()
}

/// Where the input is shown on the preview, and how many points a pixel
/// of it takes.
fn input_picture(harness: &Harness<App>) -> (egui::Rect, f32) {
    let item = &harness.state().items[0];
    let zoom = &item.view.input_zoom;
    let (_, fit_in) = zoom.anchor.expect("shown");
    crate::canvas::layout(fit_in, item.input_size().unwrap(), zoom)
}

/// Presses the primary button at `from` and moves the pointer by `by`, a
/// step at a time, without letting go.
fn press_and_move(harness: &mut Harness<App>, from: egui::Pos2, by: egui::Vec2) {
    harness.event(egui::Event::PointerMoved(from));
    harness.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    for step in 1..=4 {
        harness.event(egui::Event::PointerMoved(from + by * step as f32 / 4.0));
        harness.step();
    }
}

/// Lets go of the primary button at `at`.
fn let_go(harness: &mut Harness<App>, at: egui::Pos2) {
    harness.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
}

/// What the pointer does on the timeline and the preview, as it does it:
/// an end of a short part of a long video hovered. Needs ffmpeg.
fn manipulation(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let Some(clip) = crate::testing::video_of(dir.path(), "640x360", 8) else { return };
    let mut harness = window(config, egui::vec2(1280.0, 820.0));
    harness.state_mut().add(vec![clip]);
    finish(&mut harness);
    let settle = |harness: &mut Harness<App>| {
        for item in &mut harness.state_mut().items {
            item.view.playing = false;
        }
        harness.run_steps(3);
    };
    // a third of a second used, from 3 seconds in
    let item = &mut harness.state_mut().items[0];
    (item.choices.start, item.choices.length, item.view.time) = (Some(3.0), Some(0.3), 3.1);
    settle(&mut harness);
    let track = timeline_bar(&harness).shrink2(egui::vec2(8.0, 0.0));
    let x_of = |seconds: f32| track.left() + seconds / 8.0 * track.width();
    harness.event(egui::Event::PointerMoved(egui::pos2(x_of(3.3) + 2.0, track.center().y)));
    settle(&mut harness);
    save(&mut harness, "trim-end-hovered");
    // dragged on, the button held, where it is said under the bar
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let from = egui::pos2(x_of(3.3) + 2.0, track.center().y);
    harness.event(press(from, true));
    harness.step();
    for step in 1..=4 {
        harness.event(egui::Event::PointerMoved(from + egui::vec2(10.0 * step as f32, 0.0)));
        harness.step();
    }
    save(&mut harness, "trim-end-dragged");
    harness.event(press(from + egui::vec2(40.0, 0.0), false));
    settle(&mut harness);
    // the pointer near the bar's end, in the narrowest window, where the
    // unit's segments go under the bar
    for width in [crate::MIN_WIDTH, 1920.0] {
        harness.set_size(egui::vec2(width, 820.0));
        harness.event(egui::Event::PointerGone);
        settle(&mut harness);
        let track = timeline_bar(&harness).shrink2(egui::vec2(8.0, 0.0));
        harness.event(egui::Event::PointerMoved(track.right_center() - egui::vec2(4.0, 0.0)));
        settle(&mut harness);
        save(&mut harness, &format!("timeline-hovered-{width}"));
    }
    harness.set_size(egui::vec2(1280.0, 820.0));
    // the result's bar, hovered
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    let play = harness.get_by_label(&crate::l10n::fl!("play")).rect();
    harness.event(egui::Event::PointerMoved(play.right_center() + egui::vec2(200.0, 0.0)));
    settle(&mut harness);
    save(&mut harness, "player-hovered");
    harness.state_mut().items[0].view.show = Show::Input;
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    // a crop moved, snapped to the picture's middle, the button held
    let item = &mut harness.state_mut().items[0];
    item.choices.crop = Some(Crop { x: 100, y: 60, width: 240, height: 160 });
    settle(&mut harness);
    let (shown, scale) = input_picture(&harness);
    let inside = shown.min + egui::vec2(200.0, 120.0) * scale;
    let by = egui::vec2(117.0, 37.0) * scale;
    press_and_move(&mut harness, inside, by);
    save(&mut harness, "crop-snapped");
    let_go(&mut harness, inside + by);
    // which leaves it with the keyboard's focus, moved by the arrows
    harness.event(egui::Event::PointerGone);
    settle(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    settle(&mut harness);
    save(&mut harness, "crop-focused");
    harness.key_press(egui::Key::Escape);
    settle(&mut harness);
    // at the top of the picture, its size within it; and smaller, under it
    for (crop, name) in [
        (Crop { x: 100, y: 0, width: 240, height: 160 }, "crop-at-top"),
        (Crop { x: 100, y: 0, width: 40, height: 30 }, "crop-small-at-top"),
    ] {
        harness.state_mut().items[0].choices.crop = Some(crop);
        settle(&mut harness);
        save(&mut harness, name);
    }
    // zoomed in as far as it goes, into the crop's corner
    harness.state_mut().items[0].view.input_zoom.level = f32::MAX;
    settle(&mut harness);
    save(&mut harness, "zoomed-most");
    // eased out by Zoom out, to where it fits
    harness.get_by_label_contains(&crate::l10n::fl!("zoom-out")).click();
    harness.run_steps(20);
    save(&mut harness, "zoomed-out");
    harness.state_mut().items[0].view.input_zoom = Zoom::default();
    settle(&mut harness);
    // the shortcuts, scrolled to what the mouse does
    harness.state_mut().shortcuts = true;
    settle(&mut harness);
    let middle = harness.ctx.content_rect().center();
    harness.event(egui::Event::PointerMoved(middle));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -2000.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run_steps(8);
    save(&mut harness, "shortcuts-mouse");
    harness.state_mut().shortcuts = false;
    settle(&mut harness);
}

/// The GUI drawn as the screenshots draw it, of `config`, in a window
/// `size` large.
fn window(config: Config, size: egui::Vec2) -> Harness<'static, App> {
    // filtering as the GUI does it, so pixel art stays sharp
    let filtering = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    Harness::builder().with_size(size).with_render_options(filtering).wgpu().build_eframe(
        move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        },
    )
}

/// The bar over the preview, at the narrowest window, the usual one and a
/// wide one, where what the mouse does fits: a video made, its result and
/// its input, then cropped, zoomed in and changed since; and the result's
/// small picture of pixel art, at its smallest and largest. Needs ffmpeg.
fn bars_and_insets(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    use crate::config::Corner;
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let shot =
        |harness: &mut Harness<App>, save: &mut dyn FnMut(&mut Harness<App>, &str), name: &str| {
            harness.event(egui::Event::PointerGone);
            for item in &mut harness.state_mut().items {
                item.view.playing = false;
            }
            harness.run_steps(3);
            std::thread::sleep(Duration::from_millis(300));
            harness.run_steps(3);
            save(harness, name);
        };
    let Some(clip) = video(dir.path(), "640x360") else { return };
    for width in [crate::MIN_WIDTH, 1280.0, 1920.0] {
        let mut harness = window(config.clone(), egui::vec2(width, 820.0));
        harness.state_mut().add(vec![clip.clone()]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        shot(&mut harness, save, &format!("bar-result-{width}"));
        harness.state_mut().items[0].view.show = Show::Input;
        shot(&mut harness, save, &format!("bar-input-{width}"));
        if width == 1920.0 {
            set_theme(&harness.ctx, crate::config::Theme::Light);
            shot(&mut harness, save, &format!("bar-input-{width}-light"));
            set_theme(&harness.ctx, crate::config::Theme::Dark);
        }
        let item = &mut harness.state_mut().items[0];
        item.view.show = Show::Input;
        item.choices.crop = Some(Crop { x: 120, y: 60, width: 320, height: 200 });
        item.view.input_zoom.level = 1.5;
        shot(&mut harness, save, &format!("bar-crop-zoomed-{width}"));
        // changed since it was made
        harness.state_mut().items[0].choices.webm.tempo = Some(2.0);
        shot(&mut harness, save, &format!("bar-outdated-{width}"));
    }
    // pixel art as a sticker, and as an emoji, whose result is small
    let walker = tgradish_corpus::sprite("walker_2x");
    let ase = dir.path().join("walker.ase");
    std::fs::write(&ase, tgradish_corpus::encode::aseprite_with(&walker, &[], &[])).unwrap();
    let mut harness = window(config.clone(), egui::vec2(1280.0, 1300.0));
    harness.state_mut().add(vec![ase]);
    finish(&mut harness);
    let item = &mut harness.state_mut().items[0];
    (item.format, item.format_guessed) = (Format::Webm, false);
    finish(&mut harness);
    for (target, name) in [
        (tgradish_core::telegram::Target::Sticker, "sticker"),
        (tgradish_core::telegram::Target::Emoji, "emoji"),
    ] {
        harness.state_mut().items[0].choices.target = target;
        for size in [1, 3, 5] {
            harness.state_mut().config.gui.result_preview =
                ResultPreview { size, corner: Corner::default() };
            shot(&mut harness, save, &format!("inset-{name}-{size}"));
        }
    }
}

/// The states the GUI shows of what goes on around a conversion: a
/// video made. Needs ffmpeg.
fn states(save: &mut impl FnMut(&mut Harness<App>, &str)) {
    bars_and_insets(save);
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    let shot = |harness: &mut Harness<App>, save: &mut dyn FnMut(&mut Harness<App>, &str), name| {
        // the pointer away from what it would hover
        harness.event(egui::Event::PointerGone);
        for item in &mut harness.state_mut().items {
            item.view.playing = false;
        }
        harness.run_steps(3);
        save(harness, name);
    };
    let mut harness = window(config.clone(), egui::vec2(1280.0, 820.0));
    // Settings, at the usual size and the smallest
    harness.state_mut().settings_dialog.open();
    shot(&mut harness, save, "state-settings");
    // made larger by its buttons, the pointer gone from them since: back
    // in the middle
    for _ in 0..2 {
        let plus = harness.get_by_label(&crate::l10n::fl!("scale-larger")).rect().center();
        harness.event(egui::Event::PointerMoved(plus));
        for pressed in [true, false] {
            let (button, modifiers) = (egui::PointerButton::Primary, egui::Modifiers::NONE);
            harness.event(egui::Event::PointerButton { pos: plus, button, pressed, modifiers });
            harness.step();
        }
        harness.run_steps(2);
    }
    harness.event(egui::Event::PointerMoved(egui::pos2(5.0, 5.0)));
    harness.run_steps(4);
    shot(&mut harness, save, "state-settings-scaled");
    harness.state_mut().config.gui.scale = 1.0;
    harness.run_steps(3);
    // half a path typed for the folder, said to be wrong under it
    harness.state_mut().config.gui.output_dir = None;
    harness.run_steps(2);
    harness.get_by_label(&crate::l10n::fl!("in-a-folder")).click();
    harness.run_steps(2);
    let field = || {
        let fields =
            harness.query_all(egui_kittest::kittest::by().role(egui::accesskit::Role::TextInput));
        use egui_kittest::kittest::NodeT;
        fields.last().expect("the folder's field").accesskit_node().id()
    };
    let field = field();
    let by_id = || egui_kittest::kittest::by().predicate(move |node| node.id() == field);
    harness.get(by_id()).focus();
    harness.run_steps(1);
    harness.get(by_id()).type_text("Stickers");
    shot(&mut harness, save, "state-settings-typed");
    // back to the defaults, with a way back
    harness.get_by_label(&crate::l10n::fl!("reset-settings")).click();
    shot(&mut harness, save, "state-settings-reset");
    harness.state_mut().banner = Default::default();
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 640.0));
    shot(&mut harness, save, "state-settings-small");
    harness.state_mut().settings_dialog = settings_dialog::SettingsDialog::default();
    harness.set_size(egui::vec2(1280.0, 820.0));
    let Some(clip) = video(dir.path(), "640x360") else { return };
    harness.state_mut().add(vec![clip]);
    finish(&mut harness);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    shot(&mut harness, save, "state-video-made");
    // the timeline and the frame picker at the narrowest
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    harness.state_mut().items[0].view.show = Show::Input;
    shot(&mut harness, save, "state-timeline-narrow");
    harness.state_mut().items[0].format = Format::Webp;
    finish(&mut harness);
    shot(&mut harness, save, "state-picker-narrow");
    harness.state_mut().items[0].format = Format::Webm;
    finish(&mut harness);
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    // what keeps files from being converted, said before converting: a
    // crop kept from a larger file
    harness.state_mut().items[0].choices.crop =
        Some(Crop { x: 700, y: 400, width: 40, height: 40 });
    shot(&mut harness, save, "state-blocked-crop");
    harness.state_mut().items[0].choices.crop = None;
    // converting failed, said in short on the bar and in full beside it
    harness.state_mut().items[0].choices.preset = Some("gone".into());
    // the bar back on one line, Convert where it stays
    harness.run_steps(4);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    shot(&mut harness, save, "state-failed");
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "state-failed-narrow");
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    harness.state_mut().items[0].choices.preset = None;
    // a sprite sheet whose cells don't divide it
    let art = dir.path().join("sheet.png");
    square(&art, [220, 30, 60, 255]);
    harness.state_mut().add(vec![art]);
    let item = harness.state_mut().items.last_mut().unwrap();
    (item.format, item.format_guessed) = (Format::Tgs, false);
    item.choices.tgs.sheet = "3x1".parse().ok();
    finish(&mut harness);
    shot(&mut harness, save, "state-blocked-sheet");
    // a tag and a layer an Aseprite file no longer has
    let walker = tgradish_corpus::sprite("walker_2x");
    let bytes = tgradish_corpus::encode::aseprite_with(&walker, &[("idle", 0, 1)], &[]);
    let ase = dir.path().join("walker.ase");
    std::fs::write(&ase, bytes).unwrap();
    harness.state_mut().add(vec![ase]);
    let item = harness.state_mut().items.last_mut().unwrap();
    item.choices.tag = Some("gone".into());
    item.choices.hide_layers = Some(vec!["sketch".into()]);
    item.choices.target = tgradish_core::telegram::Target::Emoji;
    finish(&mut harness);
    shot(&mut harness, save, "state-blocked-tag");
    harness.set_size(egui::vec2(1280.0, 1300.0));
    shot(&mut harness, save, "state-blocked-tag-tall");
    harness.set_size(egui::vec2(1280.0, 820.0));
    // a long name, to a folder deep down, at the narrowest: where it goes
    // on a line of its own
    let long = "holiday at the seaside with the whole family, the long cut, final edit (2)";
    let art = dir.path().join(format!("{long}.png"));
    square(&art, [30, 160, 60, 255]);
    harness.state_mut().config.gui.output_dir =
        Some(dir.path().join("out").join("for the family").join("Stickers"));
    harness.state_mut().add(vec![art]);
    finish(&mut harness);
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "state-bar-two-lines");
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "state-bar-one-line");
    // no ffmpeg, a video and pixel art open
    let clip = harness.state().items[0].inputs[0].clone();
    let mut config = config.clone();
    config.ffmpeg.path = Some(dir.path().join("no-ffmpeg-here"));
    let mut harness = window(config, egui::vec2(1280.0, 820.0));
    let art = dir.path().join("pixel heart.png");
    square(&art, [220, 30, 60, 255]);
    harness.state_mut().add(vec![art, clip]);
    finish(&mut harness);
    shot(&mut harness, save, "state-no-ffmpeg");
    // all of them removed, with a way back
    let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
    harness.state_mut().remove_all(&ids);
    shot(&mut harness, save, "state-undo");
    // a batch at work: one converting, the rest waiting
    let mut harness = window(Config::default(), egui::vec2(1280.0, 820.0));
    let arts: Vec<PathBuf> = (0..5)
        .map(|n| {
            let art = dir.path().join(format!("frame {n}.png"));
            square(&art, [40 * n as u8, 160, 60, 255]);
            art
        })
        .collect();
    harness.state_mut().add(arts);
    finish(&mut harness);
    let state = harness.state_mut();
    for (index, item) in state.items.iter_mut().enumerate() {
        let mut job = Job::waiting(dir.path().join(format!("frame {index}.sticker.tgs")));
        if index == 0 {
            job.status = Status::Running;
        } else {
            state.queue.push_back(item.id);
        }
        item.job = Some(crate::item::Conversion::of(job, Format::Tgs, item.choices.clone()));
    }
    state.batch = 12;
    shot(&mut harness, save, "state-queue");
    harness.set_size(egui::vec2(crate::MIN_WIDTH, 640.0));
    harness.ctx.memory_mut(|memory| memory.reset_areas());
    shot(&mut harness, save, "state-queue-narrow");
}

/// Takes the GUI through each state worth a picture, handing each to
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
    // filtering as the GUI does it, so pixel art stays sharp
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
        harness.get_by_label(&crate::l10n::fl!("add-more")).click();
        harness.run_steps(2);
        save(&mut harness, "add-menu");
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        // the menu of Settings and the rest
        harness.get_by_label(&crate::l10n::fl!("main-menu")).click();
        harness.run_steps(2);
        save(&mut harness, "main-menu");
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        // a row's own menu, its right button clicked
        let name = harness.state().items[0].name();
        let row = harness.get_all_by_label(&name).next().unwrap().rect().center();
        harness.event(egui::Event::PointerMoved(row));
        for pressed in [true, false] {
            let (button, modifiers) = (egui::PointerButton::Secondary, egui::Modifiers::NONE);
            harness.event(egui::Event::PointerButton { pos: row, button, pressed, modifiers });
            harness.step();
        }
        harness.run_steps(2);
        save(&mut harness, "row-menu");
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
        item.choices.webm.scaling = Some(Scaling::Integer);
        item.choices.webm.integer_scale = Some("1/2".parse().unwrap());
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
        // a frame of it as a static sticker, picked on the bar
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
    // names in scripts the GUI's fonts lack, drawn with the system's
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
    harness.state_mut().about = true;
    harness.run_steps(3);
    save(&mut harness, "about");
    harness.state_mut().about = false;
    harness.state_mut().shortcuts = true;
    harness.run_steps(3);
    save(&mut harness, "shortcuts");
    harness.state_mut().shortcuts = false;
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
    (job.status, job.settles) = (Status::Running, true);
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
    set_theme(&harness.ctx, crate::config::Theme::Dark);
    harness.run_steps(3);
    save(&mut harness, "converting-dark");
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

/// The GUI where it is most likely to break: a result in a short
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
    harness.get_by_label(&crate::l10n::fl!("light-theme")).focus();
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
/// information, which passes in the corner over the list; three messages;
/// and one said three times.
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
        .literal("Permission denied (os error 13)");
    let gone = Message::info(fl!("files-gone", count = 1));
    show(harness, &[unreadable.clone(), unsaved, gone]);
    save(harness, "banner-stacked");
    let empty = tgradish_core::Error::Clipboard(tgradish_core::ClipboardError::Empty);
    let pasted = crate::app::not_pasted(&empty);
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

/// The GUI at work, for the README: `TGRADISH_SCREENSHOTS` says
/// where it goes.
#[test]
#[ignore]
fn readme_screenshot() {
    let out =
        PathBuf::from(std::env::var_os("TGRADISH_SCREENSHOTS").expect("TGRADISH_SCREENSHOTS"));
    crate::l10n::choose(None);
    let dir = tempfile::tempdir().unwrap();
    let inputs = readme_inputs(dir.path());
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("Stickers"));
    config.gui.theme = crate::config::Theme::Dark;
    let filtering = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    // narrow enough that the README, which shows it about 900 wide, keeps
    // its smaller words readable, the settings in one column; wide enough
    // for the timeline to stay on one row; as tall as the settings below
    // the preview end between two of them rather than through one; drawn
    // at twice the pixels, so that it is sharp as it is shown smaller and
    // on screens of more pixels
    let mut harness = Harness::builder()
        .with_size([1280.0, 880.0])
        .with_pixels_per_point(2.0)
        .with_render_options(filtering)
        .wgpu()
        .build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    harness.state_mut().add(inputs);
    finish(&mut harness);
    // [frog, sparkle, fox, cherry, eagle, house, seaside]: the sparkle a
    // custom emoji, the eagle a video sticker, the house a picture one
    let state = harness.state_mut();
    state.items[1].choices.target = tgradish_core::telegram::Target::Emoji;
    (state.items[4].format, state.items[4].format_guessed) = (Format::Webm, false);
    (state.items[5].format, state.items[5].format_guessed) = (Format::Webp, false);
    // the sparkle and the cherry made, with the results' small pictures
    for made in [1, 3] {
        let id = harness.state().items[made].id;
        harness.state_mut().select(id);
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
    }
    // the frog chosen, its idle tag, cropped to the frog and a little
    // around it, 4:3, and made, with the result's small picture and what
    // Telegram will make of it
    let frog = harness.state().items[0].id;
    harness.state_mut().select(frog);
    let item = &mut harness.state_mut().items[0];
    item.choices.tag = Some("idle".into());
    item.choices.crop = Some(Crop { x: 1, y: 5, width: 32, height: 24 });
    item.view.inset = true;
    finish(&mut harness);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    // said to go where pictures usually do, rather than to the test's
    // temporary folder
    harness.state_mut().config.gui.output_dir = Some("/home/user/Pictures/Stickers".into());
    // the input shown, with the crop over it
    let item = &mut harness.state_mut().items[0];
    item.view.show = crate::item::Show::Input;
    item.view.playing = false;
    // its first frame, calm, in the preview and the result alike
    (item.view.time, item.view.result_time) = (0.0, 0.0);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(3);
    std::thread::sleep(Duration::from_millis(300));
    harness.run_steps(3);
    let image = harness.render().expect("rendering needs a GPU");
    // packed as tightly as PNG goes, for the README to load quickly
    let file = std::fs::File::create(out.join("tgradish-readme.png")).unwrap();
    let png = image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    );
    image.write_with_encoder(png).unwrap();
}

/// The README's inputs, written to `dir`: pixel art from ansimuz's Sunny
/// Land (CC0, see readme-art/CREDITS.md) as a sticker artist would have it.
/// The frog and the fox are Aseprite files of their sheets, tagged, and the
/// seaside a video of a camera panning along the shore.
fn readme_inputs(dir: &std::path::Path) -> Vec<PathBuf> {
    use tgradish_corpus::{Canvas, Sprite};
    let art = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("readme-art");
    // the frames of a sheet of cells `width` wide
    let cells = |name: &str, width: u32| {
        let sheet = image::open(art.join(name)).unwrap().to_rgba8();
        let height = sheet.height();
        (0..sheet.width() / width)
            .map(|cell| {
                let frame = image::imageops::crop_imm(&sheet, cell * width, 0, width, height);
                let rgba = frame.to_image().into_raw();
                Canvas { width, height, rgba }
            })
            .collect::<Vec<_>>()
    };
    // an Aseprite file of `tags` in turn, each of its frames, 100 ms a
    // frame, as the pack's GIFs show them
    let aseprite = |name: &str, tags: Vec<(&str, Vec<Canvas>)>| {
        let (mut frames, mut ranges) = (Vec::new(), Vec::new());
        for (tag, cells) in tags {
            let first = frames.len() as u16;
            frames.extend(cells);
            ranges.push((tag, first, frames.len() as u16 - 1));
        }
        let delays = vec![100; frames.len()];
        let sprite =
            Sprite { name: "art", about: "", frames, delays, scale: 1, likely_scale: None };
        let path = dir.join(name);
        std::fs::write(&path, tgradish_corpus::encode::aseprite_with(&sprite, &ranges, &[]))
            .unwrap();
        path
    };
    // the frog sits calm for a second, then croaks, as the pack's GIF
    // has it; the calm frame repeated rather than held, so that the
    // list's small picture, a third of the way in, is of it too
    let idle = cells("frog-idle.png", 35);
    let calm = std::iter::repeat_n(idle[0].clone(), 10);
    let idle = calm.chain(idle[1..].iter().cloned()).collect();
    let frog =
        aseprite("frog.aseprite", vec![("idle", idle), ("jump", cells("frog-jump.png", 35))]);
    let fox = aseprite(
        "fox.aseprite",
        vec![("idle", cells("fox-idle.png", 33)), ("run", cells("fox-run.png", 33))],
    );
    let copied = |name: &str| {
        std::fs::copy(art.join(name), dir.join(name)).unwrap();
        dir.join(name)
    };
    let mut inputs = vec![frog, copied("sparkle.gif"), fox, copied("cherry.gif")];
    inputs.extend([copied("eagle.gif"), copied("house.png")]);
    // three seconds along the shore, at twice the art's size
    let seaside = dir.join("seaside.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-loop", "1", "-framerate", "30", "-i"])
        .arg(art.join("seaside.png"))
        .args(["-vf", "crop=384:216:min(t*124\\,372):0,scale=768:432:flags=neighbor"])
        .args(["-t", "3", "-pix_fmt", "yuv420p"])
        .arg(&seaside)
        .status()
        .expect("the README's screenshot needs ffmpeg");
    assert!(made.success(), "ffmpeg could not make the seaside");
    inputs.push(seaside);
    inputs
}

/// The GUI in accents of its own, in both themes: one of Plasma's, a
/// light one and a dark one, a file chosen, its result out of date, a
/// button with the keyboard's focus and one hovered; then Settings, where they are chosen, at the
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
    // the selected one's result out of date, its badge on the lit row
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    harness.state_mut().items[0].choices.tgs.tempo = Some(2.0);
    let shot =
        |harness: &mut Harness<App>, save: &mut dyn FnMut(&mut Harness<App>, &str), name: &str| {
            harness.run_steps(3);
            save(harness, name);
        };
    let own = |r, g, b| Accent::Own(egui::Color32::from_rgb(r, g, b));
    for (name, accent) in [
        ("orange", Accent::Named("orange")),
        ("grey", Accent::Named("grey")),
        ("light", own(246, 240, 170)),
        ("dark", own(28, 36, 64)),
    ] {
        harness.state_mut().config.gui.accent = accent;
        let (light, dark) = (crate::l10n::fl!("light-theme"), crate::l10n::fl!("dark-theme"));
        for (theme, switch) in [(Theme::Dark, &light), (Theme::Light, &dark)] {
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
    // the grey, whose choices a tint of it would hardly set apart
    harness.set_size(egui::vec2(1280.0, 820.0));
    harness.state_mut().config.gui.accent = Accent::Named("grey");
    for theme in [Theme::Dark, Theme::Light] {
        set_theme(&harness.ctx, theme);
        let theme = if theme == Theme::Dark { "dark" } else { "light" };
        shot(&mut harness, save, &format!("accent-settings-grey-{theme}"));
    }
    // at 125%, as egui sees a window of the same pixels then
    harness.state_mut().config.gui.accent = Accent::Named("teal");
    set_theme(&harness.ctx, Theme::Dark);
    harness.state_mut().config.gui.scale = 1.25;
    harness.run_steps(6);
    shot(&mut harness, save, "accent-settings-scale-125");
    harness.state_mut().config.gui.scale = 1.0;
    harness.run_steps(3);
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
    // the strip of hues with the keyboard's focus, a step on
    harness.get_by_label(&fl!("accent-hue")).focus();
    harness.run_steps(2);
    harness.key_press(egui::Key::ArrowRight);
    shot(&mut harness, save, "accent-picker-focus");
}

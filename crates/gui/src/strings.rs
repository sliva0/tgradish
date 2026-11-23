//! Records where each GUI message is shown, for reviewing wording and for
//! translators.
//!
//! `TGRADISH_STRINGS=/tmp/x cargo test -p tgradish-gui -- --ignored strings`
//! steps the GUI through the screenshot states, hovers widgets for tooltips,
//! and writes `window.json` (each message and its cropped screenshots under
//! `pictures/`). `cargo xtask strings` builds an HTML page from that output.
//! Requires a wgpu-capable GPU and ffmpeg; set `TGRADISH_LANG` to render
//! another language.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use eframe::egui::{self, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
use serde::Serialize;

use crate::app::App;
use crate::l10n::record;
use crate::screenshots::walk;

/// Maximum screenshots kept per message.
const PLACES: usize = 2;
/// Padding around a message in its cropped screenshot so surrounding UI is
/// visible.
const AROUND: f32 = 40.0;
/// Maximum widget size to hover; larger nodes are panels or containers
/// without tooltips of their own.
const WIDGET: egui::Vec2 = egui::vec2(420.0, 120.0);
/// Maximum scroll steps per state to reveal off-screen controls.
const SCROLLS: usize = 6;
/// Frames to run before capturing a state so dialog animations finish and
/// widget bounds match their hovered positions.
const SETTLE: usize = 10;

#[test]
#[ignore]
fn strings() {
    let out = PathBuf::from(std::env::var_os("TGRADISH_STRINGS").expect("TGRADISH_STRINGS"));
    let pictures = out.join("pictures");
    let _ = std::fs::remove_dir_all(&pictures);
    std::fs::create_dir_all(&pictures).unwrap();
    crate::l10n::choose(None);
    record::remember();
    let mut found = Found::default();
    walk(|harness, name| found.state(harness, name, &pictures));
    more(|harness, name| found.state(harness, name, &pictures));
    let json = serde_json::to_string_pretty(&found.messages).unwrap();
    std::fs::write(out.join("window.json"), json).unwrap();
}

/// Extra GUI states not covered by `screenshots`, for the messages they
/// expose: notices, active queue, failed/cancelled conversions, emoji targets,
/// expanded advanced sections, WebM/image inspection, long and slowed videos,
/// static pixel art, checked/saved settings, and invalid preset names.
fn more(mut save: impl FnMut(&mut Harness<App>, &str)) {
    use crate::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::options::Spoof;
    use tgradish_core::presets::{Format, Presets};
    use tgradish_core::telegram::Target;

    use crate::item::Conversion;
    use crate::jobs::{Attempt, Job, Status, WebmPlan, failure};
    use crate::l10n::fl;
    use crate::testing::{convert, finish, square, video, video_of};

    let dir = tempfile::tempdir().unwrap();
    let Some(clip) = video(dir.path(), "640x360") else { return };
    let long_dir = dir.path().join("long");
    std::fs::create_dir(&long_dir).unwrap();
    let Some(long) = video_of(&long_dir, "640x360", 5) else { return };
    let still = dir.path().join("still.png");
    square(&still, [200, 80, 0, 255]);
    let broken = dir.path().join("broken.mp4");
    std::fs::write(&broken, b"not a video").unwrap();
    let presets_dir = dir.path().join("presets");
    let mut config = Config::default();
    config.gui.output_dir = Some(dir.path().join("out"));
    config.ffmpeg.choice = FfmpegChoice::System;
    // Disable smooth scrolling so each wheel step lands immediately before capture.
    config.gui.motion = false;
    let mut harness =
        Harness::builder().with_size([1280.0, 820.0]).wgpu().build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
    harness.state_mut().add(vec![clip.clone(), still, long, broken]);
    finish(&mut harness);
    let id = |harness: &Harness<App>, index: usize| harness.state().items[index].id;
    let select = |harness: &mut Harness<App>, index: usize| {
        let id = harness.state().items[index].id;
        harness.state_mut().select(id);
        if let Some(item) = harness.state_mut().items.get_mut(index) {
            item.view.playing = false;
        }
        harness.run_steps(3);
    };

    // a notice in the corner over the list
    select(&mut harness, 0);
    harness.state_mut().say(crate::banner::Message::info(fl!("files-gone", count = 2)));
    harness.run_steps(2);
    save(&mut harness, "notice");
    harness.state_mut().banner.rows.clear();

    // the folded settings, open
    for fold in [fl!("commands"), fl!("encoder-options")] {
        harness.get_by_label(&fold).click_accesskit();
        harness.run_steps(2);
    }
    save(&mut harness, "folds");

    // a WebM made, then inspected, and a picture made one and inspected
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    let made = |harness: &Harness<App>, index: usize| {
        let item = &harness.state().items[index];
        let done = item.made.as_ref().map(|made| &made.job.status);
        match done {
            Some(Status::Done(done)) => Some(done.output.clone()),
            _ => None,
        }
    };
    if let Some(output) = made(&harness, 0) {
        harness.state_mut().inspection = Some(crate::inspect::Inspection::of(&output));
        save(&mut harness, "inspect-webm");
        harness.state_mut().inspection = None;
    }
    select(&mut harness, 1);
    harness.state_mut().items[1].format = Format::Webp;
    harness.run_steps(2);
    convert(&harness);
    harness.run_steps(2);
    finish(&mut harness);
    if let Some(output) = made(&harness, 1) {
        harness.state_mut().inspection = Some(crate::inspect::Inspection::of(&output));
        save(&mut harness, "inspect-image");
        harness.state_mut().inspection = None;
    }

    // emoji, as a WebM and as a picture
    harness.state_mut().items[1].choices.target = Target::Emoji;
    save(&mut harness, "emoji-image");
    harness.state_mut().items[1].choices.target = Target::Sticker;
    select(&mut harness, 0);
    harness.state_mut().items[0].choices.target = Target::Emoji;
    save(&mut harness, "emoji-webm");
    harness.state_mut().items[0].choices.target = Target::Sticker;

    // the picture read as pixel art: its sprite sheet
    select(&mut harness, 1);
    harness.state_mut().items[1].format = Format::Tgs;
    finish(&mut harness);
    save(&mut harness, "tgs-picture");
    harness.state_mut().items[1].format = Format::Webp;

    // the long video: spoofed, cut at 3 s, and slowed down
    select(&mut harness, 2);
    save(&mut harness, "long");
    let choices = &mut harness.state_mut().items[2].choices;
    choices.webm.spoof = Some(Spoof::Never);
    (choices.start, choices.length) = (Some(0.5), Some(4.0));
    save(&mut harness, "long-cut");
    let choices = &mut harness.state_mut().items[2].choices;
    (choices.webm.spoof, choices.webm.tempo) = (None, Some(0.5));
    save(&mut harness, "long-slow");

    // the queue at work: a WebM fitting, its second encode under way, and
    // another file waiting
    let mut running = Job::waiting(dir.path().join("running.webm"));
    running.status = Status::Running;
    running.progress.webm = Some(WebmPlan {
        width: 512,
        height: 288,
        fps: 30.0,
        length: 1.0,
        attempts: 8,
        limit: 256 * 1024,
    });
    let params = |rate| tgradish_core::webm::Params { fps: 30.0, length: 1.0, rate };
    running.progress.attempts = vec![
        Attempt {
            number: 1,
            params: params(tgradish_core::webm::Rate::Bitrate(2400.0)),
            bytes: Some(300_000),
            fits: false,
            ssim: None,
        },
        Attempt {
            number: 2,
            params: params(tgradish_core::webm::Rate::Bitrate(1900.0)),
            bytes: None,
            fits: false,
            ssim: None,
        },
    ];
    running.progress.fraction = Some(0.4);
    let item = &mut harness.state_mut().items[2];
    item.job = Some(Conversion::of(running, Format::Webm, item.choices.clone()));
    let item = &mut harness.state_mut().items[0];
    let waiting = Job::waiting(dir.path().join("waiting.webm"));
    item.job = Some(Conversion::of(waiting, Format::Webm, item.choices.clone()));
    let waiting = id(&harness, 0);
    harness.state_mut().work.queue.push_back(waiting);
    let item = &mut harness.state_mut().items[1];
    let later = Job::waiting(dir.path().join("later.png"));
    item.job = Some(Conversion::of(later, Format::Webp, item.choices.clone()));
    let later = id(&harness, 1);
    harness.state_mut().work.queue.push_back(later);
    select(&mut harness, 2);
    save(&mut harness, "queue");
    select(&mut harness, 0);
    save(&mut harness, "queue-waiting");
    for item in &mut harness.state_mut().items {
        item.job = None;
    }
    harness.state_mut().work.queue.clear();

    // conversions that failed, met a file in the way, or were stopped
    let ended = |status: Status| {
        let mut job = Job::waiting(dir.path().join("ended.webm"));
        job.status = status;
        job
    };
    let nothing = tgradish_core::Error::NothingFits { smallest: 300_000, limit: 262_144 };
    for (name, status) in [
        ("failed", Status::Failed(failure(&nothing))),
        ("in-the-way", Status::Exists(dir.path().join("ended.webm"))),
        ("stopped", Status::Cancelled),
    ] {
        let item = &mut harness.state_mut().items[2];
        let choices = item.choices.clone();
        item.failed = Some(Conversion::of(ended(status), Format::Webm, choices));
        select(&mut harness, 2);
        save(&mut harness, name);
    }
    harness.state_mut().items[2].failed = None;

    // a file that isn't a video
    select(&mut harness, 3);
    save(&mut harness, "unreadable");

    // the settings, ffmpeg checked and a change kept, with no home folder
    harness.state_mut().settings_dialog.open();
    harness.run_steps(10);
    harness.get_by_label(&fl!("check")).click_accesskit();
    harness.run_steps(2);
    harness.get_by_label(&fl!("replace-in-the-way")).click_accesskit();
    harness.run_steps(2);
    save(&mut harness, "settings-checked");
    harness.state_mut().settings_dialog = crate::settings_dialog::SettingsDialog::default();

    // naming a preset: as a built-in one, as one that is there, wrongly
    let mine = (&tgradish_core::webm::Options::default(), &Default::default());
    tgradish_core::presets::save(&presets_dir, "mine", "", mine, false).unwrap();
    harness.state_mut().presets = Presets::load(Some(&presets_dir)).unwrap();
    harness.state_mut().presets_dir = Some(presets_dir);
    select(&mut harness, 0);
    for (name, typed) in
        [("preset-builtin", "fast"), ("preset-exists", "mine"), ("preset-bad", "a/b")]
    {
        crate::preset_dialogs::ask(&harness.ctx, crate::preset_dialogs::Ask::Save(waiting));
        harness.run_steps(3);
        harness.event(egui::Event::Text(typed.into()));
        harness.run_steps(2);
        save(&mut harness, name);
        harness.state_mut().preset_dialog = None;
        harness.run_steps(2);
    }
}

/// Recorded text and screenshot locations for a message ID.
#[derive(Default, Serialize)]
struct Shown {
    text: String,
    places: Vec<Place>,
}

/// Cropped screenshot showing a message: state name, image filename, crop
/// bounds, and highlight rectangles in image pixels.
#[derive(Serialize)]
struct Place {
    state: String,
    picture: String,
    view: [f32; 4],
    marks: Vec<[f32; 4]>,
    /// Exact text rendered here, used to skip duplicate captures of the same
    /// wording.
    #[serde(skip)]
    said: String,
}

/// Message ID, rendered text, and bounding boxes from one frame.
type Seen = (&'static str, String, Vec<Rect>);

#[derive(Default)]
struct Found {
    messages: BTreeMap<String, Shown>,
    pictures: usize,
    /// Messages formatted earlier, indexed by text so retained strings (such
    /// as banners and stored errors) can be matched when drawn later.
    kept: std::collections::HashMap<String, Vec<&'static str>>,
    taken: usize,
}

impl Found {
    /// Captures all visible and tooltip messages in state `name`, scrolling
    /// down until no new message IDs appear (at most [`SCROLLS`] times).
    fn state(&mut self, harness: &mut Harness<App>, name: &str, pictures: &Path) {
        // Show tooltips immediately on pointer move.
        harness.ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let screen = harness.ctx.content_rect();
        // Let dialog opening animations settle.
        harness.run_steps(SETTLE);
        let mut known = BTreeSet::new();
        for _ in 0..SCROLLS {
            harness.event(egui::Event::PointerGone);
            harness.step();
            let shown = self.shown(harness, true);
            if !shown.iter().any(|(id, ..)| known.insert(*id)) {
                break;
            }
            self.view(harness, name, pictures, shown, &mut known);
            scroll(harness, -screen.height() * 0.6);
        }
        // Scroll back to the top for subsequent states.
        scroll(harness, 100_000.0);
    }

    /// Captures the current scroll position (`shown`) and hovers widgets to
    /// record any tooltips not yet in `known`.
    fn view(
        &mut self,
        harness: &mut Harness<App>,
        name: &str,
        pictures: &Path,
        shown: Vec<Seen>,
        known: &mut BTreeSet<&'static str>,
    ) {
        let screen = harness.ctx.content_rect();
        if self.wanted(name, &shown) {
            self.picture(harness, name, pictures, screen, None, shown);
        }
        // Hover each small visible widget to trigger its tooltip.
        let ppp = harness.ctx.pixels_per_point();
        let widgets: Vec<Rect> = harness
            .query_all(by())
            .filter_map(|node| node.accesskit_node().bounding_box())
            .map(|rect| {
                let point = |x: f64, y: f64| egui::pos2(x as f32 / ppp, y as f32 / ppp);
                Rect::from_min_max(point(rect.x0, rect.y0), point(rect.x1, rect.y1))
            })
            .filter(|rect| {
                rect.is_positive()
                    && rect.width() <= WIDGET.x
                    && rect.height() <= WIDGET.y
                    && screen.contains(rect.center())
            })
            .collect();
        for widget in widgets {
            harness.event(egui::Event::PointerMoved(widget.center()));
            harness.step();
            let shown: Vec<Seen> = self
                .shown(harness, false)
                .into_iter()
                .filter(|(id, ..)| known.insert(*id))
                .collect();
            if self.wanted(name, &shown) {
                self.picture(harness, name, pictures, screen, Some(widget), shown);
            }
            harness.event(egui::Event::PointerGone);
            harness.step();
        }
    }

    /// Renders and saves a screenshot for `shown` (either the full window or
    /// the region around a hovered `widget` and its tooltip).
    fn picture(
        &mut self,
        harness: &mut Harness<App>,
        name: &str,
        pictures: &Path,
        screen: Rect,
        widget: Option<Rect>,
        shown: Vec<Seen>,
    ) {
        self.pictures += 1;
        let file = format!("{name}-{}.png", self.pictures);
        let image = harness.render().expect("rendering needs a GPU");
        let ppp = harness.ctx.pixels_per_point();
        let rects = || shown.iter().flat_map(|(.., rects)| rects.iter().copied());
        let part = match widget {
            Some(widget) => rects().fold(widget, Rect::union).expand(AROUND).intersect(screen),
            None => screen,
        };
        let pixels = |value: f32| (value * ppp).round().max(0.0) as u32;
        let cut = image::imageops::crop_imm(
            &image,
            pixels(part.min.x),
            pixels(part.min.y),
            pixels(part.width()),
            pixels(part.height()),
        )
        .to_image();
        cut.save(pictures.join(&file)).unwrap();
        let offset = part.min.to_vec2();
        let bounds = Rect::from_min_size(egui::Pos2::ZERO, part.size());
        for (id, text, rects) in shown {
            let rects: Vec<Rect> = rects.iter().map(|rect| rect.translate(-offset)).collect();
            // A hover's picture shows all of its crop; a whole-window picture
            // shows the message and its surroundings.
            let view = match widget {
                Some(_) => bounds,
                None => rects.iter().fold(Rect::NOTHING, |all, rect| all.union(*rect)),
            };
            let view = view.expand(if widget.is_some() { 0.0 } else { AROUND }).intersect(bounds);
            self.place(id, text, name, &file, view, &rects, ppp);
        }
    }

    /// Returns messages rendered in the next frame along with their bounding
    /// boxes. When `kept` is true, also matches previously formatted strings
    /// (such as banners and stored errors) against painted text galleys.
    fn shown(&mut self, harness: &mut Harness<App>, kept: bool) -> Vec<Seen> {
        record::start();
        harness.step();
        let said = record::take();
        let shapes = &harness.output().shapes;
        let mut seen = BTreeSet::new();
        let mut shown: Vec<Seen> = said
            .into_iter()
            .filter(|(id, text)| !text.trim().is_empty() && seen.insert((*id, text.clone())))
            .filter_map(|(id, text)| {
                let rects = drawn(&text, shapes);
                (!rects.is_empty()).then_some((id, text, rects))
            })
            .collect();
        if kept {
            let new = record::since(self.taken);
            self.taken += new.len();
            for (id, text) in new {
                self.kept.entry(text).or_default().push(id);
            }
            // Match each painted galley as a whole, after a leading symbol, or
            // after a colon.
            for (text, rect) in galleys(shapes) {
                let after_mark = text
                    .split_once(' ')
                    .filter(|(mark, _)| !mark.chars().any(char::is_alphanumeric))
                    .map(|(_, rest)| rest);
                // Check after both the first and last colon because outer
                // messages may wrap inner ones.
                let first = text.split_once(": ").map(|(_, rest)| rest);
                let last = text.rsplit_once(": ").map(|(_, rest)| rest);
                let parts = [Some(text.as_str()), after_mark, first, last];
                for part in parts.into_iter().flatten() {
                    for &id in self.kept.get(part).into_iter().flatten() {
                        if seen.insert((id, part.to_owned())) {
                            shown.push((id, part.to_owned(), vec![rect]));
                        }
                    }
                }
            }
        }
        shown
    }

    /// Returns true if any message in `shown` still needs a screenshot from
    /// state `name`.
    fn wanted(&self, name: &str, shown: &[Seen]) -> bool {
        shown.iter().any(|(id, text, _)| self.wants(id, text, name))
    }

    /// Returns true if `id` with `text` still needs a capture from state
    /// `name`: at most [`PLACES`] captures in total, at most one per state,
    /// and never two with identical wording.
    fn wants(&self, id: &str, text: &str, name: &str) -> bool {
        self.messages.get(id).is_none_or(|message| {
            message.places.len() < PLACES
                && message.places.iter().all(|place| place.state != name && place.said != text)
        })
    }

    /// Records that `id` rendered `text` at `rects` in `picture` of state
    /// `name` (with viewport `view` in points), unless already satisfied.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        id: &str,
        text: String,
        name: &str,
        picture: &str,
        view: Rect,
        rects: &[Rect],
        ppp: f32,
    ) {
        if !self.wants(id, &text, name) {
            return;
        }
        let pixels = |rect: Rect| {
            let rect = Rect::from_min_max(rect.min * ppp, rect.max * ppp);
            [rect.min.x, rect.min.y, rect.width(), rect.height()]
        };
        let message = self.messages.entry(id.to_owned()).or_default();
        message.text = text.clone();
        message.places.push(Place {
            state: name.to_owned(),
            picture: picture.to_owned(),
            view: pixels(view),
            marks: rects.iter().map(|rect| pixels(*rect)).collect(),
            said: text,
        });
    }
}

/// Scrolls the lower settings area (or open dialog) and the side column by
/// `delta` points (positive scrolls up).
fn scroll(harness: &mut Harness<App>, delta: f32) {
    let screen = harness.ctx.content_rect();
    let settings = egui::pos2(screen.center().x + 120.0, screen.max.y - 160.0);
    let side = egui::pos2(screen.max.x - 120.0, screen.min.y + 300.0);
    for at in [settings, side] {
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, delta),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.step();
    }
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
}

/// Finds where `shapes` draw `text`: either an entire galley, or (for strings
/// of at least 8 characters) a substring bounded only by symbols, numbers, or
/// units rather than other words. Short words must match a whole galley so
/// they do not match inside longer messages.
fn drawn(text: &str, shapes: &[egui::epaint::ClippedShape]) -> Vec<Rect> {
    fn walk(text: &str, shape: &egui::Shape, clip: Rect, found: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(text, shape, clip, found);
                }
            }
            egui::Shape::Text(shape) => {
                let galley = &shape.galley;
                let whole = galley.text();
                let rect = if whole == text {
                    Some(galley.rect)
                } else if text.chars().count() >= 8
                    && let Some(at) = whole.find(text)
                    && !wordy(&whole[..at])
                    && !wordy(&whole[at + text.len()..])
                {
                    let start = whole[..at].chars().count();
                    let end = start + text.chars().count();
                    let first = galley.pos_from_cursor(egui::text::CCursor::new(start));
                    let last = galley.pos_from_cursor(egui::text::CCursor::new(end));
                    // Single-line substring: use its span; multi-line: use the whole galley.
                    Some(if (first.min.y - last.min.y).abs() < 1.0 {
                        Rect::from_min_max(first.min, last.max)
                    } else {
                        galley.rect
                    })
                } else {
                    None
                };
                if let Some(rect) = rect.map(|rect| rect.translate(shape.pos.to_vec2()))
                    && rect.intersects(clip)
                {
                    found.push(rect.intersect(clip));
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in shapes {
        walk(text, &shape.shape, shape.clip_rect, &mut found);
    }
    found
}

/// Returns every visible text galley in `shapes` and its clipped bounding box.
fn galleys(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Rect)> {
    fn walk(shape: &egui::Shape, clip: Rect, found: &mut Vec<(String, Rect)>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, clip, found);
                }
            }
            egui::Shape::Text(shape) => {
                let rect = shape.galley.rect.translate(shape.pos.to_vec2());
                if rect.intersects(clip) && !shape.galley.text().trim().is_empty() {
                    found.push((shape.galley.text().to_owned(), rect.intersect(clip)));
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in shapes {
        walk(&shape.shape, shape.clip_rect, &mut found);
    }
    found
}

/// Returns true if `text` contains at least three consecutive letters.
fn wordy(text: &str) -> bool {
    let mut run = 0;
    text.chars().any(|c| {
        run = if c.is_alphabetic() { run + 1 } else { 0 };
        run >= 3
    })
}

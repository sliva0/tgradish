//! Where each of the window's messages is shown, for reviewing their words
//! and for translators: `TGRADISH_STRINGS=/tmp/x cargo test -p tgradish-gui
//! -- --ignored strings` takes the window through the screenshots' states,
//! hovers whatever might say more, and writes `window.json`: what each
//! message said and where, in pictures of the window under `pictures`.
//! `cargo xtask strings` makes a page of it. Needs a GPU wgpu can use, and
//! ffmpeg; `TGRADISH_LANG` shows another language.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use eframe::egui::{self, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
use serde::Serialize;

use crate::app::App;
use crate::l10n::record;
use crate::screenshots::walk;

/// Pictures kept of each message, at most.
const PLACES: usize = 2;
/// Room around a message in its picture, so its surroundings show.
const AROUND: f32 = 40.0;
/// Largest a widget can be to be hovered: larger are panels and areas,
/// whose hovers say nothing of their own.
const WIDGET: egui::Vec2 = egui::vec2(420.0, 120.0);
/// Most times a state is scrolled down for more of it to show.
const SCROLLS: usize = 6;
/// Frames for a state to settle: for dialogs to finish moving into place,
/// so that hovering finds their widgets where they were.
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

/// States of the window the screenshots leave out, for the messages they
/// show: a notice, the queue at work, conversions that failed or were
/// stopped, emoji, the folded settings open, other stickers inspected,
/// long and slowed videos, a picture read as pixel art, settings checked
/// and saved, and preset names that won't do.
fn more(mut save: impl FnMut(&mut Harness<App>, &str)) {
    use tgradish_core::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::options::Spoof;
    use tgradish_core::presets::{Format, Presets};
    use tgradish_core::telegram::Target;

    use crate::item::Made;
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
    let mut harness =
        Harness::builder().with_size([1280.0, 820.0]).wgpu().build_eframe(move |cc| {
            let app = App::with(config.clone(), None, Presets::builtin(), None);
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

    // a notice over the bottom of the window
    select(&mut harness, 0);
    harness.state_mut().message = Some(fl!("files-gone", count = 2));
    save(&mut harness, "notice");
    harness.state_mut().message = None;

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
    harness.state_mut().items[1].format = Format::Image;
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
    harness.state_mut().items[1].format = Format::Image;

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
    let params = |rate| tgradish_core::events::Params { fps: 30.0, length: 1.0, rate };
    running.progress.attempts = vec![
        Attempt {
            number: 1,
            params: params(tgradish_core::events::Rate::Bitrate(2400.0)),
            bytes: Some(300_000),
            fits: false,
            ssim: None,
        },
        Attempt {
            number: 2,
            params: params(tgradish_core::events::Rate::Bitrate(1900.0)),
            bytes: None,
            fits: false,
            ssim: None,
        },
    ];
    running.progress.fraction = Some(0.4);
    let item = &mut harness.state_mut().items[2];
    item.job = Some((running, Format::Webm, item.choices.clone()));
    let item = &mut harness.state_mut().items[0];
    item.job =
        Some((Job::waiting(dir.path().join("waiting.webm")), Format::Webm, item.choices.clone()));
    let waiting = id(&harness, 0);
    harness.state_mut().queue.push_back(waiting);
    let item = &mut harness.state_mut().items[1];
    item.job =
        Some((Job::waiting(dir.path().join("later.png")), Format::Image, item.choices.clone()));
    let later = id(&harness, 1);
    harness.state_mut().queue.push_back(later);
    select(&mut harness, 2);
    save(&mut harness, "queue");
    select(&mut harness, 0);
    save(&mut harness, "queue-waiting");
    for item in &mut harness.state_mut().items {
        item.job = None;
    }
    harness.state_mut().queue.clear();

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
        item.failed = Some(Made { job: ended(status), format: Format::Webm, choices });
        select(&mut harness, 2);
        save(&mut harness, name);
    }
    harness.state_mut().items[2].failed = None;

    // a file that isn't a video
    select(&mut harness, 3);
    save(&mut harness, "unreadable");

    // the settings, ffmpeg checked and saving tried, with no home folder
    harness.state_mut().prefs.open();
    harness.run_steps(10);
    harness.get_by_label(&fl!("check")).click_accesskit();
    harness.run_steps(2);
    harness.get_by_label(&fl!("save")).click_accesskit();
    harness.run_steps(2);
    save(&mut harness, "settings-checked");
    harness.state_mut().prefs = crate::prefs::Prefs::default();

    // naming a preset: as a built-in one, as one that is there, wrongly
    let mine = (&tgradish_core::options::Options::default(), &Default::default());
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

/// What a message said, and where.
#[derive(Default, Serialize)]
struct Shown {
    text: String,
    places: Vec<Place>,
}

/// A part of a picture where a message shows: the state of the window it
/// is of, the part to look at, and the message's outlines in it, in the
/// picture's pixels.
#[derive(Serialize)]
struct Place {
    state: String,
    picture: String,
    view: [f32; 4],
    marks: Vec<[f32; 4]>,
    /// What it said there: another picture of the same words is no more
    /// use.
    #[serde(skip)]
    said: String,
}

/// A message the window showed, with where it drew it.
type Seen = (&'static str, String, Vec<Rect>);

#[derive(Default)]
struct Found {
    messages: BTreeMap<String, Shown>,
    pictures: usize,
    /// The messages given so far, by what they say, for finding those the
    /// window keeps and shows later; and how many of them are in it.
    kept: std::collections::HashMap<String, Vec<&'static str>>,
    taken: usize,
}

impl Found {
    /// Notes the messages `harness` shows in the state `name`, scrolled
    /// down as far as more show, and those its widgets show when hovered,
    /// with pictures of where they are.
    fn state(&mut self, harness: &mut Harness<App>, name: &str, pictures: &Path) {
        // hovers at once, as the pointer moves on
        harness.ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let screen = harness.ctx.content_rect();
        // dialogs ease into place as they open
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
        // back to the top, for the states after this one
        scroll(harness, 100_000.0);
    }

    /// Notes what the window shows as it is scrolled now, `shown`, and
    /// what its widgets say when hovered that isn't `known` yet.
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
        // what each widget says on hover
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

    /// Saves a picture of the window for `shown`, of all of it, or of the
    /// part around a hovered `widget` and what it says.
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
            // a hover's part shows all of it; a whole window, the message's
            // surroundings
            let view = match widget {
                Some(_) => bounds,
                None => rects.iter().fold(Rect::NOTHING, |all, rect| all.union(*rect)),
            };
            let view = view.expand(if widget.is_some() { 0.0 } else { AROUND }).intersect(bounds);
            self.place(id, text, name, &file, view, &rects, ppp);
        }
    }

    /// The messages the last frame showed, with where it drew them: those
    /// it gave, and with `kept`, any given before that it shows as kept,
    /// as notices and errors.
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
            // each text drawn, as a whole, after a mark, or after a colon
            for (text, rect) in galleys(shapes) {
                let after_mark = text
                    .split_once(' ')
                    .filter(|(mark, _)| !mark.chars().any(char::is_alphanumeric))
                    .map(|(_, rest)| rest);
                // after the first colon or the last, as messages hold others
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

    /// Whether any of `shown` still wants a picture of the state `name`.
    fn wanted(&self, name: &str, shown: &[Seen]) -> bool {
        shown.iter().any(|(id, text, _)| self.wants(id, text, name))
    }

    /// Whether `id` saying `text` wants a picture of the state `name`: one
    /// of each state at most, a few in all, and another only where it says
    /// something else.
    fn wants(&self, id: &str, text: &str, name: &str) -> bool {
        self.messages.get(id).is_none_or(|message| {
            message.places.len() < PLACES
                && message.places.iter().all(|place| place.state != name && place.said != text)
        })
    }

    /// Notes that `id` said `text` at `rects` in `picture` of the state
    /// `name`, whose part `view` shows it, in points; unless a picture of
    /// it saying that is kept already.
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

/// Scrolls the settings under the preview, or a dialog's, and the column
/// beside the preview by `delta`, up where positive.
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

/// Where `shapes` draw `text`: the text of a whole galley, or, for longer
/// messages, the part of one that it is, beside marks, numbers and units
/// rather than other words. Short words only count whole, as they are in
/// longer ones too.
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
                    // on one row, just its part; across rows, all of it
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

/// Each text `shapes` draw, with where, as far as it shows.
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

/// Whether `text` has words in it: three letters in a row.
fn wordy(text: &str) -> bool {
    let mut run = 0;
    text.chars().any(|c| {
        run = if c.is_alphabetic() { run + 1 } else { 0 };
        run >= 3
    })
}

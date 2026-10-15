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
    let mut found = Found::default();
    walk(|harness, name| found.state(harness, name, &pictures));
    let json = serde_json::to_string_pretty(&found.messages).unwrap();
    std::fs::write(out.join("window.json"), json).unwrap();
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
}

/// A message the window showed, with where it drew it.
type Seen = (&'static str, String, Vec<Rect>);

#[derive(Default)]
struct Found {
    messages: BTreeMap<String, Shown>,
    pictures: usize,
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
        // over the settings under the preview, or a dialog's
        let wheel = egui::pos2(screen.center().x + 120.0, screen.max.y - 160.0);
        // dialogs ease into place as they open
        harness.run_steps(SETTLE);
        let mut known = BTreeSet::new();
        for _ in 0..SCROLLS {
            harness.event(egui::Event::PointerGone);
            harness.step();
            let shown = self.shown(harness);
            if !shown.iter().any(|(id, ..)| known.insert(*id)) {
                break;
            }
            self.view(harness, name, pictures, shown, &mut known);
            harness.event(egui::Event::PointerMoved(wheel));
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -screen.height() * 0.6),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            });
            harness.run_steps(2);
        }
        // back to the top, for the states after this one
        harness.event(egui::Event::PointerMoved(wheel));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 100_000.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.event(egui::Event::PointerGone);
        harness.run_steps(2);
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
            let shown: Vec<Seen> =
                self.shown(harness).into_iter().filter(|(id, ..)| known.insert(*id)).collect();
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

    /// The messages the last frame showed, with where it drew them.
    fn shown(&self, harness: &mut Harness<App>) -> Vec<Seen> {
        record::start();
        harness.step();
        let said = record::take();
        let shapes = &harness.output().shapes;
        let mut seen = BTreeSet::new();
        said.into_iter()
            .filter(|(id, text)| !text.trim().is_empty() && seen.insert((*id, text.clone())))
            .filter_map(|(id, text)| {
                let rects = drawn(&text, shapes);
                (!rects.is_empty()).then_some((id, text, rects))
            })
            .collect()
    }

    /// Whether any of `shown` still wants a picture of the state `name`.
    fn wanted(&self, name: &str, shown: &[Seen]) -> bool {
        shown.iter().any(|(id, ..)| self.wants(id, name))
    }

    /// Whether `id` wants a picture of the state `name`: one of each state
    /// at most, and a few in all.
    fn wants(&self, id: &str, name: &str) -> bool {
        self.messages.get(id).is_none_or(|message| {
            message.places.len() < PLACES && message.places.iter().all(|place| place.state != name)
        })
    }

    /// Notes that `id` said `text` at `rects` in `picture` of the state
    /// `name`, whose part `view` shows it, in points.
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
        if !self.wants(id, name) {
            return;
        }
        let pixels = |rect: Rect| {
            let rect = Rect::from_min_max(rect.min * ppp, rect.max * ppp);
            [rect.min.x, rect.min.y, rect.width(), rect.height()]
        };
        let message = self.messages.entry(id.to_owned()).or_default();
        message.text = text;
        message.places.push(Place {
            state: name.to_owned(),
            picture: picture.to_owned(),
            view: pixels(view),
            marks: rects.iter().map(|rect| pixels(*rect)).collect(),
        });
    }
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

/// Whether `text` has words in it: three letters in a row.
fn wordy(text: &str) -> bool {
    let mut run = 0;
    text.chars().any(|c| {
        run = if c.is_alphabetic() { run + 1 } else { 0 };
        run >= 3
    })
}

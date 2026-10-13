//! The big preview: the input with its crop, which is edited by dragging,
//! or the result. The wheel zooms, the right or middle button pans.

use eframe::egui::{
    self, Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2,
};
use tgradish_core::config::Corner;
use tgradish_core::options::{Crop, ExactScale};
use tgradish_core::telegram::Target;

use crate::item::Zoom;
use crate::media::Clip;
use crate::{theme, widgets};

/// The one texture the preview draws, and which frame it holds.
#[derive(Default)]
pub struct Screen {
    texture: Option<egui::TextureHandle>,
    /// Clip, frame and whether it is drawn sharp.
    shows: Option<(u64, usize, bool)>,
}

impl Screen {
    /// The clip whose frame is shown.
    #[cfg(test)]
    pub fn showing(&self) -> Option<u64> {
        self.shows.map(|(clip, ..)| clip)
    }

    /// The frame `index` of `clip`, drawn sharp or smooth.
    fn texture(
        &mut self,
        ctx: &egui::Context,
        clip: &Clip,
        index: usize,
        sharp: bool,
    ) -> egui::TextureId {
        let options =
            if sharp { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
        if self.shows != Some((clip.id, index, sharp)) || self.texture.is_none() {
            let image = clip.image(index);
            match &mut self.texture {
                Some(texture) => texture.set(image, options),
                None => self.texture = Some(ctx.load_texture("preview", image, options)),
            }
            self.shows = Some((clip.id, index, sharp));
        }
        self.texture.as_ref().expect("set above").id()
    }
}

/// The small picture of what the result will look like, beside the input
/// or over its corner.
#[derive(Default)]
pub struct Inset {
    texture: Option<egui::TextureHandle>,
    /// What the texture shows, whether it is drawn sharp, and the most
    /// pixels along a side it was made with.
    key: Option<(u64, bool, usize)>,
    /// Whether a part of a clip looks like pixel art, by their key.
    art: Option<(u64, bool)>,
    /// What a `.tgs` result shows of a clip, by its key.
    bounds: Option<(u64, [usize; 4])>,
    /// The layout drawn last: the item's, and what it was made of.
    drawn: Option<(u64, Arrangement, Layout)>,
    /// A change of layout being eased in: where it started, and when.
    motion: Option<(Layout, f64)>,
    /// How far the small picture is dragged, while it is.
    dragged: Option<Vec2>,
    /// The room the small picture was let go of in, to ease from there.
    dropped: Option<Rect>,
}

/// What the layout of the input and the small picture is made of: whether
/// the small picture shows, its size and its corner.
pub type Arrangement = (bool, usize, Corner);

/// Where things go in the preview: the area the input fits, the small
/// picture's room, and where in it the small picture is, from 0 to 1
/// across and down, which its corner says.
pub type Layout = (Rect, Rect, Vec2);

/// What the small picture's controls asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsetChange {
    Size(usize),
    Corner(Corner),
}

/// How long a change of layout eases in, in seconds.
const EASING: f64 = 0.15;

impl Inset {
    /// Whether the part of a clip `key` names looks like pixel art, worked
    /// out by `find` when the key changes rather than every frame.
    pub fn looks_like_art(&mut self, key: u64, find: impl FnOnce() -> bool) -> bool {
        match self.art {
            Some((seen, art)) if seen == key => art,
            _ => self.art.insert((key, find())).1,
        }
    }

    /// What a `.tgs` result shows of the clip `key` names, worked out by
    /// `find` when the key changes rather than every frame.
    pub fn tgs_bounds(&mut self, key: u64, find: impl FnOnce() -> [usize; 4]) -> [usize; 4] {
        match self.bounds {
            Some((seen, bounds)) if seen == key => bounds,
            _ => self.bounds.insert((key, find())).1,
        }
    }

    /// The layout to draw for `item` now: the input's area and the small
    /// picture's room, and its place in that as its corner says, eased in
    /// from what was drawn before when what it is made of changed or the
    /// small picture was let go of, rather than with the window's size.
    pub fn ease(
        &mut self,
        ctx: &egui::Context,
        item: u64,
        arrangement: Arrangement,
        (input, room): (Rect, Rect),
    ) -> Layout {
        let now = ctx.input(|input| input.time);
        let corner = arrangement.2;
        let layout = (input, room, vec2(!corner.left() as u8 as f32, !corner.top() as u8 as f32));
        let dropped = self.dropped.take();
        match self.drawn {
            Some((seen, before, (input, room, align)))
                if seen == item && (before != arrangement || dropped.is_some()) =>
            {
                self.motion = Some(((input, dropped.unwrap_or(room), align), now));
            }
            Some((seen, ..)) if seen != item => self.motion = None,
            _ => {}
        }
        let drawn = match self.motion {
            Some((from, since)) if now - since < EASING => {
                ctx.request_repaint();
                // fast, then settling
                let t = (1.0 - (1.0 - (now - since) / EASING).powi(3)) as f32;
                (
                    lerp(from.0, layout.0, t),
                    lerp(from.1, layout.1, t),
                    from.2 + (layout.2 - from.2) * t,
                )
            }
            _ => {
                self.motion = None;
                layout
            }
        };
        self.drawn = Some((item, arrangement, drawn));
        drawn
    }

    /// Whether the small picture is still easing out of sight.
    pub fn leaving(&self) -> bool {
        self.motion.is_some() && self.drawn.is_some_and(|(_, (on, ..), _)| !on)
    }

    /// Shows the picture `make` makes of a `full` sized result, made again
    /// only when `key` changes, as large as fits `room`, at `align` in it,
    /// labelled as the result, with `details` under that and then how
    /// large it shows. `make` is told the most pixels along a side the
    /// picture shows with, which it needn't make more of. With `controls`,
    /// buttons on the label change its `size`, one of [`INSET_SIZES`], and
    /// dragging it or its label to another corner of `view` than its
    /// `corner` moves it there.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        (view, room, align): (Rect, Rect, Vec2),
        (corner, size, controls): (Corner, usize, bool),
        key: u64,
        details: Option<&str>,
        full: Vec2,
        make: impl FnOnce(usize) -> egui::ColorImage,
    ) -> Option<InsetChange> {
        let ppp = ui.ctx().pixels_per_point();
        let room_size = room.size().max(Vec2::splat(1.0));
        let fits = |size: Vec2| (room_size.x / size.x).min(room_size.y / size.y).min(1.0 / ppp);
        let full = full.max(Vec2::splat(1.0));
        let shown = full * fits(full);
        // as many pixels as show, and no more than the result has
        let most = ((shown.max_elem() * ppp).ceil() as usize).max(1);
        let sharp = fits(full) * ppp >= 1.0;
        if self.key != Some((key, sharp, most)) || self.texture.is_none() {
            let options =
                if sharp { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
            let image = make(most);
            match &mut self.texture {
                Some(texture) => texture.set(image, options),
                None => self.texture = Some(ui.ctx().load_texture("inset", image, options)),
            }
            self.key = Some((key, sharp, most));
        }
        let texture = self.texture.as_ref().expect("set above");
        let docked = Rect::from_min_size(room.min + (room.size() - shown) * align, shown);
        let painter = ui.painter_at(view);
        // a result's size, and how much of a screen pixel each of its
        // pixels takes
        let label = match details {
            Some(details) => {
                let scale = (fits(full) * ppp * 100.0).round();
                format!("Result at {scale}%\n{details}")
            }
            None => "Result".to_owned(),
        };
        let galley = painter.layout_no_wrap(label, INSET_LABEL, Color32::PLACEHOLDER);
        let boxed = label_box(galley.size());
        // the label over the picture's left edge, or further left where it
        // would stick out
        let label_at = |frame: Rect| {
            let left = frame.left().min(view.right() - MARGIN - boxed.x).max(view.left() + 3.0);
            pos2(left, frame.top() - boxed.y - 8.0)
        };
        // the size buttons at the end of the label at `at`
        let buttons_at = |at: Pos2| {
            Rect::from_min_max(
                pos2(at.x + boxed.x - SIZE_BUTTONS, at.y),
                pos2(at.x + boxed.x, at.y + boxed.y),
            )
        };

        // dragged by the picture or its label, which keep the crop under
        // them from being dragged, towards another corner, which the
        // pointer's quarter of the view picks
        let mut change = None;
        let mut frame = docked.translate(self.dragged.unwrap_or_default());
        let mut dragging = false;
        if controls {
            let id = ui.id().with("inset");
            let at = label_at(frame);
            let behind = Rect::from_min_size(at, boxed).expand(3.0);
            // the buttons are buttons, until a drag starts on them
            let on_buttons = ui.rect_contains_pointer(buttons_at(at));
            let label = ui.interact(behind, id.with("label"), Sense::drag());
            let held = [
                ui.interact(frame.expand(4.0), id, Sense::drag()),
                if on_buttons { label } else { label.on_hover_text("Drag it to another corner") },
            ];
            let pointer = ui.input(|input| input.pointer.latest_pos());
            let origin = ui.input(|input| input.pointer.press_origin());
            // once the pointer has moved, so pressing a button on the
            // label is only that
            let moved = ui.input(|input| input.pointer.is_decidedly_dragging());
            if held.iter().any(|held| held.dragged())
                && moved
                && let (Some(origin), Some(pointer)) = (origin, pointer)
            {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                self.dragged = Some(pointer - origin);
                frame = docked.translate(pointer - origin);
                dragging = true;
            } else if held.iter().any(|held| held.hovered()) && !on_buttons {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
            // let go of: eased from where it was to its corner, the same
            // one or another
            if held.iter().any(|held| held.drag_stopped())
                && let Some(by) = self.dragged.take()
            {
                self.dropped = Some(room.translate(by));
                let to = pointer.map_or(corner, |pointer| {
                    Corner::at(pointer.x < view.center().x, pointer.y < view.center().y)
                });
                if to != corner {
                    change = Some(InsetChange::Corner(to));
                }
            }
        }

        let (back, text) = if ui.visuals().dark_mode {
            (Color32::from_black_alpha(200), Color32::WHITE)
        } else {
            (Color32::from_white_alpha(230), Color32::from_gray(20))
        };
        let edge = ui.visuals().widgets.noninteractive.bg_stroke;
        if dragging {
            // the corners it can go to, under its label at the top
            let stroke = Stroke::new(1.0, edge.color.gamma_multiply(0.8));
            for corner in
                [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight]
            {
                let x = if corner.left() { view.left() + MARGIN } else { view.right() - MARGIN };
                let y = if corner.top() {
                    view.top() + MARGIN + boxed.y + LABEL_GAP
                } else {
                    view.bottom() - MARGIN
                };
                let at = pos2(if corner.left() { x } else { x - shown.x }, {
                    if corner.top() { y } else { y - shown.y }
                });
                let target = Rect::from_min_size(at, shown).expand(4.0);
                painter.rect_stroke(target, 4.0, stroke, egui::StrokeKind::Outside);
            }
        }
        painter.rect(frame.expand(4.0), 4.0, back, edge, egui::StrokeKind::Outside);
        widgets::checkerboard(&painter, frame, 6.0);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(texture.id(), frame, uv, Color32::WHITE);
        if !controls {
            return change;
        }

        // the label, and the buttons that change the size after it
        let at = label_at(frame);
        let behind = Rect::from_min_size(at, boxed).expand(3.0);
        painter.rect(behind, 3.0, back, edge, egui::StrokeKind::Outside);
        // a grip, for dragging it by
        for row in 0..3 {
            for column in 0..2 {
                let dot = pos2(
                    at.x + 2.0 + 4.0 * column as f32,
                    at.y + boxed.y / 2.0 + 4.0 * (row - 1) as f32,
                );
                painter.circle_filled(dot, 1.2, text.gamma_multiply(0.6));
            }
        }
        let text_at = pos2(at.x + GRIP, at.y + (boxed.y - galley.size().y) / 2.0);
        painter.galley_with_override_text_color(text_at, galley, text);
        let buttons = buttons_at(at);
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(buttons)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        // square, rather than as tall as other buttons
        row.spacing_mut().item_spacing.x = 2.0;
        row.spacing_mut().button_padding = vec2(0.0, 0.0);
        row.spacing_mut().interact_size = Vec2::splat(SIZE_BUTTON);
        let button = |text| egui::Button::new(text).min_size(vec2(SIZE_BUTTON, SIZE_BUTTON));
        let last = INSET_SIZES.len() - 1;
        if row.add_enabled(size > 0, button("−")).on_hover_text("Smaller").clicked() {
            change = Some(InsetChange::Size(size - 1));
        }
        if row.add_enabled(size < last, button("+")).on_hover_text("Larger").clicked() {
            change = Some(InsetChange::Size(size + 1));
        }
        change
    }
}

/// `from` moved `t` of the way to `to`.
fn lerp(from: Rect, to: Rect, t: f32) -> Rect {
    Rect::from_min_max(from.min.lerp(to.min, t), from.max.lerp(to.max, t))
}

/// The font of the label over the result's small picture.
pub const INSET_LABEL: egui::FontId = egui::FontId::proportional(12.0);
/// The sizes of the small picture, as shares of the view's shorter side.
pub const INSET_SIZES: [f32; 5] = [0.20, 0.27, 0.34, 0.42, 0.50];
/// The side of a button that changes its size.
const SIZE_BUTTON: f32 = 24.0;
/// The width the two of them take after the label's text.
const SIZE_BUTTONS: f32 = 2.0 * SIZE_BUTTON + 2.0;
/// The width of the grip before the label's text.
const GRIP: f32 = 12.0;
/// Height the label takes over the small picture, besides its text.
const LABEL_GAP: f32 = 12.0;
/// The frame around the small picture, beyond it.
const FRAME: f32 = 5.0;
/// The smallest the small picture gets beside the input, on its longer
/// side.
const SMALLEST_INSET: f32 = 64.0;
/// The least share of its size the input keeps to make room for it.
const LEAST_SHARE: f32 = 0.75;

/// The size of the label over the small picture, for `text` of that size,
/// the grip before it and the buttons after it.
pub fn label_box(text: Vec2) -> Vec2 {
    vec2(GRIP + text.x + 6.0 + SIZE_BUTTONS, text.y.max(SIZE_BUTTON))
}

/// Where the input and the result's small picture go in `view`. The small
/// picture is `share` of the view's shorter side, one of [`INSET_SIZES`],
/// in `corner`. The input gives up a strip along a side of that corner for
/// it, the side where the input shrinks least, by up to a quarter of its
/// size; a larger small picture goes over the input's corner from there,
/// as it does where the input can't give it [`SMALLEST_INSET`]. Zoomed in,
/// the input grows from where it fits over the strip too. `input` is the
/// input's size in pixels, `label` the size of the small picture's longest
/// label box.
///
/// The room is a square, whatever shape the result has, so changing the
/// crop never moves the input it is changed on. Returns the input's area
/// and the small picture's room.
pub fn split(
    view: Rect,
    input: (u32, u32),
    label: Vec2,
    share: f32,
    corner: Corner,
) -> (Rect, Rect) {
    let label_height = label.y + LABEL_GAP;
    let largest =
        (view.width() - 2.0 * MARGIN - FRAME).min(view.height() - 2.0 * MARGIN - label_height);
    let side = (view.size().min_elem() * share).max(SMALLEST_INSET).min(largest).max(0.0);
    let x = if corner.left() { view.left() + MARGIN } else { view.right() - MARGIN - side };
    let y = if corner.top() {
        view.top() + MARGIN + label_height
    } else {
        view.bottom() - MARGIN - side
    };
    let room = Rect::from_min_size(pos2(x, y), Vec2::splat(side));

    let input = vec2(input.0.max(1) as f32, input.1.max(1) as f32);
    let fit =
        ((view.width() - 2.0 * MARGIN) / input.x).min((view.height() - 2.0 * MARGIN) / input.y);
    if fit <= 0.0 {
        return (view, room);
    }
    let shown = input * fit;
    let smallest = SMALLEST_INSET.min(side);
    // the share of its size the input keeps beside a strip along its
    // length `view`, where it is `shown` long, and what the strip takes;
    // `need` is how long the strip is for a small picture of a size
    let strip = |view: f32, shown: f32, need: &dyn Fn(f32) -> f32, across: bool| {
        let share_for = |side: f32| ((view - 3.0 * MARGIN - need(side)) / shown).min(1.0);
        (across && share_for(smallest) >= LEAST_SHARE).then(|| {
            let share = share_for(side).max(LEAST_SHARE);
            (share, (view - shown * share - 2.0 * MARGIN).min(need(side) + MARGIN))
        })
    };
    let beside = strip(
        view.width(),
        shown.x,
        &|side| side.max(label.x) + FRAME,
        view.height() - 2.0 * MARGIN >= smallest + label_height,
    );
    let across = strip(
        view.height(),
        shown.y,
        &|side| side + label_height + FRAME,
        view.width() - 2.0 * MARGIN >= smallest.max(label.x) + FRAME,
    );
    let input = match (beside, across) {
        (Some((by_side, taken)), Some((by_end, _))) if by_side >= by_end => (true, taken),
        (_, Some((_, taken))) => (false, taken),
        (Some((_, taken)), None) => (true, taken),
        (None, None) => return (view, room),
    };
    let input = match (input, corner.left(), corner.top()) {
        ((true, taken), true, _) => {
            Rect::from_min_max(pos2(view.left() + taken, view.top()), view.max)
        }
        ((true, taken), false, _) => {
            Rect::from_min_size(view.min, vec2(view.width() - taken, view.height()))
        }
        ((false, taken), _, true) => {
            Rect::from_min_max(pos2(view.left(), view.top() + taken), view.max)
        }
        ((false, taken), _, false) => {
            Rect::from_min_size(view.min, vec2(view.width(), view.height() - taken))
        }
    };
    (input, room)
}

/// A frame to show, and the size of what it shows in input pixels: the
/// clip may hold smaller frames.
pub struct Picture<'a> {
    pub clip: &'a Clip,
    pub frame: usize,
    pub size: (u32, u32),
    /// Shown in the middle of a square canvas, as `.tgs` stickers are.
    pub square: bool,
}

/// The crop being edited, with the width to height ratio it keeps.
pub struct Cropping<'a> {
    pub crop: &'a mut Option<Crop>,
    pub ratio: Option<f64>,
    /// The ratio of a box the crop fills, cutting off the rest of it.
    pub filling: Option<f64>,
    /// The size an exact scale holds the crop to.
    pub exact: Option<Exact>,
}

/// Which edge of a crop stays put when its size changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Start,
    End,
    Middle,
}

/// A crop held to an exact scale: its longer side at most `long` input
/// pixels, the most the scale fits into the result, and its sides whole
/// blocks of `step` pixels, which shrinking averages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exact {
    pub long: u32,
    pub step: u32,
}

impl Exact {
    /// The crop `scale` takes for `target`, if a pixel at that scale isn't
    /// larger than the result.
    pub fn of(scale: ExactScale, target: Target) -> Option<Exact> {
        let (width, height) = target.box_size();
        let long = scale.most_input(width.max(height));
        (long > 0).then_some(Exact { long, step: scale.down })
    }

    /// Whether a `size` picture has a whole block.
    pub fn fits(self, size: (u32, u32)) -> bool {
        size.0.min(size.1) >= self.step
    }

    /// `crop` held to this size within a `size` picture: no larger than the
    /// scale takes, in whole blocks, and as `ratio` says; `keep` says which
    /// edges stay put, across and down.
    pub fn fit(
        self,
        crop: Crop,
        keep: (Keep, Keep),
        ratio: Option<f64>,
        size: (u32, u32),
    ) -> Option<Crop> {
        if !self.fits(size) {
            return None;
        }
        let landscape = ratio.map_or(crop.width >= crop.height, |r| r >= 1.0);
        let (room_long, room_short) = if landscape { size } else { (size.1, size.0) };
        let blocks = |side: u32| (side / self.step * self.step).max(self.step);
        // shorter than it could be, where the picture is too narrow for
        // the shape
        let shaped = ratio.map_or(room_long, |r| {
            let r = if landscape { r } else { 1.0 / r };
            (f64::from(room_short) * r).floor() as u32
        });
        let current = if landscape { crop.width } else { crop.height };
        let long = blocks(current.min(self.long).min(room_long).min(shaped));
        let wanted = match ratio {
            Some(r) => f64::from(long) / if landscape { r } else { 1.0 / r },
            None => f64::from(if landscape { crop.height } else { crop.width }),
        };
        let short = blocks((wanted.round() as u32).min(long).min(room_short));
        let (width, height) = if landscape { (long, short) } else { (short, long) };
        let place = |start: u32, old: u32, new: u32, keep: Keep, room: u32| -> u32 {
            let (start, old, new) = (i64::from(start), i64::from(old), i64::from(new));
            let at = match keep {
                Keep::Start => start,
                Keep::End => start + old - new,
                Keep::Middle => start + (old - new) / 2,
            };
            at.clamp(0, i64::from(room) - new) as u32
        };
        Some(Crop {
            x: place(crop.x, crop.width, width, keep.0, size.0),
            y: place(crop.y, crop.height, height, keep.1, size.1),
            width,
            height,
        })
    }
}

/// What dragging on the picture does, decided when it starts.
#[derive(Clone, Copy, Debug)]
enum Drag {
    /// Moves the edges marked: left, top, right, bottom.
    Edges([bool; 4], Crop),
    /// Moves the crop, from where the pointer started, in input pixels.
    Move(Crop, Pos2),
    /// A new crop from this corner, in input pixels.
    New(Pos2),
}

/// Space around the picture, so handles at its edges can be grabbed.
const MARGIN: f32 = 14.0;
/// How near an edge the pointer grabs it, in points.
const GRAB: f32 = 7.0;
const MAX_ZOOM: f32 = 64.0;

/// Where the picture goes in `rect`, and how many points an input pixel
/// takes.
fn layout(rect: Rect, size: (u32, u32), zoom: &Zoom) -> (Rect, f32) {
    let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let scale = fit(rect, size) * zoom.level;
    let shown = vec2(width, height) * scale;
    let min = rect.center() - vec2(zoom.centre.x * shown.x, zoom.centre.y * shown.y);
    (Rect::from_min_size(min, shown), scale)
}

/// How many points an input pixel takes where a `size` picture fits
/// `rect`.
fn fit(rect: Rect, size: (u32, u32)) -> f32 {
    let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let fit = ((rect.width() - 2.0 * MARGIN) / width).min((rect.height() - 2.0 * MARGIN) / height);
    fit.max(1e-3)
}

/// Screen pixels to a picture pixel from which pixels show as squares:
/// a smooth picture blurs them, and you can't put a crop on a pixel's edge.
const SHARP_FROM: f32 = 2.0;

/// Shows `picture` in `rect`, with `cropping` over it if given, which
/// dragging changes. The picture fits `fit_in`, a part of `rect`, and
/// zooms from there, over the rest of `rect`: nothing moves when zooming
/// starts or ends.
pub fn show(
    ui: &mut egui::Ui,
    screen: &mut Screen,
    (rect, fit_in): (Rect, Rect),
    picture: &Picture,
    zoom: &mut Zoom,
    cropping: Option<Cropping>,
) {
    // a part of the space the caller took: taking it again would move
    // what comes next
    let response = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::stage(ui.visuals().dark_mode));

    let side = picture.size.0.max(picture.size.1);
    let canvas = if picture.square { (side, side) } else { picture.size };
    hold(zoom, (rect, fit_in), canvas);
    zoom_and_pan(ui, &response, fit_in, canvas, zoom);
    let (whole, scale) = layout(fit_in, canvas, zoom);
    widgets::checkerboard(&painter, whole, 8.0);
    let shown = Rect::from_center_size(
        whole.center(),
        vec2(picture.size.0 as f32, picture.size.1 as f32) * scale,
    );
    // in the clip's pixels, which may be fewer than the input's
    let clip_scale = scale * picture.size.0 as f32 / picture.clip.width.max(1) as f32;
    let device = clip_scale * ui.ctx().pixels_per_point();
    let sharp = device >= SHARP_FROM || (picture.clip.pixelated && device >= 1.0);
    let texture = screen.texture(ui.ctx(), picture.clip, picture.frame, sharp);
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(texture, shown, uv, Color32::WHITE);

    let Some(cropping) = cropping else { return };
    let size = picture.size;
    let to_pixels = |point: Pos2| {
        let x = ((point.x - shown.min.x) / scale).clamp(0.0, size.0 as f32);
        let y = ((point.y - shown.min.y) / scale).clamp(0.0, size.1 as f32);
        pos2(x, y)
    };
    let full = Crop { x: 0, y: 0, width: size.0, height: size.1 };
    let crop = cropping.crop.unwrap_or(full);
    let on_screen = Rect::from_min_size(
        shown.min + vec2(crop.x as f32, crop.y as f32) * scale,
        vec2(crop.width as f32, crop.height as f32) * scale,
    );

    // what a drag starting at `point` would do
    let grab = |point: Pos2| -> Option<Drag> {
        let near = |a: f32, b: f32| (a - b).abs() <= GRAB;
        let along_x = (on_screen.left() - GRAB..=on_screen.right() + GRAB).contains(&point.x);
        let along_y = (on_screen.top() - GRAB..=on_screen.bottom() + GRAB).contains(&point.y);
        let edges = [
            along_y && near(point.x, on_screen.left()),
            along_x && near(point.y, on_screen.top()),
            along_y && near(point.x, on_screen.right()),
            along_x && near(point.y, on_screen.bottom()),
        ];
        if edges.iter().any(|&edge| edge) {
            Some(Drag::Edges(edges, crop))
        } else if cropping.crop.is_some() && on_screen.contains(point) {
            Some(Drag::Move(crop, to_pixels(point)))
        } else if shown.contains(point) {
            Some(Drag::New(to_pixels(point)))
        } else {
            None
        }
    };

    if let Some(point) = response.hover_pos() {
        let icon = match grab(point) {
            Some(Drag::Edges([true, true, false, false] | [false, false, true, true], _)) => {
                CursorIcon::ResizeNwSe
            }
            Some(Drag::Edges([false, true, true, false] | [true, false, false, true], _)) => {
                CursorIcon::ResizeNeSw
            }
            Some(Drag::Edges([true, false, false, false] | [false, false, true, false], _)) => {
                CursorIcon::ResizeHorizontal
            }
            Some(Drag::Edges(..)) => CursorIcon::ResizeVertical,
            Some(Drag::Move(..)) => CursorIcon::Move,
            Some(Drag::New(_)) => CursorIcon::Crosshair,
            None => CursorIcon::Default,
        };
        ui.ctx().set_cursor_icon(icon);
    }

    let id = response.id.with("drag");
    if response.drag_started_by(PointerButton::Primary)
        && let Some(origin) = ui.input(|input| input.pointer.press_origin())
        && let Some(drag) = grab(origin)
    {
        ui.data_mut(|data| data.insert_temp(id, drag));
    }
    let drag: Option<Drag> = ui.data(|data| data.get_temp(id));
    if let (Some(drag), true) = (drag, response.dragged_by(PointerButton::Primary))
        && let Some(point) = response.interact_pointer_pos()
    {
        let point = to_pixels(point);
        let side = |end: bool| if end { Keep::End } else { Keep::Start };
        let (new, keep) = match drag {
            Drag::Edges(edges, start) => (
                resized(start, edges, point, cropping.ratio, size),
                (side(edges[0]), side(edges[1])),
            ),
            Drag::Move(start, from) => {
                (Some(moved(start, point - from, size)), (Keep::Start, Keep::Start))
            }
            Drag::New(corner) => (
                drawn(corner, point, cropping.ratio, size),
                (side(point.x < corner.x), side(point.y < corner.y)),
            ),
        };
        let new = match cropping.exact {
            Some(exact) => new.and_then(|new| exact.fit(new, keep, cropping.ratio, size)),
            None => new,
        };
        if let Some(new) = new {
            let new = (new != full).then_some(new);
            if new != *cropping.crop {
                *cropping.crop = new;
            }
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<Drag>(id));
    }
    if response.double_clicked() && cropping.crop.is_some() {
        // all of the picture, or as much of it as the exact scale takes
        let middle = (Keep::Middle, Keep::Middle);
        *cropping.crop = match cropping.exact {
            Some(exact) => {
                exact.fit(full, middle, cropping.ratio, size).filter(|crop| *crop != full)
            }
            None => None,
        };
    }

    // the crop as it is now
    let crop = cropping.crop.unwrap_or(full);
    let on_screen = Rect::from_min_size(
        shown.min + vec2(crop.x as f32, crop.y as f32) * scale,
        vec2(crop.width as f32, crop.height as f32) * scale,
    );
    if cropping.crop.is_some() {
        let shade = Color32::from_black_alpha(150);
        let outside = [
            Rect::from_min_max(shown.min, pos2(shown.max.x, on_screen.min.y)),
            Rect::from_min_max(pos2(shown.min.x, on_screen.max.y), shown.max),
            Rect::from_min_max(
                pos2(shown.min.x, on_screen.min.y),
                pos2(on_screen.min.x, on_screen.max.y),
            ),
            Rect::from_min_max(
                pos2(on_screen.max.x, on_screen.min.y),
                pos2(shown.max.x, on_screen.max.y),
            ),
        ];
        for part in outside {
            painter.rect_filled(part, 0.0, shade);
        }
    }
    // what filling the box cuts off
    if let Some(ratio) = cropping.filling.map(|r| r as f32) {
        let (width, height) = (on_screen.width(), on_screen.height());
        let kept = if width / height > ratio {
            vec2(height * ratio, height)
        } else {
            vec2(width, width / ratio)
        };
        let kept = Rect::from_center_size(on_screen.center(), kept);
        let cut = Color32::from_black_alpha(110);
        for part in [
            Rect::from_min_max(on_screen.min, pos2(on_screen.max.x, kept.min.y)),
            Rect::from_min_max(pos2(on_screen.min.x, kept.max.y), on_screen.max),
            Rect::from_min_max(pos2(on_screen.min.x, kept.min.y), pos2(kept.min.x, kept.max.y)),
            Rect::from_min_max(pos2(kept.max.x, kept.min.y), pos2(on_screen.max.x, kept.max.y)),
        ] {
            painter.rect_filled(part, 0.0, cut);
        }
        painter.rect_stroke(
            kept,
            0.0,
            Stroke::new(1.0, Color32::from_white_alpha(140)),
            egui::StrokeKind::Middle,
        );
    }
    let hovered = response.hovered() || response.dragged();
    if cropping.crop.is_some() || hovered {
        let stroke = Stroke::new(1.5, Color32::WHITE);
        painter.rect_stroke(
            on_screen,
            0.0,
            Stroke::new(3.0, Color32::from_black_alpha(120)),
            egui::StrokeKind::Outside,
        );
        painter.rect_stroke(on_screen, 0.0, stroke, egui::StrokeKind::Middle);
        for x in [on_screen.left(), on_screen.center().x, on_screen.right()] {
            for y in [on_screen.top(), on_screen.center().y, on_screen.bottom()] {
                if x == on_screen.center().x && y == on_screen.center().y {
                    continue;
                }
                let handle = Rect::from_center_size(pos2(x, y), vec2(8.0, 8.0));
                painter.rect_filled(handle, 1.0, Color32::WHITE);
                painter.rect_stroke(
                    handle,
                    1.0,
                    Stroke::new(1.0, Color32::from_gray(30)),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
    if cropping.crop.is_some() {
        let text = format!("{} × {}", crop.width, crop.height);
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.5), Color32::WHITE);
        let at = pos2(
            on_screen.left() + 6.0,
            (on_screen.top() - galley.size().y - 8.0).max(rect.top() + 4.0),
        );
        let back = Rect::from_min_size(at, galley.size()).expand(4.0);
        painter.rect_filled(back, 4.0, Color32::from_black_alpha(170));
        painter.galley(at, galley, Color32::WHITE);
    }
}

fn zoom_and_pan(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: Rect,
    size: (u32, u32),
    zoom: &mut Zoom,
) {
    if let Some(pointer) = response.hover_pos() {
        let (scroll, pinch) = ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
        let factor = pinch * (scroll * 0.002).exp();
        if (factor - 1.0).abs() > 1e-4 {
            let (shown, _) = layout(rect, size, zoom);
            let at = (pointer - shown.min) / shown.size();
            let level = (zoom.level * factor).clamp(1.0, MAX_ZOOM);
            let shown = shown.size() * (level / zoom.level);
            // the picture point under the pointer stays there
            let min = pointer - vec2(at.x * shown.x, at.y * shown.y);
            zoom.centre = (rect.center() - min) / shown;
            zoom.level = level;
        }
    }
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        let (shown, _) = layout(rect, size, zoom);
        zoom.centre -= response.drag_delta() / shown.size();
    }
    cover(zoom, rect, size);
}

/// Zoomed in, keeps a `size` picture where it is on the screen when the
/// part `fit_in` of the `view` it fits changes, as for the small picture
/// beside it.
fn hold(zoom: &mut Zoom, (view, fit_in): (Rect, Rect), size: (u32, u32)) {
    if let Some((seen, before)) = zoom.anchor
        && seen == view
        && before != fit_in
        && zoom.level > 1.0
    {
        let (shown, scale) = layout(before, size, zoom);
        zoom.level = (scale / fit(fit_in, size)).clamp(1.0, MAX_ZOOM);
        let (now, _) = layout(fit_in, size, zoom);
        zoom.centre = (fit_in.center() - shown.min) / now.size();
    }
    zoom.anchor = Some((view, fit_in));
}

/// Pans a `size` picture so it covers `rect`, the part it fits, as far as
/// it is large enough: zooming out ends where it fits.
fn cover(zoom: &mut Zoom, rect: Rect, size: (u32, u32)) {
    let (shown, _) = layout(rect, size, zoom);
    let seen = (rect.size() - Vec2::splat(2.0 * MARGIN)) / shown.size();
    let leeway = (Vec2::splat(0.5) - seen * 0.5).max(Vec2::ZERO);
    zoom.centre = zoom.centre.clamp(Vec2::splat(0.5) - leeway, Vec2::splat(0.5) + leeway);
}

/// Edges `[left, top, right, bottom]` as numbers.
type Bounds = [f32; 4];

fn bounds(crop: Crop) -> Bounds {
    let (x, y) = (crop.x as f32, crop.y as f32);
    [x, y, x + crop.width as f32, y + crop.height as f32]
}

/// `bounds` made whole pixels within a `size` picture, at least one pixel
/// wide and high, shrunk to fit while keeping its ratio if `ratio` is given.
fn settle([left, top, right, bottom]: Bounds, ratio: Option<f64>, size: (u32, u32)) -> Crop {
    let (limit_w, limit_h) = (size.0 as f32, size.1 as f32);
    let (mut width, mut height) = (right - left, bottom - top);
    if width > limit_w {
        if ratio.is_some() {
            height *= limit_w / width;
        }
        width = limit_w;
    }
    if height > limit_h {
        if ratio.is_some() {
            width *= limit_h / height;
        }
        height = limit_h;
    }
    let width = width.round().clamp(1.0, limit_w);
    let height = height.round().clamp(1.0, limit_h);
    let x = left.round().clamp(0.0, limit_w - width);
    let y = top.round().clamp(0.0, limit_h - height);
    Crop { x: x as u32, y: y as u32, width: width as u32, height: height as u32 }
}

fn moved(start: Crop, by: egui::Vec2, size: (u32, u32)) -> Crop {
    let [left, top, right, bottom] = bounds(start);
    let shift = vec2(by.x.round(), by.y.round());
    settle([left + shift.x, top + shift.y, right + shift.x, bottom + shift.y], None, size)
}

fn resized(
    start: Crop,
    edges: [bool; 4],
    point: Pos2,
    ratio: Option<f64>,
    size: (u32, u32),
) -> Option<Crop> {
    let [mut left, mut top, mut right, mut bottom] = bounds(start);
    if edges[0] {
        left = point.x.min(right - 1.0);
    }
    if edges[1] {
        top = point.y.min(bottom - 1.0);
    }
    if edges[2] {
        right = point.x.max(left + 1.0);
    }
    if edges[3] {
        bottom = point.y.max(top + 1.0);
    }
    if let Some(ratio) = ratio.map(|r| r as f32) {
        let sideways = edges[0] || edges[2];
        let upright = edges[1] || edges[3];
        let (width, height) = (right - left, bottom - top);
        match (sideways, upright) {
            // an edge: the other side follows, around the middle
            (true, false) => {
                let middle = (top + bottom) / 2.0;
                (top, bottom) = (middle - width / ratio / 2.0, middle + width / ratio / 2.0);
            }
            (false, true) => {
                let middle = (left + right) / 2.0;
                (left, right) = (middle - height * ratio / 2.0, middle + height * ratio / 2.0);
            }
            // a corner: the larger of both, from the opposite corner
            _ => {
                let width = width.max(height * ratio);
                let height = width / ratio;
                if edges[0] {
                    left = right - width;
                } else {
                    right = left + width;
                }
                if edges[1] {
                    top = bottom - height;
                } else {
                    bottom = top + height;
                }
            }
        }
    }
    Some(settle([left, top, right, bottom], ratio, size))
}

fn drawn(corner: Pos2, point: Pos2, ratio: Option<f64>, size: (u32, u32)) -> Option<Crop> {
    let (mut width, mut height) = ((point.x - corner.x).abs(), (point.y - corner.y).abs());
    if width < 2.0 && height < 2.0 {
        return None;
    }
    if let Some(ratio) = ratio.map(|r| r as f32) {
        width = width.max(height * ratio);
        height = width / ratio;
    }
    let left = if point.x < corner.x { corner.x - width } else { corner.x };
    let top = if point.y < corner.y { corner.y - height } else { corner.y };
    Some(settle([left, top, left + width, top + height], ratio, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (u32, u32) = (200, 100);

    #[test]
    fn makes_room_for_the_small_picture_in_its_corner() {
        let label = vec2(100.0, 30.0);
        // a square input in a wide view: a strip on the side of the corner
        let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let (input, room) = split(wide, (100, 100), label, INSET_SIZES[2], Corner::BottomRight);
        assert_eq!((room.right(), room.bottom(), room.width()), (786.0, 386.0, 136.0));
        assert!(input.left() == 0.0 && input.right() <= room.left(), "{input:?} {room:?}");
        let (input, room) = split(wide, (100, 100), label, INSET_SIZES[2], Corner::TopLeft);
        assert_eq!(room.min, pos2(MARGIN, MARGIN + label.y + LABEL_GAP));
        assert!(input.right() == 800.0 && input.left() >= room.right(), "{input:?} {room:?}");

        // where there is little room, the input keeps three quarters of its
        // size, and a larger picture goes over it
        let tight = Rect::from_min_size(pos2(0.0, 0.0), vec2(510.0, 400.0));
        let shown = (510.0 - 2.0 * MARGIN) / 400.0 * 400.0;
        for (size, &share) in INSET_SIZES.iter().enumerate() {
            let (input, room) = split(tight, (400, 300), label, share, Corner::BottomRight);
            assert!(input.width() - 2.0 * MARGIN >= shown * LEAST_SHARE - 0.01, "{input:?}");
            if size == 0 || size == 4 {
                assert_eq!(input.right() > room.left(), size == 4, "{size}: {input:?} {room:?}");
            }
        }
        // with no room at all, it goes over the input's corner
        let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 150.0));
        let (input, room) = split(small, (400, 300), label, INSET_SIZES[0], Corner::BottomLeft);
        assert_eq!((input, room.left()), (small, MARGIN));
    }

    #[test]
    fn changes_the_small_picture_by_its_buttons() {
        use egui_kittest::kittest::{NodeT, Queryable};
        type State = (Inset, Option<InsetChange>, usize);
        let show = |ui: &mut egui::Ui, (inset, change, size): &mut State| {
            let view = ui.max_rect();
            let room = Rect::from_min_size(view.max - vec2(100.0, 100.0), vec2(80.0, 80.0));
            let place = (Corner::BottomRight, *size, true);
            let red = |_| egui::ColorImage::filled([8, 8], Color32::RED);
            let room = (view, room, vec2(1.0, 1.0));
            *change = inset.show(ui, room, place, 1, None, vec2(8.0, 8.0), red).or(*change);
        };
        let mut harness = egui_kittest::Harness::new_ui_state(show, (Inset::default(), None, 2));
        harness.get_by_label("+").click();
        harness.run();
        assert_eq!(harness.state().1, Some(InsetChange::Size(3)));
        // a click is no drag
        assert!(harness.state().0.dropped.is_none());
        // the smallest has none smaller
        harness.state_mut().2 = 0;
        harness.run();
        let disabled = |label| harness.get_by_label(label).accesskit_node().is_disabled();
        assert!(disabled("−") && !disabled("+"));
        // dragged to the top left, by a button on its label, which keeps
        // the drag from what is under it, it goes there, from where it was
        // let go of
        let start = harness.get_by_label("+").rect().center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(start));
        harness.event(button(start, true));
        harness.run();
        for to in [start - vec2(40.0, 40.0), pos2(20.0, 20.0)] {
            harness.event(egui::Event::PointerMoved(to));
            harness.run();
        }
        harness.event(button(pos2(20.0, 20.0), false));
        harness.run();
        assert_eq!(harness.state().1, Some(InsetChange::Corner(Corner::TopLeft)));
        let dropped = harness.state().0.dropped.unwrap();
        assert!(dropped.center().x < start.x - 100.0, "{dropped:?}");
    }

    #[test]
    fn zooms_without_jumps() {
        let view = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let (beside, smaller) = (
            Rect::from_min_size(pos2(0.0, 0.0), vec2(450.0, 400.0)),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 400.0)),
        );
        let size = (300, 200);
        // zoomed in, the picture stays put as the small picture's strip
        // changes beside it
        let mut zoom = Zoom { level: 2.0, centre: vec2(0.3, 0.6), anchor: None };
        hold(&mut zoom, (view, beside), size);
        let before = layout(beside, size, &zoom).0;
        hold(&mut zoom, (view, smaller), size);
        let after = layout(smaller, size, &zoom).0;
        assert!((before.min - after.min).length() < 1e-3, "{before:?} {after:?}");
        assert!((before.size() - after.size()).length() < 1e-3, "{before:?} {after:?}");
        // zooming out, the picture comes to where it fits bit by bit
        let mut last = zoom.centre;
        for level in [1.8, 1.4, 1.1, 1.01, 1.0] {
            zoom.level = level;
            cover(&mut zoom, smaller, size);
            assert!((zoom.centre - last).length() < 0.15, "{level}: {:?}", zoom.centre);
            last = zoom.centre;
        }
        assert_eq!(zoom.centre, vec2(0.5, 0.5));
    }

    #[test]
    fn eases_only_changes_of_what_the_layout_is_made_of() {
        let ctx = egui::Context::default();
        let mut inset = Inset::default();
        let rect = |x: f32| Rect::from_min_size(pos2(x, 0.0), vec2(10.0, 10.0));
        let (before, after) = ((rect(0.0), rect(1.0)), (rect(2.0), rect(3.0)));
        // bottom right, where the picture is at the end of its room both ways
        let at = (true, 2, Corner::BottomRight);
        let with = |(input, room): (Rect, Rect)| (input, room, vec2(1.0, 1.0));
        assert_eq!(inset.ease(&ctx, 1, at, before), with(before));
        // the window's size changing follows it at once
        assert_eq!(inset.ease(&ctx, 1, at, after), with(after));
        // a size changing starts from what was drawn
        assert_eq!(inset.ease(&ctx, 1, (true, 3, Corner::BottomRight), before), with(after));
        // another corner, its place in its room too
        let top_left = (true, 3, Corner::TopLeft);
        assert_eq!(inset.ease(&ctx, 1, top_left, before), with(after));
        // let go of where it is, from there
        inset.dropped = Some(rect(9.0));
        assert_eq!(inset.ease(&ctx, 1, top_left, before).1, rect(9.0));
        // another item is drawn as it is
        assert_eq!(inset.ease(&ctx, 2, at, before), with(before));
    }

    #[test]
    fn keeps_crops_within_the_picture() {
        let crop = Crop { x: 150, y: 50, width: 40, height: 40 };
        assert_eq!(
            moved(crop, vec2(30.0, 30.0), SIZE),
            Crop { x: 160, y: 60, width: 40, height: 40 }
        );
        assert_eq!(moved(crop, vec2(-500.0, 0.0), SIZE).x, 0);
    }

    #[test]
    fn resizes_by_edges_and_corners() {
        let crop = Crop { x: 50, y: 20, width: 100, height: 60 };
        // the right edge
        let wider = resized(crop, [false, false, true, false], pos2(170.0, 0.0), None, SIZE);
        assert_eq!(wider, Some(Crop { width: 120, ..crop }));
        // the top left corner, square: the larger side wins, shrunk to fit
        let square =
            resized(crop, [true, true, false, false], pos2(40.0, 10.0), Some(1.0), SIZE).unwrap();
        assert_eq!(square, Crop { x: 40, y: 0, width: 100, height: 100 });
        // an edge keeps the ratio around the middle
        let wide =
            resized(crop, [false, false, true, false], pos2(170.0, 0.0), Some(2.0), SIZE).unwrap();
        assert_eq!((wide.width, wide.height, wide.y), (120, 60, 20));
    }

    #[test]
    fn draws_new_crops_from_either_corner() {
        assert_eq!(drawn(pos2(50.0, 50.0), pos2(51.0, 50.5), None, SIZE), None);
        let drawn = drawn(pos2(100.0, 80.0), pos2(60.0, 20.0), Some(1.0), SIZE).unwrap();
        assert_eq!(drawn, Crop { x: 40, y: 20, width: 60, height: 60 });
    }
}

#[cfg(test)]
mod exact_tests {
    use super::*;

    #[test]
    fn holds_crops_to_an_exact_scale() {
        let size = (1920, 1080);
        // twice as large: at most 256 on the longer side
        let twice = Exact::of("2".parse().unwrap(), Target::Sticker).unwrap();
        assert_eq!((twice.long, twice.step), (256, 1));
        let crop = Crop { x: 100, y: 100, width: 300, height: 101 };
        let start = (Keep::Start, Keep::Start);
        assert_eq!(twice.fit(crop, start, None, size), Some(Crop { width: 256, ..crop }));
        // a left edge dragged keeps the right one
        let end = (Keep::End, Keep::Start);
        assert_eq!(twice.fit(crop, end, None, size).unwrap().x, 144);
        // a smaller crop stays as it is: margins fill the rest
        let small = Crop { width: 200, ..crop };
        assert_eq!(twice.fit(small, start, None, size), Some(small));
        // three times: at most 170, which make 510
        let thrice = Exact::of("3".parse().unwrap(), Target::Sticker).unwrap();
        assert_eq!(thrice.fit(crop, start, None, size), Some(Crop { width: 170, ..crop }));
        // halved: at most 1024 on the longer side, both in whole 2x2 blocks
        let half = Exact::of("1/2".parse().unwrap(), Target::Sticker).unwrap();
        let tall = Crop { x: 0, y: 0, width: 501, height: 1070 };
        assert_eq!(
            half.fit(tall, start, None, size),
            Some(Crop { width: 500, height: 1024, ..tall })
        );
        // a shape kept, the crop resized within the most
        let one = Exact::of("1".parse().unwrap(), Target::Sticker).unwrap();
        let wide = one.fit(crop, start, Some(16.0 / 9.0), size).unwrap();
        assert_eq!((wide.width, wide.height), (300, 169));
        let full = Crop { x: 0, y: 0, width: 1920, height: 1080 };
        let middle = one.fit(full, (Keep::Middle, Keep::Middle), Some(16.0 / 9.0), size).unwrap();
        assert_eq!(middle, Crop { x: 704, y: 396, width: 512, height: 288 });
        // in whole blocks, within a smaller picture
        let quarter = Exact::of("1/4".parse().unwrap(), Target::Emoji).unwrap();
        assert_eq!(
            quarter.fit(crop, start, Some(1.0), (390, 400)).map(|c| (c.width, c.height)),
            Some((300, 300))
        );
        assert_eq!(quarter.fit(crop, start, None, (3, 600)), None);
        // no pixel larger than the result
        assert!(Exact::of(ExactScale { up: 513, down: 1 }, Target::Sticker).is_none());
    }
}

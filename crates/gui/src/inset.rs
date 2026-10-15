//! The small picture of what the result will look like, beside the input
//! or over its corner: where it goes, its label and the buttons on it,
//! dragging it to another corner, and easing it into place as it moves.
//! What it shows is predicted in [`crate::predict`].

use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use tgradish_core::config::Corner;

use crate::canvas::MARGIN;
use crate::l10n::fl;
use crate::widgets;

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
                format!("{}\n{details}", fl!("inset-label", scale = scale))
            }
            None => fl!("inset-label", "plain"),
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
                if on_buttons { label } else { label.on_hover_text(fl!("inset-label", "drag")) },
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
        if row.add_enabled(size > 0, button("−")).on_hover_text(fl!("inset-smaller")).clicked() {
            change = Some(InsetChange::Size(size - 1));
        }
        if row.add_enabled(size < last, button("+")).on_hover_text(fl!("inset-larger")).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

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
            button: egui::PointerButton::Primary,
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
}

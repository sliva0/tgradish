//! The big preview: the input with its crop, which is edited by dragging,
//! or the result. The wheel zooms, the right or middle button pans. The
//! small picture of the result beside the input is [`crate::inset`].

use eframe::egui::{
    self, Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2,
};
use tgradish_core::options::{Crop, IntegerScale};
use tgradish_core::telegram::Target;

use crate::item::Zoom;
use crate::media::Clip;
use crate::{theme, widgets};

/// The one texture the preview draws, and which frame it holds.
#[derive(Default)]
pub(crate) struct Screen {
    texture: Option<egui::TextureHandle>,
    /// Clip, frame and whether it is drawn sharp.
    shows: Option<(u64, usize, bool)>,
}

impl Screen {
    /// The clip whose frame is shown.
    #[cfg(test)]
    pub(crate) fn showing(&self) -> Option<u64> {
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

/// A frame to show, and the size of what it shows in input pixels: the
/// clip may hold smaller frames.
pub(crate) struct Picture<'a> {
    pub(crate) clip: &'a Clip,
    pub(crate) frame: usize,
    pub(crate) size: (u32, u32),
    /// Shown in the middle of a square canvas, as `.tgs` stickers are.
    pub(crate) square: bool,
}

/// The crop being edited, with the width to height ratio it keeps.
pub(crate) struct Cropping<'a> {
    pub(crate) crop: &'a mut Option<Crop>,
    pub(crate) ratio: Option<f64>,
    /// What of the crop the result keeps, cutting off the rest.
    pub(crate) kept: Option<Kept>,
    /// The size an integer scale holds the crop to.
    pub(crate) exact: Option<Exact>,
}

/// What of a crop a result keeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kept {
    /// Its middle, filling a box of this width to height ratio.
    Ratio(f64),
    /// This part of the picture, in input pixels: left, top, right and
    /// bottom.
    Part([usize; 4]),
}

/// Which edge of a crop stays put when its size changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    Start,
    End,
    Middle,
}

/// A crop held to an integer scale: its longer side at most `long` input
/// pixels, the most the scale fits into the result, and its sides whole
/// blocks of `step` pixels, which shrinking averages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Exact {
    pub(crate) long: u32,
    pub(crate) step: u32,
}

impl Exact {
    /// The crop `scale` takes for `target`, if a pixel at that scale isn't
    /// larger than the result.
    pub(crate) fn of(scale: IntegerScale, target: Target) -> Option<Exact> {
        let (width, height) = target.box_size();
        let long = scale.most_input(width.max(height));
        (long > 0).then_some(Exact { long, step: scale.down })
    }

    /// Whether a `size` picture has a whole block.
    pub(crate) fn fits(self, size: (u32, u32)) -> bool {
        size.0.min(size.1) >= self.step
    }

    /// `crop` held to this size within a `size` picture: no larger than the
    /// scale takes, in whole blocks, and as `ratio` says; `keep` says which
    /// edges stay put, across and down.
    pub(crate) fn fit(
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
pub(crate) const MARGIN: f32 = 14.0;
/// How near an edge the pointer grabs it, in points.
const GRAB: f32 = 7.0;
/// How many of the screen's pixels a pixel of a picture takes, zoomed in
/// as far as it goes: enough to see each and put a crop on its edge,
/// however large the picture.
const MOST_PIXELS: f32 = 32.0;
/// How far a picture is zoomed in at least, from where it fits, where its
/// pixels take more than that there already, as a small sprite's do.
const LEAST_MOST_LEVEL: f32 = 4.0;
/// The most of the screen's pixels a pixel of a picture takes zoomed in,
/// which Zoom out has room to say.
pub(crate) const MOST_TIMES: f32 = 9999.0;

/// How far a picture is zoomed in to at most, from where it fits, where a
/// pixel of it takes `fit` of the screen's pixels: until a pixel takes
/// [`MOST_PIXELS`], rather than so many times where it fits, which for a
/// sprite would be pixels larger than the screen and for a photo would
/// stop short of seeing its pixels.
fn most_level(fit: f32) -> f32 {
    let most = MOST_PIXELS.max(fit * LEAST_MOST_LEVEL).min(MOST_TIMES);
    (most / fit).max(1.0)
}
/// Side of the squares that show transparency, in points.
const CHECKER: f32 = 8.0;

/// Where the picture goes in `rect`, and how many points an input pixel
/// takes.
pub(crate) fn layout(rect: Rect, size: (u32, u32), zoom: &Zoom) -> (Rect, f32) {
    let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let scale = fit(rect, size, zoom.grid) * zoom.level;
    let shown = vec2(width, height) * scale;
    let min = rect.center() - vec2(zoom.centre.x * shown.x, zoom.centre.y * shown.y);
    (Rect::from_min_size(min, shown), scale)
}

/// How many points an input pixel takes where a `size` picture fits
/// `rect`: for pixel art, whose pixels take `grid` of the screen's for
/// each point an input pixel takes, a whole number of the screen's pixels
/// to each of its own from two up, as nearest neighbour draws them, so
/// that they are all as wide, rather than some a screen pixel wider.
fn fit(rect: Rect, size: (u32, u32), grid: Option<f32>) -> f32 {
    let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let fit = ((rect.width() - 2.0 * MARGIN) / width).min((rect.height() - 2.0 * MARGIN) / height);
    let fit = match grid {
        Some(grid) if fit * grid >= 2.0 => (fit * grid).floor() / grid,
        _ => fit,
    };
    fit.max(1e-3)
}

/// Screen pixels to a picture pixel from which pixels show as squares:
/// a smooth picture blurs them, and you can't put a crop on a pixel's edge.
const SHARP_FROM: f32 = 2.0;

/// Shows `picture` in `rect`, with `cropping` over it if given, which
/// dragging changes. The picture fits `fit_in`, a part of `rect`, and
/// zooms from there, over the rest of `rect`: nothing moves when zooming
/// starts or ends.
pub(crate) fn show(
    ui: &mut egui::Ui,
    screen: &mut Screen,
    (rect, fit_in): (Rect, Rect),
    picture: &Picture,
    zoom: &mut Zoom,
    cropping: Option<Cropping>,
) {
    // a part of the space the caller took: taking it again would move
    // what comes next. Not a stop of the keyboard's focus itself: the crop
    // on it is
    let response = ui.interact(rect, ui.id().with("canvas"), Sense::CLICK | Sense::DRAG);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::stage(ui.visuals().dark_mode));

    let side = picture.size.0.max(picture.size.1);
    let canvas = if picture.square { (side, side) } else { picture.size };
    // pixel art's pixels to the screen's, for each input pixel's point
    let ppp = ui.ctx().pixels_per_point();
    let art = picture.clip.width as f32 / picture.size.0.max(1) as f32;
    zoom.grid = picture.clip.pixelated.then_some(ppp / art);
    ease_out(ui.ctx(), zoom);
    hold(zoom, (rect, fit_in), canvas, ppp);
    zoom_and_pan(ui, &response, fit_in, canvas, zoom);
    let (whole, scale) = layout(fit_in, canvas, zoom);
    zoom.scale = Some(scale * ui.ctx().pixels_per_point());
    // its squares still on the stage, as the picture zooms and pans over
    // them: counted from a corner of the picture, they would race along
    // with it while zooming eases in, all but those near that corner
    widgets::checkerboard_from(&painter, whole, CHECKER, rect.min);
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
    let full = Crop::whole(size);
    let crop = cropping.crop.unwrap_or(full);
    let on_screen = Rect::from_min_size(
        shown.min + vec2(crop.x as f32, crop.y as f32) * scale,
        vec2(crop.width as f32, crop.height as f32) * scale,
    );

    // the crop's size and its cross, over its top left corner, or within
    // it near the stage's top: there the pointer is on them, not on the
    // crop. As they take no drags, egui gives this the drags that start on
    // them, and has it hovered under them
    let tag = cropping.crop.is_some().then(|| size_tag(&painter, crop, on_screen, rect).back);

    // what a drag starting at `point` would do
    let cropped = cropping.crop.is_some();
    let grab = |point: Pos2| -> Option<Drag> {
        if tag.is_some_and(|tag| tag.contains(point)) {
            return None;
        }
        if let Some(edges) = edges_at(point, on_screen) {
            Some(Drag::Edges(edges, crop))
        } else if cropped && on_screen.contains(point) {
            Some(Drag::Move(crop, to_pixels(point)))
        } else if shown.contains(point) {
            Some(Drag::New(to_pixels(point)))
        } else {
            None
        }
    };

    let id = response.id.with("drag");
    if response.drag_started_by(PointerButton::Primary) {
        // none from the size or cross, nor what is left of a drag that
        // ended while this wasn't shown; with the crop it started from
        let origin = ui.input(|input| input.pointer.press_origin());
        ui.data_mut(|data| match origin.and_then(grab) {
            Some(drag) => {
                data.insert_temp(id, (drag, *cropping.crop));
            }
            None => data.remove::<(Drag, Option<Crop>)>(id),
        });
    }
    let dragging = response.dragged_by(PointerButton::Primary);
    // Escape takes a drag back, the crop as it was before it, as egui ends
    // the drag on it, rather than leaving the crop where it was dragged
    let held = ui.input(|input| input.pointer.primary_down());
    if held
        && let Some((_, before)) = ui.data(|data| data.get_temp::<(Drag, Option<Crop>)>(id))
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        *cropping.crop = before;
        ui.data_mut(|data| data.remove::<(Drag, Option<Crop>)>(id));
    }
    let drag = ui.data(|data| data.get_temp::<(Drag, Option<Crop>)>(id)).map(|(drag, _)| drag);
    if let Some(point) = response.hover_pos() {
        // what is being dragged, while it is, as past the size and cross,
        // or else what would be
        let icon = match if dragging { drag } else { grab(point) } {
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
    // the lines of the picture the crop is snapped to, across and down
    let mut guides: Vec<(bool, f32)> = Vec::new();
    if let (Some(drag), true) = (drag, dragging)
        && let Some(point) = response.interact_pointer_pos()
    {
        let mut point = to_pixels(point);
        let side = |end: bool| if end { Keep::End } else { Keep::Start };
        let keys = ui.input(|input| input.modifiers);
        // to the picture's edges and middle, where near enough, unless Ctrl
        // is held for a crop anywhere
        let snaps = if keys.command { [vec![], vec![]] } else { snaps(drag, keys.alt, size) };
        let within = SNAP / scale;
        for (across, snaps) in [true, false].into_iter().zip(snaps) {
            let value = if across { &mut point.x } else { &mut point.y };
            if let Some((at, line)) = nearest_snap(*value, &snaps, within) {
                *value = at;
                guides.push((across, line));
            }
        }
        // a corner dragged with Shift keeps the shape the crop had, unless
        // one is set; with Alt, the crop grows and shrinks about its middle
        let ratio = match drag {
            Drag::Edges([left, top, right, bottom], start)
                if keys.shift && (left || right) && (top || bottom) =>
            {
                let shape = f64::from(start.width) / f64::from(start.height);
                cropping.ratio.or(Some(shape))
            }
            _ => cropping.ratio,
        };
        let (new, keep) = match drag {
            Drag::Edges(edges, start) if keys.alt => {
                (resized(start, edges, point, (ratio, true), size), (Keep::Middle, Keep::Middle))
            }
            Drag::Edges(edges, start) => (
                resized(start, edges, point, (ratio, false), size),
                (side(edges[0]), side(edges[1])),
            ),
            Drag::Move(start, from) => {
                (Some(moved(start, point - from, size)), (Keep::Start, Keep::Start))
            }
            Drag::New(corner) => (
                drawn(corner, point, ratio, size),
                (side(point.x < corner.x), side(point.y < corner.y)),
            ),
        };
        let new = match cropping.exact {
            Some(exact) => new.and_then(|new| exact.fit(new, keep, ratio, size)),
            None => new,
        };
        if let Some(new) = new {
            let new = (new != full).then_some(new);
            if new != *cropping.crop {
                *cropping.crop = new;
            }
        }
        // only those it is still on, as a shape kept or an integer scale
        // may take it off them
        let crop = cropping.crop.unwrap_or(full);
        guides.retain(|&(across, line)| on_line(crop, across, line));
    }
    // nor kept once the button is up, wherever it was let go
    if response.drag_stopped() || !held {
        ui.data_mut(|data| data.remove::<(Drag, Option<Crop>)>(id));
    }
    // all of the picture, or as much of it as the integer scale takes
    let whole_picture = |crop: &mut Option<Crop>| {
        let middle = (Keep::Middle, Keep::Middle);
        *crop = match cropping.exact {
            Some(exact) => {
                exact.fit(full, middle, cropping.ratio, size).filter(|crop| *crop != full)
            }
            None => None,
        };
    };
    if response.double_clicked() && cropping.crop.is_some() {
        whole_picture(cropping.crop);
    }
    // the crop takes the keyboard's focus as it is pressed on, or drawn
    let crop_id = response.id.with("crop");
    ui.data_mut(|data| data.insert_temp(egui::Id::new(CROP), crop_id));
    let pressed_on =
        |point: Option<Pos2>| point.and_then(grab).filter(|drag| !matches!(drag, Drag::New(_)));
    let mut focus = (dragging && drag.is_some())
        || (response.clicked() && pressed_on(response.interact_pointer_pos()).is_some());
    // its ring is shown once the arrows move it, till the canvas is
    // pressed again or the focus goes elsewhere: a press takes the focus
    // too, and the crop's own frame is enough to see it by then
    let keyed_id = crop_id.with(KEYED);
    if response.is_pointer_button_down_on() || !ui.memory(|memory| memory.has_focus(crop_id)) {
        ui.data_mut(|data| data.remove::<bool>(keyed_id));
    }
    // where it has the focus, the arrows move it a pixel, ten with Shift,
    // and with Ctrl move its bottom right corner
    if cropping.crop.is_some() && ui.memory(|memory| memory.has_focus(crop_id)) {
        let arrows = egui::EventFilter {
            horizontal_arrows: true,
            vertical_arrows: true,
            ..Default::default()
        };
        ui.memory_mut(|memory| memory.set_focus_lock_filter(crop_id, arrows));
        // resized a whole block at a time, where an integer scale takes
        // them, as it rounds the crop down to them
        let block = cropping.exact.map_or(1, |exact| exact.step);
        let crop = cropping.crop.unwrap_or(full);
        if let Some(new) = keyed(ui, crop, (cropping.ratio, block), size) {
            let new = match cropping.exact {
                Some(exact) => exact.fit(new, (Keep::Start, Keep::Start), cropping.ratio, size),
                None => Some(new),
            };
            if let Some(new) = new {
                *cropping.crop = (new != full).then_some(new);
            }
            focus = true;
            ui.data_mut(|data| data.insert_temp(keyed_id, true));
        }
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
    // what the result cuts off
    if let Some(kept) = cropping.kept {
        let kept = match kept {
            Kept::Ratio(ratio) => {
                let ratio = ratio as f32;
                let (width, height) = (on_screen.width(), on_screen.height());
                let kept = if width / height > ratio {
                    vec2(height * ratio, height)
                } else {
                    vec2(width, width / ratio)
                };
                Rect::from_center_size(on_screen.center(), kept)
            }
            Kept::Part([x0, y0, x1, y1]) => {
                let at = |x: usize, y: usize| shown.min + vec2(x as f32, y as f32) * scale;
                Rect::from_min_max(at(x0, y0), at(x1, y1))
            }
        };
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
    // a line of the accent across the picture where the crop is snapped
    let ppp = ui.ctx().pixels_per_point();
    for (across, line) in guides {
        let guide = Stroke::new(1.0 / ppp, theme::mark(ui.visuals()));
        let at = shown.min + vec2(line, line) * scale;
        // on a whole pixel, as a line a pixel wide is sharp only there
        let pixel = |x: f32| ((x * ppp).floor() + 0.5) / ppp;
        let ends = match across {
            true => [pos2(pixel(at.x), shown.top()), pos2(pixel(at.x), shown.bottom())],
            false => [pos2(shown.left(), pixel(at.y)), pos2(shown.right(), pixel(at.y))],
        };
        painter.line_segment(ends, guide);
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
        let SizeTag { galley, at, back, cross } = size_tag(&painter, crop, on_screen, rect);
        painter.rect_filled(back, 4.0, Color32::from_black_alpha(170));
        painter.galley(at, galley, Color32::WHITE);
        let hint = match cropping.exact {
            Some(_) => crate::l10n::fl!("whole-picture", "exact"),
            None => crate::l10n::fl!("whole-picture", "hint"),
        };
        let button = egui::Button::new(egui::RichText::new("✖").size(10.0).color(Color32::WHITE))
            .min_size(cross.size());
        let clicked = ui.scope(|ui| {
            // bare on the dark tag, and lit on hover, whichever the theme
            let states = &mut ui.visuals_mut().widgets;
            for (state, fill) in [
                (&mut states.inactive, Color32::TRANSPARENT),
                (&mut states.hovered, Color32::from_white_alpha(40)),
                (&mut states.active, Color32::from_white_alpha(70)),
            ] {
                (state.weak_bg_fill, state.bg_stroke) = (fill, Stroke::NONE);
            }
            // a button to the pointer, not the edge under it
            ui.visuals_mut().interact_cursor = Some(CursorIcon::PointingHand);
            let button = ui.place(cross, button);
            widgets::named(button, &crate::l10n::fl!("whole-picture")).on_hover_text(hint).clicked()
        });
        if clicked.inner {
            whole_picture(cropping.crop);
        }
        // last, as egui takes the focus from it on a press elsewhere, which
        // a press on its edges outside it is
        let ring = on_screen.expand(RING);
        let crop = ui.interact(ring, crop_id, Sense::focusable_noninteractive());
        widgets::named(crop, &crate::l10n::fl!("crop-keys"));
        // asked for again on the next frame, as in the window egui drops a
        // focus asked for in the frame of a click by the end of that frame
        let asked = crop_id.with("asked");
        if ui.data_mut(|data| data.remove_temp::<bool>(asked)).is_some() {
            ui.memory_mut(|memory| memory.request_focus(crop_id));
        }
        if focus && !clicked.inner {
            ui.memory_mut(|memory| memory.request_focus(crop_id));
            ui.data_mut(|data| data.insert_temp(asked, true));
        }
    }
}

/// The id under which the crop's own is kept, for [`crop_focused`].
const CROP: &str = "crop";

/// Whether the crop has the keyboard's focus, which takes only the arrows
/// that move it: the GUI's other shortcuts are kept as they are.
pub(crate) fn crop_focused(ctx: &egui::Context) -> bool {
    let crop = ctx.data(|data| data.get_temp::<egui::Id>(egui::Id::new(CROP)));
    crop.is_some() && ctx.memory(|memory| memory.focused()) == crop
}

/// The id under the crop's own of whether the arrows moved it, for
/// [`crop_ringed`].
const KEYED: &str = "keyed";

/// Whether the crop shows the ring of the keyboard's focus: only once the
/// arrows moved it, as it takes the focus with each press on it too, and
/// only till the canvas is pressed again.
pub(crate) fn crop_ringed(ctx: &egui::Context) -> bool {
    let crop = ctx.data(|data| data.get_temp::<egui::Id>(egui::Id::new(CROP)));
    let keyed = |crop: egui::Id| ctx.data(|data| data.get_temp::<bool>(crop.with(KEYED)));
    crop_focused(ctx) && crop.and_then(keyed).is_some()
}

/// How far out of the crop the ring of the keyboard's focus is drawn, the
/// handles inside it.
const RING: f32 = 7.0;

/// How near a corner of the crop, in points, the pointer takes it, across
/// and down: a larger target than an edge, which it wins over.
const CORNER: f32 = 10.0;

/// The edges of the crop shown at `crop` that the pointer at `point` takes,
/// `[left, top, right, bottom]`: a corner's two where it is near one, or
/// an edge. Within the crop, each reaches at most a quarter of its width
/// or height, so that the middle of even a small crop is left to move it
/// by.
fn edges_at(point: Pos2, crop: Rect) -> Option<[bool; 4]> {
    // near the start of a side `low` to `high` along an axis, and its end
    let near = |at: f32, (low, high): (f32, f32), reach: f32| {
        let inside = reach.min((high - low) / 4.0);
        let start = (low - reach..=low + inside).contains(&at);
        let end = (high - inside..=high + reach).contains(&at);
        (start, end)
    };
    let across = (crop.left(), crop.right());
    let down = (crop.top(), crop.bottom());
    let (left, right) = near(point.x, across, CORNER);
    let (top, bottom) = near(point.y, down, CORNER);
    if (left || right) && (top || bottom) {
        return Some([left, top, right, bottom]);
    }
    let along = |at: f32, (low, high): (f32, f32)| (low - GRAB..=high + GRAB).contains(&at);
    let (left, right) = near(point.x, across, GRAB);
    let (top, bottom) = near(point.y, down, GRAB);
    let (left, right) = (left && along(point.y, down), right && along(point.y, down));
    let (top, bottom) = (top && along(point.x, across), bottom && along(point.x, across));
    let edges = [left, top, right, bottom];
    edges.contains(&true).then_some(edges)
}

/// What the arrows pressed this frame make of `crop` in a `size` picture,
/// with the `ratio` it keeps: moved a pixel each, ten with Shift, or with
/// Ctrl its bottom right corner moved, by as many blocks of `block`
/// pixels; taken from what else they do.
fn keyed(
    ui: &egui::Ui,
    crop: Crop,
    (ratio, block): (Option<f64>, u32),
    size: (u32, u32),
) -> Option<Crop> {
    use egui::Key;
    let mut by = Vec2::ZERO;
    let mut resize = false;
    ui.input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                let step = if modifiers.shift { 10.0 } else { 1.0 };
                let along = match key {
                    Key::ArrowLeft => vec2(-step, 0.0),
                    Key::ArrowRight => vec2(step, 0.0),
                    Key::ArrowUp => vec2(0.0, -step),
                    Key::ArrowDown => vec2(0.0, step),
                    _ => return true,
                };
                if modifiers.alt {
                    return true;
                }
                by += along;
                resize = modifiers.command;
                false
            }
            _ => true,
        });
    });
    if by == Vec2::ZERO {
        return None;
    }
    if !resize {
        return Some(moved(crop, by, size));
    }
    let by = by * block as f32;
    let [left, top, mut right, mut bottom] = bounds(crop);
    // no further than the picture, rather than moving it back in
    right = (right + by.x).min(size.0 as f32).max(left + 1.0);
    bottom = (bottom + by.y).min(size.1 as f32).max(top + 1.0);
    // the shape kept from the side that moved, and no further than the
    // picture the other way either
    if let Some(ratio) = ratio.map(|r| r as f32) {
        if by.x != 0.0 {
            bottom = top + (right - left) / ratio;
        } else {
            right = left + (bottom - top) * ratio;
        }
        let (width, height) = (right - left, bottom - top);
        let room = ((size.0 as f32 - left) / width).min((size.1 as f32 - top) / height);
        let shrink = room.min(1.0);
        (right, bottom) = (left + width * shrink, top + height * shrink);
    }
    Some(settle([left, top, right, bottom], ratio, size))
}

/// The crop's size over its top left corner, with a cross after it back to
/// all of the picture.
struct SizeTag {
    galley: std::sync::Arc<egui::Galley>,
    /// Where the size's text goes.
    at: Pos2,
    /// All of it, the cross's square too.
    back: Rect,
    cross: Rect,
}

/// Where the size of `crop`, shown at `on_screen`, goes on the `stage`:
/// over its top left corner, clear of where the pointer takes its top
/// edge; where the stage has no room for that, within it, clear of its
/// edges, or else under it; and only where there is no room for any of
/// those, over its top edge at the stage's top.
fn size_tag(painter: &egui::Painter, crop: Crop, on_screen: Rect, stage: Rect) -> SizeTag {
    let text = format!("{} × {}", crop.width, crop.height);
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.5), Color32::WHITE);
    // the size, then a cross back to all of the picture, as wide as the
    // tag is high
    let side = galley.size().y + 8.0;
    let tag = vec2(galley.size().x + side, galley.size().y);
    let pad = 4.0;
    // its back's left, from the left of the crop, within the stage
    let x = (on_screen.left() + 6.0).min(stage.right() - tag.x - 8.0).max(stage.left() + 4.0);
    let x = x - pad;
    let clear = GRAB + 1.0;
    let high = tag.y + 2.0 * pad;
    let room = stage.shrink(pad);
    let over = on_screen.top() - clear - high;
    let within = on_screen.top() + clear;
    let under = on_screen.bottom() + clear;
    let fits_within = on_screen.width() >= tag.x + 2.0 * (pad + clear)
        && on_screen.height() >= high + 2.0 * clear;
    let top = if over >= room.top() {
        over
    } else if fits_within {
        within
    } else if under + high <= room.bottom() {
        under
    } else {
        room.top()
    };
    let x = if top == within { on_screen.left() + clear } else { x };
    // a crop kept from a larger picture lies past this one's edges, where
    // the stage would cut the tag, and its cross with it
    let width = tag.x + 2.0 * pad;
    let x = x.min(room.right() - width).max(room.left());
    let top = top.min(room.bottom() - high).max(room.top());
    let back = Rect::from_min_size(pos2(x, top), tag + Vec2::splat(2.0 * pad));
    let at = back.min + Vec2::splat(pad);
    let cross = Rect::from_min_max(pos2(back.right() - side, back.top()), back.max);
    SizeTag { galley, at, back, cross }
}

fn zoom_and_pan(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: Rect,
    size: (u32, u32),
    zoom: &mut Zoom,
) {
    let most = most_level(fit(rect, size, zoom.grid) * ui.ctx().pixels_per_point());
    // as far as it goes in the room there is now
    zoom.level = zoom.level.min(most);
    if let Some(pointer) = response.hover_pos() {
        let (scroll, pinch) = ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
        let factor = pinch * (scroll * wheel_rate(ui)).exp();
        // taken, so that nothing under the preview scrolls with it too
        if scroll != 0.0 {
            ui.ctx().input_mut(|input| input.smooth_scroll_delta.y = 0.0);
        }
        if (factor - 1.0).abs() > 1e-4 {
            zoom.easing = None;
            let (shown, _) = layout(rect, size, zoom);
            let at = (pointer - shown.min) / shown.size();
            let level = (zoom.level * factor).clamp(1.0, most);
            let shown = shown.size() * (level / zoom.level);
            // the picture point under the pointer stays there
            let min = pointer - vec2(at.x * shown.x, at.y * shown.y);
            zoom.centre = (rect.center() - min) / shown;
            zoom.level = level;
        }
    }
    // scrolled across, as with Shift held, it pans across where zoomed in
    let across = ui.input(|input| input.smooth_scroll_delta.x);
    if response.hovered() && across != 0.0 && zoom.level > 1.0 {
        zoom.easing = None;
        let (shown, _) = layout(rect, size, zoom);
        zoom.centre.x -= across / shown.width();
        ui.ctx().input_mut(|input| input.smooth_scroll_delta.x = 0.0);
    }
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        zoom.easing = None;
        let (shown, _) = layout(rect, size, zoom);
        zoom.centre -= response.drag_delta() / shown.size();
    }
    cover(zoom, rect, size);
}

/// How much a wheel notch zooms in, or out.
const NOTCH: f32 = 1.25;
/// How fast scrolling by points zooms, as a touchpad scrolls, for each.
const PER_POINT: f32 = 0.002;

/// How fast scrolling zooms, by each point scrolled, as the wheel's last
/// turn came: a notch of a mouse wheel, which egui scrolls by so many
/// points, zooms by [`NOTCH`]; a touchpad's scrolling, by points, as far
/// as it scrolls, finer.
fn wheel_rate(ui: &egui::Ui) -> f32 {
    let id = egui::Id::new("wheel in notches");
    let notched = ui.input(|input| {
        input.events.iter().rev().find_map(|event| match event {
            egui::Event::MouseWheel { unit, .. } => Some(*unit != egui::MouseWheelUnit::Point),
            _ => None,
        })
    });
    // the turn eases in over a few frames after its event
    let notched = match notched {
        Some(notched) => {
            ui.data_mut(|data| data.insert_temp(id, notched));
            notched
        }
        None => ui.data(|data| data.get_temp(id)).unwrap_or(false),
    };
    match notched {
        true => NOTCH.ln() / ui.ctx().options(|options| options.input_options.line_scroll_speed),
        false => PER_POINT,
    }
}

/// How long zooming out to all of the picture eases, in seconds.
const ZOOM_OUT: f64 = 0.15;

/// Moves `zoom` on its way out to all of the picture, fast and then
/// settling, while it is on it.
fn ease_out(ctx: &egui::Context, zoom: &mut Zoom) {
    let Some((level, centre, since)) = zoom.easing else { return };
    // at its end at once where motion stops on the way
    let t = crate::inset::eased(f64::from(crate::motion::along(ctx, since, ZOOM_OUT)));
    zoom.level = level + (1.0 - level) * t;
    zoom.centre = centre + (Vec2::splat(0.5) - centre) * t;
    if t >= 1.0 {
        zoom.easing = None;
    }
}

/// Zoomed in, keeps a `size` picture where it is on the screen when the
/// part `fit_in` of the `view` it fits changes, as for the small picture
/// beside it.
fn hold(zoom: &mut Zoom, (view, fit_in): (Rect, Rect), size: (u32, u32), ppp: f32) {
    if let Some((seen, before)) = zoom.anchor
        && seen == view
        && before != fit_in
        && zoom.level > 1.0
    {
        let (shown, scale) = layout(before, size, zoom);
        let fit = fit(fit_in, size, zoom.grid);
        zoom.level = (scale / fit).clamp(1.0, most_level(fit * ppp));
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

/// How near the picture's edges and middle, in points, a crop's edges and
/// middle are snapped to them.
const SNAP: f32 = 6.0;

/// What `drag` snaps across and down: each a place for the pointer, in
/// the picture's pixels, that would put the crop's edge or middle on a
/// line of the picture, its edges or middle, with that line. A crop
/// resized about its middle, as with `centred`, keeps it.
fn snaps(drag: Drag, centred: bool, size: (u32, u32)) -> [Vec<(f32, f32)>; 2] {
    let sides = [size.0 as f32, size.1 as f32];
    std::array::from_fn(|axis| {
        let side = sides[axis];
        let lines = [0.0, side / 2.0, side];
        // the crop's ends along this axis, as numbers
        let ends = |crop: Crop| {
            let [left, top, right, bottom] = bounds(crop);
            if axis == 0 { (left, right) } else { (top, bottom) }
        };
        match drag {
            Drag::Edges(edges, start) if edges[axis] || edges[axis + 2] => {
                let mut snaps: Vec<(f32, f32)> = lines.iter().map(|&line| (line, line)).collect();
                // the middle, from the edge that stays
                let (from, to) = ends(start);
                if !centred {
                    let stays = if edges[axis] { to } else { from };
                    snaps.push((side - stays, side / 2.0));
                }
                snaps
            }
            Drag::Edges(..) => vec![],
            Drag::Move(start, from) => {
                let (low, high) = ends(start);
                let grabbed = if axis == 0 { from.x } else { from.y };
                let parts = [low, (low + high) / 2.0, high];
                // each part of the crop onto each line
                let pairs =
                    parts.iter().flat_map(|part| lines.iter().map(move |line| (part, line)));
                pairs.map(|(part, &line)| (grabbed + line - part, line)).collect()
            }
            Drag::New(corner) => {
                let corner = if axis == 0 { corner.x } else { corner.y };
                let mut snaps: Vec<(f32, f32)> = lines.iter().map(|&line| (line, line)).collect();
                snaps.push((side - corner, side / 2.0));
                snaps
            }
        }
    })
}

/// The nearest of `snaps` to `value`, within `within`.
fn nearest_snap(value: f32, snaps: &[(f32, f32)], within: f32) -> Option<(f32, f32)> {
    let distance = |at: f32| (at - value).abs();
    let near = snaps.iter().filter(|(at, _)| distance(*at) <= within);
    near.min_by(|a, b| distance(a.0).total_cmp(&distance(b.0))).copied()
}

/// Whether an edge or the middle of `crop`, `across` or down, is on `line`.
fn on_line(crop: Crop, across: bool, line: f32) -> bool {
    let [left, top, right, bottom] = bounds(crop);
    let (low, high) = if across { (left, right) } else { (top, bottom) };
    [low, (low + high) / 2.0, high].iter().any(|part| (part - line).abs() < 0.01)
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

/// `start` with its `edges` dragged to `point`, keeping a `ratio` if one is
/// given, and about its middle if `centred`, the opposite edges moving
/// the other way, as far as the picture goes on both sides.
fn resized(
    start: Crop,
    edges: [bool; 4],
    point: Pos2,
    (ratio, centred): (Option<f64>, bool),
    size: (u32, u32),
) -> Option<Crop> {
    let [mut left, mut top, mut right, mut bottom] = bounds(start);
    let middle = pos2((left + right) / 2.0, (top + bottom) / 2.0);
    if centred {
        // half its size at least, and no further than the nearer edge of
        // the picture
        let room =
            vec2(middle.x.min(size.0 as f32 - middle.x), middle.y.min(size.1 as f32 - middle.y));
        if edges[0] || edges[2] {
            let half = (point.x - middle.x).abs().clamp(0.5, room.x.max(0.5));
            (left, right) = (middle.x - half, middle.x + half);
        }
        if edges[1] || edges[3] {
            let half = (point.y - middle.y).abs().clamp(0.5, room.y.max(0.5));
            (top, bottom) = (middle.y - half, middle.y + half);
        }
        if let Some(ratio) = ratio.map(|r| r as f32) {
            // the larger of both, or of the edge dragged, shrunk to fit
            let (width, height) = (right - left, bottom - top);
            let width = match (edges[0] || edges[2], edges[1] || edges[3]) {
                (true, false) => width,
                (false, true) => height * ratio,
                _ => width.max(height * ratio),
            };
            let half = vec2(width, width / ratio) / 2.0;
            let half = half * (room.x / half.x).min(room.y / half.y).min(1.0);
            (left, right) = (middle.x - half.x, middle.x + half.x);
            (top, bottom) = (middle.y - half.y, middle.y + half.y);
        }
        return Some(settle([left, top, right, bottom], ratio, size));
    }
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
                (top, bottom) = (middle.y - width / ratio / 2.0, middle.y + width / ratio / 2.0);
            }
            (false, true) => {
                (left, right) = (middle.x - height * ratio / 2.0, middle.x + height * ratio / 2.0);
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
    fn zooms_without_jumps() {
        let view = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let (beside, smaller) = (
            Rect::from_min_size(pos2(0.0, 0.0), vec2(450.0, 400.0)),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 400.0)),
        );
        let size = (300, 200);
        // zoomed in, the picture stays put as the small picture's strip
        // changes beside it
        let mut zoom = Zoom { level: 2.0, centre: vec2(0.3, 0.6), ..Zoom::default() };
        hold(&mut zoom, (view, beside), size, 1.0);
        let before = layout(beside, size, &zoom).0;
        hold(&mut zoom, (view, smaller), size, 1.0);
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

    /// Whether a dark square of the checkerboard is painted at each of
    /// `points` in what `harness` drew last, at the scale it drew at.
    fn checked<State>(harness: &egui_kittest::Harness<'_, State>, points: &[Pos2]) -> Vec<bool> {
        let dark = [Color32::from_gray(72), Color32::from_gray(212)];
        let shapes = &harness.output().shapes;
        points
            .iter()
            .map(|point| {
                shapes.iter().any(|clipped| match &clipped.shape {
                    egui::Shape::Rect(rect) => {
                        dark.contains(&rect.fill) && rect.rect.contains(*point)
                    }
                    _ => false,
                })
            })
            .collect()
    }

    #[test]
    fn keeps_the_checkerboard_still_while_zooming_eases_in() {
        use egui_kittest::Harness;
        // a transparent picture, zoomed by the wheel as smooth scrolling
        // eases it in over several frames, at the window's usual scale and
        // at a fractional one
        for ppp in [1.0, 1.25] {
            let clip = Clip {
                id: 1,
                width: 64,
                height: 48,
                frames: vec![vec![0; 64 * 48 * 4]],
                times: vec![0.0, 0.1],
                pixelated: false,
            };
            let state = (Screen::default(), Zoom::default(), clip);
            let mut harness = Harness::builder()
                .with_size([600.0, 400.0])
                .with_pixels_per_point(ppp)
                .build_ui_state(
                    |ui, (screen, zoom, clip): &mut (Screen, Zoom, Clip)| {
                        let rect = ui.max_rect();
                        let picture = Picture { clip, frame: 0, size: (64, 48), square: false };
                        show(ui, screen, (rect, rect), &picture, zoom, None);
                    },
                    state,
                );
            harness.run_steps(2);
            // far from the picture's top left corner, and from the stage's
            let points = [pos2(500.0, 330.0), pos2(420.0, 250.0), pos2(310.0, 340.0)];
            let before = checked(&harness, &points);
            let zoomed_at = pos2(450.0, 300.0);
            harness.event(egui::Event::PointerMoved(zoomed_at));
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 300.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            let mut levels = vec![harness.state().1.level];
            for _ in 0..4 {
                harness.step();
                levels.push(harness.state().1.level);
                assert_eq!(checked(&harness, &points), before, "{ppp}: zoomed to {levels:?}");
            }
            // it did zoom, a frame at a time
            assert!(levels.windows(2).filter(|pair| pair[1] > pair[0]).count() >= 2, "{levels:?}");
            // panned with the middle button, too
            let press = |pressed| egui::Event::PointerButton {
                pos: zoomed_at,
                button: egui::PointerButton::Middle,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            harness.event(press(true));
            harness.step();
            let centre = harness.state().1.centre;
            harness.event(egui::Event::PointerMoved(zoomed_at - vec2(60.0, 40.0)));
            harness.step();
            assert_ne!(harness.state().1.centre, centre);
            assert_eq!(checked(&harness, &points), before, "{ppp}: panned");
            harness.event(press(false));
            harness.step();
        }
    }

    #[test]
    fn the_crops_size_and_cross_win_over_the_edges_under_them() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        // a crop at the picture's top, which is at the stage's, so its size
        // and cross go within it, over what would move it
        let clip = Clip {
            id: 1,
            width: 64,
            height: 48,
            frames: vec![vec![255; 64 * 48 * 4]],
            times: vec![0.0, 0.1],
            pixelated: true,
        };
        let crop = Crop { x: 8, y: 0, width: 40, height: 30 };
        type State = (Screen, Zoom, Clip, Option<Crop>, Rect);
        let state: State = (Screen::default(), Zoom::default(), clip, Some(crop), Rect::NOTHING);
        let mut harness = Harness::builder().with_size([600.0, 400.0]).build_ui_state(
            |ui, (screen, zoom, clip, crop, stage): &mut State| {
                *stage = ui.max_rect();
                let picture = Picture { clip, frame: 0, size: (64, 48), square: false };
                let cropping = Cropping { crop, ratio: None, kept: None, exact: None };
                show(ui, screen, (*stage, *stage), &picture, zoom, Some(cropping));
            },
            state,
        );
        crate::widgets::test_fonts(&harness);
        harness.run_steps(2);
        let (shown, scale) = layout(harness.state().4, (64, 48), &harness.state().1);
        let (left, top) = (shown.left() + 8.0 * scale, shown.top());
        let cross = harness.get_by_label(&crate::l10n::fl!("whole-picture")).rect();
        // clear of where the pointer takes its top edge and corner
        assert!(cross.top() > top + GRAB && cross.left() > left + GRAB, "{cross:?}");
        // the size, before the cross, and the top edge over them
        let size = pos2(cross.left() - 10.0, cross.center().y);
        let beside = pos2(cross.center().x, top);
        // four pixels down
        let down = vec2(0.0, 4.0 * scale);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let hover = |harness: &mut Harness<'_, State>, at| {
            harness.event(egui::Event::PointerMoved(at));
            harness.step();
            harness.output().platform_output.cursor_icon
        };
        let drag = |harness: &mut Harness<'_, State>, from: Pos2| {
            harness.event(egui::Event::PointerMoved(from));
            harness.event(press(from, true));
            harness.step();
            for step in 1..=4 {
                harness.event(egui::Event::PointerMoved(from + down * step as f32 / 4.0));
                harness.step();
            }
            harness.event(press(from + down, false));
            harness.step();
            harness.step();
            harness.state().3
        };
        assert_eq!(hover(&mut harness, cross.center()), CursorIcon::PointingHand);
        assert_eq!(hover(&mut harness, size), CursorIcon::Default);
        assert_eq!(hover(&mut harness, beside), CursorIcon::ResizeVertical);
        // dragged from the cross or the size, nothing moves
        assert_eq!(drag(&mut harness, cross.center()), Some(crop));
        assert_eq!(drag(&mut harness, size), Some(crop));
        // the edge over them moves
        let moved = Crop { y: 4, height: 26, ..crop };
        assert_eq!(drag(&mut harness, beside), Some(moved));
        // and a click on the cross is all of the picture
        harness.state_mut().3 = Some(crop);
        harness.run_steps(2);
        harness.event(egui::Event::PointerMoved(cross.center()));
        harness.event(press(cross.center(), true));
        harness.step();
        harness.event(press(cross.center(), false));
        harness.step();
        assert_eq!(harness.state().3, None);
    }

    type Cropped = (Screen, Zoom, Clip, Option<Crop>, Rect);

    /// A harness of a 64 × 48 picture of pixel art on a 600 × 400 stage,
    /// cropped to `crop`, and the scale it shows at and where its top left
    /// corner is.
    fn cropped(crop: Option<Crop>) -> (egui_kittest::Harness<'static, Cropped>, f32, Pos2) {
        cropped_of((64, 48), crop)
    }

    /// As [`cropped`], of a picture `size` large.
    fn cropped_of(
        size: (u32, u32),
        crop: Option<Crop>,
    ) -> (egui_kittest::Harness<'static, Cropped>, f32, Pos2) {
        cropped_with(size, crop, None, None)
    }

    /// As [`cropped_of`], the crop keeping `ratio` and held to `exact`.
    fn cropped_with(
        size: (u32, u32),
        crop: Option<Crop>,
        ratio: Option<f64>,
        exact: Option<Exact>,
    ) -> (egui_kittest::Harness<'static, Cropped>, f32, Pos2) {
        let (width, height) = (size.0 as usize, size.1 as usize);
        let clip = Clip {
            id: 1,
            width: size.0,
            height: size.1,
            frames: vec![vec![255; width * height * 4]],
            times: vec![0.0, 0.1],
            pixelated: true,
        };
        let state: Cropped = (Screen::default(), Zoom::default(), clip, crop, Rect::NOTHING);
        let mut harness =
            egui_kittest::Harness::builder().with_size([600.0, 400.0]).build_ui_state(
                move |ui, (screen, zoom, clip, crop, stage): &mut Cropped| {
                    *stage = ui.max_rect();
                    let picture = Picture { clip, frame: 0, size, square: false };
                    let cropping = Cropping { crop, ratio, kept: None, exact };
                    show(ui, screen, (*stage, *stage), &picture, zoom, Some(cropping));
                },
                state,
            );
        crate::widgets::test_fonts(&harness);
        harness.run_steps(2);
        let (shown, scale) = layout(harness.state().4, size, &harness.state().1);
        (harness, scale, shown.min)
    }

    /// Clicks inside the crop of `harness`, at `at`, which gives it the
    /// keyboard's focus.
    fn focus_crop(harness: &mut egui_kittest::Harness<'_, Cropped>, at: Pos2) {
        harness.event(egui::Event::PointerMoved(at));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.step();
        }
        harness.step();
    }

    /// The crop of `harness` once `key` is pressed with `modifiers`.
    fn keyed_to(
        harness: &mut egui_kittest::Harness<'_, Cropped>,
        modifiers: egui::Modifiers,
        key: egui::Key,
    ) -> Crop {
        harness.key_press_modifiers(modifiers, key);
        harness.step();
        harness.state().3.unwrap()
    }

    /// Presses the primary button at `from`, held with `modifiers`, and
    /// moves the pointer by `by` in four steps, without letting go.
    fn press_and_move(
        harness: &mut egui_kittest::Harness<'_, Cropped>,
        from: Pos2,
        by: Vec2,
        modifiers: egui::Modifiers,
    ) {
        harness.event(egui::Event::ModifiersChanged(modifiers));
        harness.event(egui::Event::PointerMoved(from));
        harness.event(egui::Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        harness.step();
        for step in 1..=4 {
            harness.event(egui::Event::PointerMoved(from + by * step as f32 / 4.0));
            harness.step();
        }
    }

    /// Lets go of the primary button at `at`.
    fn let_go(harness: &mut egui_kittest::Harness<'_, Cropped>, at: Pos2) {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        harness.step();
    }

    #[test]
    fn takes_a_drag_back_on_escape() {
        let crop = Crop { x: 8, y: 8, width: 30, height: 20 };
        let (mut harness, scale, corner) = cropped(Some(crop));
        // its right edge dragged out, then Escape, then on: as it was
        let right = corner + vec2(38.0, 18.0) * scale;
        press_and_move(&mut harness, right, vec2(60.0, 0.0), egui::Modifiers::NONE);
        assert_ne!(harness.state().3, Some(crop));
        harness.key_press(egui::Key::Escape);
        harness.step();
        assert_eq!(harness.state().3, Some(crop));
        harness.event(egui::Event::PointerMoved(right + vec2(90.0, 0.0)));
        harness.step();
        let_go(&mut harness, right + vec2(90.0, 0.0));
        assert_eq!(harness.state().3, Some(crop));
        // a new one drawn, then Escape: none
        let (mut harness, scale, corner) = cropped(None);
        let from = corner + vec2(4.0, 4.0) * scale;
        press_and_move(&mut harness, from, vec2(80.0, 60.0), egui::Modifiers::NONE);
        assert!(harness.state().3.is_some());
        harness.key_press(egui::Key::Escape);
        harness.step();
        let_go(&mut harness, from + vec2(80.0, 60.0));
        assert_eq!(harness.state().3, None);
    }

    #[test]
    fn keeps_the_shape_with_shift_and_the_middle_with_alt() {
        // 3:2, its bottom right corner dragged with Shift: still 3:2
        let crop = Crop { x: 8, y: 8, width: 30, height: 20 };
        let (mut harness, scale, corner) = cropped(Some(crop));
        let bottom_right = corner + vec2(38.0, 28.0) * scale;
        press_and_move(&mut harness, bottom_right, vec2(9.0, 2.0) * scale, egui::Modifiers::SHIFT);
        let_go(&mut harness, bottom_right + vec2(9.0, 2.0) * scale);
        let shaped = harness.state().3.unwrap();
        assert_eq!((shaped.x, shaped.y), (8, 8), "{shaped:?}");
        assert_eq!(shaped.width * 2, shaped.height * 3, "{shaped:?}");
        assert!(shaped.width > 30, "{shaped:?}");
        // without, it follows the pointer
        let (mut harness, ..) = cropped(Some(crop));
        press_and_move(&mut harness, bottom_right, vec2(9.0, 2.0) * scale, egui::Modifiers::NONE);
        let_go(&mut harness, bottom_right + vec2(9.0, 2.0) * scale);
        assert_eq!(harness.state().3, Some(Crop { width: 39, height: 22, ..crop }));
        // its right edge dragged with Alt: the left edge goes the other way
        let (mut harness, ..) = cropped(Some(crop));
        let right = corner + vec2(38.0, 18.0) * scale;
        press_and_move(&mut harness, right, vec2(4.0, 0.0) * scale, egui::Modifiers::ALT);
        let_go(&mut harness, right + vec2(4.0, 0.0) * scale);
        assert_eq!(harness.state().3, Some(Crop { x: 4, width: 38, ..crop }));
    }

    #[test]
    fn snaps_to_the_pictures_edges_and_middle_unless_ctrl_is_held() {
        let crop = Crop { x: 100, y: 100, width: 200, height: 150 };
        let size = (640, 480);
        // whether a line of the accent is drawn
        let guided = |harness: &egui_kittest::Harness<'_, Cropped>| {
            harness.output().shapes.iter().any(|clipped| match &clipped.shape {
                egui::Shape::LineSegment { stroke, .. } => {
                    stroke.color == theme::mark(&harness.ctx.global_style().visuals)
                }
                _ => false,
            })
        };
        // moved to five pixels from the left edge: onto it
        let moves = |by: Vec2, modifiers| {
            let (mut harness, scale, corner) = cropped_of(size, Some(crop));
            let inside = corner + vec2(150.0, 150.0) * scale;
            press_and_move(&mut harness, inside, by * scale, modifiers);
            let guide = guided(&harness);
            let_go(&mut harness, inside + by * scale);
            (harness.state().3.unwrap(), guide)
        };
        let (snapped, guide) = moves(vec2(-95.0, 0.0), egui::Modifiers::NONE);
        assert_eq!((snapped.x, snapped.y), (0, 100));
        assert!(guide);
        // not with Ctrl held
        let (free, guide) = moves(vec2(-95.0, 0.0), egui::Modifiers::COMMAND);
        assert_eq!((free.x, free.y), (5, 100));
        assert!(!guide);
        // its middle onto the picture's, from four pixels off it
        let (middle, _) = moves(vec2(116.0, 0.0), egui::Modifiers::NONE);
        assert_eq!(middle.x, 220);
        // and far from any line, none
        let (away, guide) = moves(vec2(40.0, 30.0), egui::Modifiers::NONE);
        assert_eq!((away.x, away.y), (140, 130));
        assert!(!guide);
        // an edge dragged to near the picture's middle: onto it
        let (mut harness, scale, corner) = cropped_of(size, Some(crop));
        let right = corner + vec2(300.0, 175.0) * scale;
        press_and_move(&mut harness, right, vec2(23.0, 0.0) * scale, egui::Modifiers::NONE);
        let_go(&mut harness, right + vec2(23.0, 0.0) * scale);
        assert_eq!(harness.state().3, Some(Crop { width: 220, ..crop }));
    }

    #[test]
    fn moves_and_resizes_the_crop_by_keys_once_it_is_clicked() {
        let crop = Crop { x: 8, y: 8, width: 30, height: 20 };
        let (mut harness, scale, corner) = cropped(Some(crop));
        let id = harness.ctx.memory(|memory| memory.focused());
        assert_eq!(id, None);
        // clicked inside: it has the focus
        let inside = corner + vec2(20.0, 20.0) * scale;
        harness.event(egui::Event::PointerMoved(inside));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: inside,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.step();
        }
        harness.step();
        assert!(harness.ctx.memory(|memory| memory.focused()).is_some());
        let keyed = |harness: &mut egui_kittest::Harness<'_, Cropped>, modifiers, key| {
            harness.key_press_modifiers(modifiers, key);
            harness.step();
            harness.state().3.unwrap()
        };
        use egui::{Key, Modifiers};
        assert_eq!(keyed(&mut harness, Modifiers::NONE, Key::ArrowRight), Crop { x: 9, ..crop });
        assert_eq!(
            keyed(&mut harness, Modifiers::SHIFT, Key::ArrowDown),
            Crop { x: 9, y: 18, ..crop }
        );
        let moved = Crop { x: 9, y: 18, ..crop };
        // with Ctrl, its bottom right corner, as far as the picture goes
        let wider = Crop { width: 31, ..moved };
        assert_eq!(keyed(&mut harness, Modifiers::COMMAND, Key::ArrowRight), wider);
        let lower = Crop { height: 30, ..wider };
        let both = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(keyed(&mut harness, both, Key::ArrowDown), lower);
        assert_eq!(keyed(&mut harness, both, Key::ArrowDown), lower);
        // still there after the arrows, which move nothing else's focus
        assert!(harness.ctx.memory(|memory| memory.focused()).is_some());
        // Escape lets go of it, the arrows doing nothing to it after
        harness.key_press(Key::Escape);
        harness.step();
        assert_eq!(harness.ctx.memory(|memory| memory.focused()), None);
        assert_eq!(keyed(&mut harness, Modifiers::NONE, Key::ArrowLeft), lower);
    }

    #[test]
    fn resizes_a_crop_keeping_its_shape_by_keys_only_as_far_as_the_picture_goes() {
        use egui::{Key, Modifiers};
        // square, on the picture's bottom edge: wider would be taller, past
        // it, so it stays where it is, as large as it is
        let crop = Crop { x: 8, y: 18, width: 30, height: 30 };
        let (mut harness, scale, corner) = cropped_with((64, 48), Some(crop), Some(1.0), None);
        focus_crop(&mut harness, corner + vec2(20.0, 30.0) * scale);
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowRight), crop);
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowDown), crop);
        let smaller = Crop { width: 29, height: 29, ..crop };
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowLeft), smaller);
        // and up to the picture's edge, from its top left corner
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowDown), crop);
        // on its right edge, likewise
        let crop = Crop { x: 34, y: 4, width: 30, height: 30 };
        let (mut harness, scale, corner) = cropped_with((64, 48), Some(crop), Some(1.0), None);
        focus_crop(&mut harness, corner + vec2(50.0, 20.0) * scale);
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowDown), crop);
        let both = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(keyed_to(&mut harness, both, Key::ArrowDown), crop);
    }

    #[test]
    fn resizes_a_crop_held_to_whole_blocks_by_keys_a_block_at_a_time() {
        use egui::{Key, Modifiers};
        // halved, in blocks of 2 × 2, 40 across at most
        let crop = Crop { x: 8, y: 8, width: 20, height: 20 };
        let exact = Some(Exact { long: 40, step: 2 });
        let (mut harness, scale, corner) = cropped_with((64, 48), Some(crop), None, exact);
        focus_crop(&mut harness, corner + vec2(18.0, 18.0) * scale);
        let wider = Crop { width: 22, ..crop };
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowRight), wider);
        let lower = Crop { height: 22, ..wider };
        assert_eq!(keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowDown), lower);
        assert_eq!(
            keyed_to(&mut harness, Modifiers::COMMAND, Key::ArrowLeft),
            Crop { width: 20, ..lower }
        );
        // ten blocks with Shift, as far as the scale takes
        let both = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(keyed_to(&mut harness, both, Key::ArrowRight).width, 40);
        // and moved a pixel at a time still
        assert_eq!(keyed_to(&mut harness, Modifiers::NONE, Key::ArrowRight).x, 9);
    }

    #[test]
    fn rings_the_crop_only_once_the_arrows_move_it() {
        let crop = Crop { x: 8, y: 8, width: 30, height: 20 };
        let (mut harness, scale, corner) = cropped(Some(crop));
        let focused = |harness: &egui_kittest::Harness<'_, Cropped>| {
            (crop_focused(&harness.ctx), crop_ringed(&harness.ctx))
        };
        // moved by a drag: the focus, and no ring
        let inside = corner + vec2(20.0, 15.0) * scale;
        press_and_move(&mut harness, inside, vec2(2.0, 0.0) * scale, egui::Modifiers::NONE);
        assert_eq!(focused(&harness), (true, false));
        let_go(&mut harness, inside + vec2(2.0, 0.0) * scale);
        harness.step();
        assert_eq!(focused(&harness), (true, false));
        // moved by an arrow: the ring
        harness.key_press(egui::Key::ArrowRight);
        harness.step();
        assert_eq!(harness.state().3, Some(Crop { x: 11, ..crop }));
        assert_eq!(focused(&harness), (true, true));
        // pressed again, and clicked: the focus kept, the ring gone
        let inside = corner + vec2(23.0, 15.0) * scale;
        harness.event(egui::Event::PointerMoved(inside));
        harness.event(egui::Event::PointerButton {
            pos: inside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
        assert!(!crop_ringed(&harness.ctx));
        let_go(&mut harness, inside);
        harness.step();
        assert_eq!(focused(&harness), (true, false));
        // and once more by an arrow, then let go of with Escape
        harness.key_press(egui::Key::ArrowLeft);
        harness.step();
        assert_eq!(focused(&harness), (true, true));
        harness.key_press(egui::Key::Escape);
        harness.step();
        assert_eq!(focused(&harness), (false, false));
    }

    #[test]
    fn takes_corners_from_further_than_edges_and_leaves_a_small_crop_its_middle() {
        let crop = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 100.0));
        let corner = [true, true, false, false];
        // a corner, from 9 points out both ways, where an edge isn't taken
        assert_eq!(edges_at(pos2(91.0, 91.0), crop), Some(corner));
        assert_eq!(edges_at(pos2(91.0, 150.0), crop), None);
        // from beside it on either edge, too
        assert_eq!(edges_at(pos2(109.0, 95.0), crop), Some(corner));
        assert_eq!(edges_at(pos2(150.0, 95.0), crop), Some([false, true, false, false]));
        assert_eq!(edges_at(pos2(150.0, 150.0), crop), None);
        // a crop 16 points across: its edges and corners reach 4 into it,
        // and its middle is left to move it by
        let small = Rect::from_min_size(pos2(100.0, 100.0), vec2(16.0, 16.0));
        assert_eq!(edges_at(pos2(108.0, 108.0), small), None);
        assert_eq!(edges_at(pos2(103.0, 103.0), small), Some(corner));
        assert_eq!(edges_at(pos2(114.0, 108.0), small), Some([false, false, true, false]));
        assert_eq!(edges_at(pos2(90.0, 90.0), small), Some(corner));
    }

    #[test]
    fn puts_the_crops_size_clear_of_where_its_edges_are_taken() {
        let stage = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let crop = Crop { x: 0, y: 0, width: 10, height: 10 };
        let crops = [
            // with room over it
            Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 100.0)),
            // at the stage's top, large enough to hold it
            Rect::from_min_size(pos2(100.0, 10.0), vec2(200.0, 100.0)),
            // at the stage's top, too small to hold it
            Rect::from_min_size(pos2(100.0, 10.0), vec2(30.0, 30.0)),
            // as tall as the stage, and too narrow
            Rect::from_min_size(pos2(100.0, 10.0), vec2(30.0, 380.0)),
            // past the stage's bottom right, as a crop kept from a larger
            // picture is
            Rect::from_min_size(pos2(590.0, 380.0), vec2(40.0, 40.0)),
            Rect::from_min_size(pos2(700.0, 450.0), vec2(40.0, 40.0)),
        ];
        let mut tags = Vec::new();
        let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            tags = crops.map(|on_screen| size_tag(painter, crop, on_screen, stage).back).to_vec();
        });
        output.textures_delta.clear();
        let [over, within, under, pinned, past, beyond] = tags[..] else { panic!("{tags:?}") };
        for tag in [past, beyond] {
            assert!(stage.contains_rect(tag), "{tag:?}");
        }
        assert!(over.bottom() < crops[0].top() - GRAB, "{over:?}");
        assert!(within.top() > crops[1].top() + GRAB, "{within:?}");
        assert!(within.left() > crops[1].left() + GRAB, "{within:?}");
        assert!(crops[1].contains_rect(within), "{within:?}");
        assert!(under.top() > crops[2].bottom() + GRAB, "{under:?}");
        // and only where there is no room for any of those, over its edge
        assert!(pinned.y_range().contains(crops[3].top()), "{pinned:?}");
    }

    #[test]
    fn zooms_by_a_quarter_a_notch_and_finer_by_points() {
        let zoomed = |unit, delta: f32| {
            let (mut harness, ..) = cropped(None);
            harness.event(egui::Event::PointerMoved(pos2(300.0, 200.0)));
            harness.event(egui::Event::MouseWheel {
                unit,
                delta: vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run_steps(60);
            harness.state().1.level
        };
        let notch = zoomed(egui::MouseWheelUnit::Line, 1.0);
        assert!((notch - 1.25).abs() < 0.01, "{notch}");
        let two = zoomed(egui::MouseWheelUnit::Line, 2.0);
        assert!((two - 1.25 * 1.25).abs() < 0.01, "{two}");
        // a touchpad's 40 points, as far as egui scrolls by a notch
        let points = zoomed(egui::MouseWheelUnit::Point, 40.0);
        assert!((points - (40.0 * PER_POINT).exp()).abs() < 0.01, "{points}");
    }

    #[test]
    fn zooms_in_until_a_pixel_takes_32_of_the_screens() {
        // a sticker, fitted a little larger than its pixels
        assert_eq!(most_level(1.25), 25.6);
        // a photo, fitted at a tenth
        assert_eq!(most_level(0.1), 320.0);
        // a sprite, fitted 30 times larger: four times that still
        assert_eq!(most_level(30.0), 4.0);
        // never short of where it fits
        assert_eq!(most_level(20_000.0), 1.0);
    }

    #[test]
    fn fits_pixel_art_a_whole_number_of_the_screens_pixels_to_its_own() {
        let stage = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        // 7.75 points a pixel where it fits, as smooth pictures stay
        assert_eq!(fit(stage, (64, 48), None), 7.75);
        assert_eq!(fit(stage, (64, 48), Some(1.0)), 7.0);
        // at a scale of 1.25, 9 of the screen's pixels, rather than 9.69
        assert_eq!(fit(stage, (64, 48), Some(1.25)), 9.0 / 1.25);
        // art of twice the input's pixels: 3 of the screen's pixels to
        // each of its own, where 3.875 would fit
        assert_eq!(fit(stage, (64, 48), Some(0.5)), 6.0);
        // under two of the screen's pixels, as it fits
        let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        assert_eq!(fit(small, (64, 48), Some(1.0)), 72.0 / 64.0);
        // as the preview shows it
        let (harness, scale, _) = cropped(None);
        assert_eq!((scale, harness.state().1.scale), (7.0, Some(7.0)));
    }

    #[test]
    fn eases_out_to_all_of_it_and_pans_across_with_shift() {
        let (mut harness, ..) = cropped(None);
        let zoomed = Zoom { level: 4.0, centre: vec2(0.3, 0.6), ..Zoom::default() };
        harness.state_mut().1 = zoomed;
        harness.step();
        // the wheel scrolled with Shift, across: panned that way
        harness.event(egui::Event::PointerMoved(pos2(300.0, 200.0)));
        let centre = harness.state().1.centre;
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, -1.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::SHIFT,
        });
        harness.run_steps(30);
        let panned = harness.state().1;
        assert!(panned.centre.x > centre.x + 0.01, "{panned:?}");
        assert_eq!((panned.level, panned.centre.y), (4.0, centre.y));
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        // out, a step at a time, to where it fits
        let now = harness.ctx.input(|input| input.time);
        harness.state_mut().1.zoom_out(now, true);
        let mut levels = vec![];
        for _ in 0..20 {
            harness.step();
            levels.push(harness.state().1.level);
        }
        assert!(levels.windows(2).all(|pair| pair[1] <= pair[0]), "{levels:?}");
        assert!(levels.iter().filter(|level| (1.0..4.0).contains(*level)).count() >= 3);
        assert_eq!(harness.state().1.level, 1.0);
        assert_eq!(harness.state().1.centre, vec2(0.5, 0.5));
        // or at once, without motion
        harness.state_mut().1 = zoomed;
        harness.step();
        harness.state_mut().1.zoom_out(now, false);
        assert_eq!((harness.state().1.level, harness.state().1.centre), (1.0, vec2(0.5, 0.5)));
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
        let free = (None, false);
        let wider = resized(crop, [false, false, true, false], pos2(170.0, 0.0), free, SIZE);
        assert_eq!(wider, Some(Crop { width: 120, ..crop }));
        // the top left corner, square: the larger side wins, shrunk to fit
        let square =
            resized(crop, [true, true, false, false], pos2(40.0, 10.0), (Some(1.0), false), SIZE)
                .unwrap();
        assert_eq!(square, Crop { x: 40, y: 0, width: 100, height: 100 });
        // an edge keeps the ratio around the middle
        let wide =
            resized(crop, [false, false, true, false], pos2(170.0, 0.0), (Some(2.0), false), SIZE)
                .unwrap();
        assert_eq!((wide.width, wide.height, wide.y), (120, 60, 20));
        // about its middle: the left edge goes out as far as the right
        let right = [false, false, true, false];
        let both = resized(crop, right, pos2(160.0, 0.0), (None, true), SIZE).unwrap();
        assert_eq!(both, Crop { x: 40, width: 120, ..crop });
        // only as far as the nearer edge of the picture
        let most = resized(crop, right, pos2(200.0, 0.0), (None, true), SIZE).unwrap();
        assert_eq!(most, Crop { x: 0, width: 200, ..crop });
        // a corner, square, about the middle, shrunk to fit the picture
        let corner = [false, false, true, true];
        let square = resized(crop, corner, pos2(160.0, 60.0), (Some(1.0), true), SIZE).unwrap();
        assert_eq!(square, Crop { x: 50, y: 0, width: 100, height: 100 });
    }

    #[test]
    fn draws_new_crops_from_either_corner() {
        assert_eq!(drawn(pos2(50.0, 50.0), pos2(51.0, 50.5), None, SIZE), None);
        let drawn = drawn(pos2(100.0, 80.0), pos2(60.0, 20.0), Some(1.0), SIZE).unwrap();
        assert_eq!(drawn, Crop { x: 40, y: 20, width: 60, height: 60 });
    }

    #[test]
    fn holds_crops_to_an_integer_scale() {
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
        assert!(Exact::of(IntegerScale { up: 513, down: 1 }, Target::Sticker).is_none());
    }
}

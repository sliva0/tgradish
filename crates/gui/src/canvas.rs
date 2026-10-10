//! The big preview: the input with its crop, which is edited by dragging,
//! or the result. The wheel zooms, the right or middle button pans.

use eframe::egui::{
    self, Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, Stroke, pos2, vec2,
};
use tgradish_core::options::Crop;

use crate::item::Zoom;
use crate::media::Clip;
use crate::widgets;

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

/// The small picture of what the result will look like, over the input.
#[derive(Default)]
pub struct Inset {
    texture: Option<egui::TextureHandle>,
    /// What the texture shows, and whether it is drawn sharp.
    key: Option<(u64, bool)>,
}

impl Inset {
    /// Shows the picture `make` makes, made again only when `key` changes,
    /// in the bottom right corner of `area`, labelled.
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        area: Rect,
        key: u64,
        label: &str,
        make: impl FnOnce() -> egui::ColorImage,
    ) {
        let ppp = ui.ctx().pixels_per_point();
        let room = vec2((area.width() * 0.34).max(110.0), (area.height() * 0.42).max(110.0));
        // the size of the image isn't known before it is made, so the
        // last one's is used to choose its filtering
        let last = self.texture.as_ref().map(|texture| texture.size_vec2());
        let fits = |size: egui::Vec2| (room.x / size.x).min(room.y / size.y).min(1.0 / ppp);
        let sharp = last.is_some_and(|size| fits(size) * ppp >= 1.0);
        if self.key != Some((key, sharp)) || self.texture.is_none() {
            let options =
                if sharp { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
            let image = make();
            match &mut self.texture {
                Some(texture) => texture.set(image, options),
                None => self.texture = Some(ui.ctx().load_texture("inset", image, options)),
            }
            self.key = Some((key, sharp));
        }
        let texture = self.texture.as_ref().expect("set above");
        let size = texture.size_vec2();
        let shown = size * fits(size);
        let frame = Rect::from_min_size(area.right_bottom() - shown - vec2(MARGIN, MARGIN), shown);
        let painter = ui.painter_at(area);
        painter.rect_filled(frame.expand(4.0), 4.0, Color32::from_black_alpha(200));
        widgets::checkerboard(&painter, frame, 6.0);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(texture.id(), frame, uv, Color32::WHITE);
        let galley =
            painter.layout_no_wrap(label.into(), egui::FontId::proportional(12.0), Color32::WHITE);
        let at = pos2(frame.left(), frame.top() - galley.size().y - 8.0);
        painter.rect_filled(
            Rect::from_min_size(at, galley.size()).expand(3.0),
            3.0,
            Color32::from_black_alpha(200),
        );
        painter.galley(at, galley, Color32::WHITE);
    }
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
    let fit = ((rect.width() - 2.0 * MARGIN) / width).min((rect.height() - 2.0 * MARGIN) / height);
    let scale = fit.max(1e-3) * zoom.level;
    let shown = vec2(width, height) * scale;
    let min = rect.center() - vec2(zoom.centre.x * shown.x, zoom.centre.y * shown.y);
    (Rect::from_min_size(min, shown), scale)
}

/// Screen pixels to a picture pixel from which pixels show as squares:
/// a smooth picture blurs them, and you can't put a crop on a pixel's edge.
const SHARP_FROM: f32 = 2.0;

/// Shows `picture` in `rect`, with `cropping` over it if given. True when
/// the crop changed.
pub fn show(
    ui: &mut egui::Ui,
    screen: &mut Screen,
    rect: Rect,
    picture: &Picture,
    zoom: &mut Zoom,
    cropping: Option<Cropping>,
) -> bool {
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(24));

    let side = picture.size.0.max(picture.size.1);
    let canvas = if picture.square { (side, side) } else { picture.size };
    zoom_and_pan(ui, &response, rect, canvas, zoom);
    let (whole, scale) = layout(rect, canvas, zoom);
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

    let Some(cropping) = cropping else { return false };
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
    let mut changed = false;
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
        let new = match drag {
            Drag::Edges(edges, start) => resized(start, edges, point, cropping.ratio, size),
            Drag::Move(start, from) => Some(moved(start, point - from, size)),
            Drag::New(corner) => drawn(corner, point, cropping.ratio, size),
        };
        if let Some(new) = new {
            let new = (new != full).then_some(new);
            if new != *cropping.crop {
                *cropping.crop = new;
                changed = true;
            }
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<Drag>(id));
    }
    if response.double_clicked() && cropping.crop.is_some() {
        *cropping.crop = None;
        changed = true;
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
    changed
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
    if zoom.level <= 1.0 {
        zoom.centre = vec2(0.5, 0.5);
    }
    zoom.centre = zoom.centre.clamp(vec2(0.0, 0.0), vec2(1.0, 1.0));
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

//! Painting helpers: transparency checkerboards, cased ticks and outlines,
//! and hatching.

use eframe::egui::{self, Color32};

use crate::theme;

/// Draws a transparency checkerboard over `rect` anchored at `rect.min`.
pub(crate) fn checkerboard(painter: &egui::Painter, rect: egui::Rect, square: f32) {
    checkerboard_from(painter, rect, square, rect.min);
}

/// Draws a transparency checkerboard over `rect` with squares aligned to
/// `origin` so panning or zooming an image moves over a stationary stage grid.
pub(crate) fn checkerboard_from(
    painter: &egui::Painter,
    rect: egui::Rect,
    square: f32,
    origin: egui::Pos2,
) {
    let (light, dark) = if painter.ctx().global_style().visuals.dark_mode {
        (Color32::from_gray(92), Color32::from_gray(72))
    } else {
        (Color32::from_gray(240), Color32::from_gray(212))
    };
    painter.rect_filled(rect, 0.0, light);
    let clip = painter.clip_rect().intersect(rect);
    if clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    // Snap the origin to whole physical pixels so square edges stay crisp.
    let origin = egui::emath::GuiRounding::round_to_pixels(origin, painter.pixels_per_point());
    // Iterate only over the grid cells intersecting the clip rect.
    let first_column = ((clip.left() - origin.x) / square).floor() as i64;
    let first_row = ((clip.top() - origin.y) / square).floor() as i64;
    let columns = ((clip.right() - origin.x) / square).ceil() as i64;
    let rows = ((clip.bottom() - origin.y) / square).ceil() as i64;
    for row in first_row..rows {
        let start = first_column + (row + first_column).rem_euclid(2);
        for column in (start..columns).step_by(2) {
            let min = origin + egui::vec2(column as f32 * square, row as f32 * square);
            let cell = egui::Rect::from_min_size(min, egui::vec2(square, square)).intersect(rect);
            if cell.is_positive() {
                painter.rect_filled(cell, 0.0, dark);
            }
        }
    }
}

/// Draws a vertical marker tick at `x` spanning `top..=bottom` with a 2 pt core
/// and 1 pt contrasting casing (via [`theme::over_fill`]) snapped to whole
/// screen pixels so it remains sharp over any background or accent colour.
/// Returns the `(left, right)` outer X coordinates.
pub(crate) fn tick(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    x: f32,
    (top, bottom): (f32, f32),
) -> (f32, f32) {
    let cased = theme::over_fill(visuals);
    let ppp = painter.pixels_per_point();
    let pixels = |points: f32| (points * ppp).round().max(1.0);
    let (core, casing) = (pixels(2.0), pixels(1.0));
    // Snap the core's left edge to a physical pixel boundary centred on `x`.
    let left = (x * ppp - core / 2.0).round();
    let rect = |left: f32, right: f32, grow: f32| {
        egui::Rect::from_min_max(
            egui::pos2(left / ppp, ((top * ppp).round() - grow) / ppp),
            egui::pos2(right / ppp, ((bottom * ppp).round() + grow) / ppp),
        )
    };
    let outer = rect(left - casing, left + core + casing, casing);
    painter.rect_filled(outer, 0.0, cased.casing);
    painter.rect_filled(rect(left, left + core, 0.0), 0.0, cased.core);
    (outer.left(), outer.right())
}

/// Draws a double-stroked outline inside `rect` using [`theme::over_fill`]
/// (outer core, inner casing) so it stays visible over any fill or accent hue.
pub(crate) fn cased_outline(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: egui::Rect,
    radius: f32,
) {
    let cased = theme::over_fill(visuals);
    let line = |colour| egui::Stroke::new(1.0, colour);
    painter.rect_stroke(rect, radius, line(cased.core), egui::StrokeKind::Inside);
    let inner = rect.shrink(1.0);
    if inner.is_positive() {
        let radius = (radius - 1.0).max(0.0);
        painter.rect_stroke(inner, radius, line(cased.casing), egui::StrokeKind::Inside);
    }
}

/// Fills `rect` with 45° right-leaning diagonal stripes of `colour` (equal
/// stripe and gap widths) anchored at X coordinate `origin` so the pattern
/// stays stationary as `rect`'s edges move.
pub(crate) fn hatch(painter: &egui::Painter, rect: egui::Rect, origin: f32, colour: Color32) {
    /// Horizontal period between stripe centres along the bar.
    const PERIOD: f32 = 6.0;
    let clip = painter.clip_rect().intersect(rect);
    if !clip.is_positive() {
        return;
    }
    let painter = painter.with_clip_rect(clip);
    // At 45°, perpendicular stroke width is half the horizontal period divided by √2.
    let stroke = egui::Stroke::new(PERIOD / 2.0 / std::f32::consts::SQRT_2, colour);
    let rise = rect.height();
    let first = ((clip.left() - rise - origin) / PERIOD).floor() as i64;
    let last = ((clip.right() - origin) / PERIOD).ceil() as i64;
    for stripe in first..=last {
        let x = origin + stripe as f32 * PERIOD;
        let from = egui::pos2(x, rect.bottom() + 1.0);
        let to = egui::pos2(x + rise + 2.0, rect.top() - 1.0);
        painter.line_segment([from, to], stroke);
    }
}

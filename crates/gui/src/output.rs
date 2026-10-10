//! What a result will look like, worked out on a frame of the input: the
//! part used, scaled the way the conversion scales it, in its box. Shown
//! small over the input, so the effect of the crop and the scaling shows
//! without converting.

use eframe::egui;
use tgradish_core::convert::Sizes;
use tgradish_core::options::{Crop, Scaling};

/// The part of a frame used, in the frame's own pixels, which may be fewer
/// than the input's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Part {
    /// `crop`, in input pixels of an `input` sized picture, on a frame of
    /// `frame` pixels.
    pub fn of(crop: Option<Crop>, input: (u32, u32), frame: (u32, u32)) -> Part {
        let crop = crop.unwrap_or(Crop { x: 0, y: 0, width: input.0, height: input.1 });
        let (sx, sy) = (
            f64::from(frame.0) / f64::from(input.0.max(1)),
            f64::from(frame.1) / f64::from(input.1.max(1)),
        );
        Part {
            x: f64::from(crop.x) * sx,
            y: f64::from(crop.y) * sy,
            width: f64::from(crop.width) * sx,
            height: f64::from(crop.height) * sy,
        }
    }
}

/// Whether `part` of `clip` looks like pixel art, as conversions decide
/// when scaling is automatic.
pub fn looks_like_art(clip: &crate::media::Clip, part: Part) -> bool {
    let crop = Crop {
        x: part.x as u32,
        y: part.y as u32,
        width: (part.width.round() as u32).max(1),
        height: (part.height.round() as u32).max(1),
    };
    let frames = &clip.frames[..clip.frames.len().min(3)];
    tgradish_core::convert::is_pixel_art(frames, clip.width, Some(crop))
}

/// sRGB channel values in linear light.
fn to_linear() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|value| {
            let c = value as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        })
    })
}

fn to_srgb(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let c = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0).round() as u8
}

/// Weights of the source pixels an output pixel covers, along one side:
/// `(first, weights)`, from the overlap of source pixels with the range.
fn area(from: f64, to: f64, size: usize) -> (usize, Vec<f32>) {
    let first = (from.floor().max(0.0) as usize).min(size - 1);
    let last = (to.ceil() as usize).clamp(first + 1, size);
    let weights = (first..last)
        .map(|i| {
            let overlap = (to.min(i as f64 + 1.0) - from.max(i as f64)).max(0.0);
            overlap as f32
        })
        .collect();
    (first, weights)
}

/// The WebM result of `frame` (`width` pixels wide, straight RGBA):
/// `part` scaled to `sizes.scaled` the way `sizes.scaling` does, cropped
/// or padded around the middle to the result's size.
pub fn webm(frame: &[u8], width: u32, part: Part, sizes: &Sizes) -> egui::ColorImage {
    let height = (frame.len() / 4) as u32 / width.max(1);
    let (out_w, out_h) = (sizes.width as usize, sizes.height as usize);
    let (sw, sh) = (sizes.scaled.0 as f64, sizes.scaled.1 as f64);
    // where the scaled picture starts in the result; padding keeps chroma
    // planes whole, so it starts on even pixels
    let offset = |out: usize, scaled: f64| ((out as f64 - scaled) / 2.0 / 2.0).floor() * 2.0;
    let (left, top) = (offset(out_w, sw), offset(out_h, sh));
    let linear = to_linear();
    // source pixels and weights along each side, for each output column and row
    let along = |out: usize, start: f64, scaled: f64, from: f64, used: f64, size: u32| {
        let size = size.max(1) as usize;
        (0..out)
            .map(|o| {
                let at = o as f64 - start;
                if at < 0.0 || at >= scaled {
                    return None;
                }
                let per = used / scaled;
                Some(match sizes.scaling {
                    Scaling::PixelPerfect => {
                        let i = (from + ((at + 0.5) * per).floor()).min(size as f64 - 1.0);
                        (i.max(0.0) as usize, vec![1.0])
                    }
                    Scaling::Sharp => {
                        // blocks of whole source pixels, then averaged down
                        let k = (scaled / used).ceil().max(1.0);
                        let block = used * k / scaled;
                        let (a, b) = (at * block / k, (at + 1.0) * block / k);
                        area(from + a, from + b, size)
                    }
                    Scaling::Smooth | Scaling::Auto if per >= 1.0 => {
                        area(from + at * per, from + (at + 1.0) * per, size)
                    }
                    Scaling::Smooth | Scaling::Auto => {
                        // growing: between the two nearest pixel centres,
                        // within the part, which is cropped before scaling
                        let lowest = from.floor().clamp(0.0, size as f64 - 1.0);
                        let highest = ((from + used).ceil() - 1.0).clamp(lowest, size as f64 - 1.0);
                        let centre = from + (at + 0.5) * per - 0.5;
                        let first = centre.floor().clamp(lowest, highest);
                        let t = (centre - first).clamp(0.0, 1.0) as f32;
                        let second_weight = if first < highest { t } else { 0.0 };
                        (first as usize, vec![1.0 - second_weight, second_weight])
                    }
                })
            })
            .collect::<Vec<_>>()
    };
    let columns = along(out_w, left, sw, part.x, part.width, width);
    let rows = along(out_h, top, sh, part.y, part.height, height);
    let mut pixels = vec![egui::Color32::TRANSPARENT; out_w * out_h];
    for (y, row) in rows.iter().enumerate() {
        let Some((first_row, row_weights)) = row else { continue };
        for (x, column) in columns.iter().enumerate() {
            let Some((first_column, column_weights)) = column else { continue };
            // premultiplied, in linear light
            let mut sum = [0f32; 4];
            let mut total = 0.0;
            for (dy, &wy) in row_weights.iter().enumerate() {
                for (dx, &wx) in column_weights.iter().enumerate() {
                    let weight = wx * wy;
                    if weight == 0.0 {
                        continue;
                    }
                    let (sx, sy) = (first_column + dx, first_row + dy);
                    if sx >= width as usize || sy >= height as usize {
                        continue;
                    }
                    let at = (sy * width as usize + sx) * 4;
                    let alpha = f32::from(frame[at + 3]) / 255.0;
                    for c in 0..3 {
                        sum[c] += linear[frame[at + c] as usize] * alpha * weight;
                    }
                    sum[3] += alpha * weight;
                    total += weight;
                }
            }
            if total == 0.0 || sum[3] == 0.0 {
                continue;
            }
            let alpha = sum[3] / total;
            let [r, g, b] = [0, 1, 2].map(|c| to_srgb(sum[c] / sum[3]));
            pixels[y * out_w + x] =
                egui::Color32::from_rgba_unmultiplied(r, g, b, (alpha * 255.0).round() as u8);
        }
    }
    egui::ColorImage {
        size: [out_w, out_h],
        pixels,
        source_size: egui::vec2(out_w as f32, out_h as f32),
    }
}

/// The `.tgs` result of pixel art: `part` of `frames`, cut to what is
/// visible in any frame unless `keep_canvas`, and scaled to fill the
/// 512 x 512 canvas along its longer side, pixel for pixel. Shown on a
/// smaller canvas of `side`, which keeps the art pixels whole when it can.
/// Left, top, right and bottom of what a `.tgs` result shows of `frames`:
/// `part`, cut to what is visible in any frame unless `keep_canvas`. Looks
/// at every pixel of every frame, so the caller keeps it.
pub fn tgs_bounds(frames: &[Vec<u8>], width: u32, part: Part, keep_canvas: bool) -> [usize; 4] {
    let height = (frames[0].len() / 4) as u32 / width.max(1);
    // a crop may lie outside the picture until it is fixed
    let (width_px, height_px) = (width as usize, height as usize);
    let (x0, y0) =
        ((part.x.round() as usize).min(width_px), (part.y.round() as usize).min(height_px));
    let x1 = ((part.x + part.width).round() as usize).clamp(x0, width_px);
    let y1 = ((part.y + part.height).round() as usize).clamp(y0, height_px);
    if keep_canvas {
        return [x0, y0, x1, y1];
    }
    let (mut left, mut top, mut right, mut bottom) = (x1, y1, x0, y0);
    for rgba in frames {
        for y in y0..y1 {
            for x in x0..x1 {
                if rgba[(y * width_px + x) * 4 + 3] > 0 {
                    (left, top) = (left.min(x), top.min(y));
                    (right, bottom) = (right.max(x + 1), bottom.max(y + 1));
                }
            }
        }
    }
    if right > left && bottom > top { [left, top, right, bottom] } else { [x0, y0, x1, y1] }
}

/// The `.tgs` result of `rgba`, a frame `width` pixels wide: `bounds` of
/// it scaled to fill the canvas along its longer side, as the conversion
/// does, shown on a smaller canvas of `side`. Shapes have no pixels of
/// their own, so the scale needn't be whole.
pub fn tgs(rgba: &[u8], width: u32, [x0, y0, x1, y1]: [usize; 4], side: usize) -> egui::ColorImage {
    let (w, h) = ((x1 - x0).max(1), (y1 - y0).max(1));
    let scale = side as f64 / w.max(h) as f64;
    let (shown_w, shown_h) =
        ((w as f64 * scale).round() as usize, (h as f64 * scale).round() as usize);
    let (left, top) = ((side - shown_w.min(side)) / 2, (side - shown_h.min(side)) / 2);
    let mut pixels = vec![egui::Color32::TRANSPARENT; side * side];
    for y in 0..shown_h.min(side) {
        let sy = y0 + ((y as f64 + 0.5) / scale) as usize;
        for x in 0..shown_w.min(side) {
            let sx = x0 + ((x as f64 + 0.5) / scale) as usize;
            if sx >= x1 || sy >= y1 {
                continue;
            }
            let at = (sy * width as usize + sx) * 4;
            pixels[(top + y) * side + left + x] = egui::Color32::from_rgba_unmultiplied(
                rgba[at],
                rgba[at + 1],
                rgba[at + 2],
                rgba[at + 3],
            );
        }
    }
    egui::ColorImage {
        size: [side, side],
        pixels,
        source_size: egui::vec2(side as f32, side as f32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::options::Resize;
    use tgradish_core::telegram::Target;

    /// 2x1: red, then blue, opaque.
    fn pair() -> Vec<u8> {
        vec![255, 0, 0, 255, 0, 0, 255, 255]
    }

    fn sizes(scaling: Scaling) -> Sizes {
        tgradish_core::convert::sizes(Target::Emoji, Resize::Pad, scaling, (2, 1))
    }

    #[test]
    fn scales_as_the_conversion_does() {
        let whole = Part::of(None, (2, 1), (2, 1));
        // 100 x 100 emoji: the pair is 100 x 50, padded above and below
        let sharp = webm(&pair(), 2, whole, &sizes(Scaling::Sharp));
        assert_eq!(sharp.size, [100, 100]);
        assert_eq!(sharp.pixels[0], egui::Color32::TRANSPARENT);
        let middle = |image: &egui::ColorImage, x: usize| image.pixels[50 * 100 + x];
        // pixel blocks meet without blending
        assert_eq!(middle(&sharp, 49).to_srgba_unmultiplied(), [255, 0, 0, 255]);
        assert_eq!(middle(&sharp, 50).to_srgba_unmultiplied(), [0, 0, 255, 255]);
        // smooth blends near the edge, in linear light: brighter than the
        // average of the two in sRGB would be
        let smooth = webm(&pair(), 2, whole, &sizes(Scaling::Smooth));
        let [r, _, b, _] = middle(&smooth, 50).to_srgba_unmultiplied();
        assert!(r > 140 && b > 140, "{r} {b}");
    }

    #[test]
    fn blends_only_within_the_crop() {
        // red, blue, green: the blue one alone, grown, stays blue
        let row = vec![255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255];
        let part = Part { x: 1.0, y: 0.0, width: 1.0, height: 1.0 };
        let sizes =
            tgradish_core::convert::sizes(Target::Emoji, Resize::Pad, Scaling::Smooth, (1, 1));
        let image = webm(&row, 3, part, &sizes);
        for x in [0, 50, 99] {
            assert_eq!(image.pixels[50 * 100 + x].to_srgba_unmultiplied(), [0, 0, 255, 255]);
        }
    }

    #[test]
    fn fills_the_canvas_with_pixel_art() {
        // 2x1 art, the right pixel transparent: only the red one shows
        let mut art = pair();
        art[7] = 0;
        let part = Part::of(None, (2, 1), (2, 1));
        let frames = [art.clone()];
        let image = tgs(&art, 2, tgs_bounds(&frames, 2, part, false), 64);
        assert_eq!(image.pixels[32 * 64 + 32].to_srgba_unmultiplied(), [255, 0, 0, 255]);
        // with its canvas, the art is twice as wide as high
        let kept = tgs(&art, 2, tgs_bounds(&frames, 2, part, true), 64);
        assert_eq!(kept.pixels[8 * 64 + 2], egui::Color32::TRANSPARENT);
        assert_eq!(kept.pixels[32 * 64 + 2].to_srgba_unmultiplied(), [255, 0, 0, 255]);
        // 3x1 art fills the canvas's width, though 64 isn't a multiple of 3
        let three = [art.clone(), vec![255, 0, 0, 255]].concat();
        let wide = tgs(
            &three,
            3,
            tgs_bounds(&[three.clone()], 3, Part::of(None, (3, 1), (3, 1)), true),
            64,
        );
        assert_eq!(wide.pixels[32 * 64].to_srgba_unmultiplied(), [255, 0, 0, 255]);
        assert_eq!(wide.pixels[32 * 64 + 63].to_srgba_unmultiplied(), [255, 0, 0, 255]);
    }
}

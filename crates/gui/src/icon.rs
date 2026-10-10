//! tgradish's icon. It is pixel art, so it is only ever drawn with whole
//! screen pixels for each of its own, which keeps them square and sharp.

use eframe::egui::{self, Color32, Rect, pos2, vec2};

/// The icon, a square 8-bit RGBA PNG.
const PNG: &[u8] = include_bytes!("../assets/icon.png");

/// The icon's side and straight RGBA pixels.
fn pixels() -> (usize, Vec<u8>) {
    let mut reader =
        png::Decoder::new(std::io::Cursor::new(PNG)).read_info().expect("the icon is a PNG");
    let mut pixels = vec![0; reader.output_buffer_size().expect("the icon is small")];
    let info = reader.next_frame(&mut pixels).expect("the icon is a PNG");
    pixels.truncate(info.buffer_size());
    (info.width as usize, pixels)
}

/// The icon for the taskbar and the like, each of its pixels a square of
/// `SCALE`: systems shrink it to the size they want instead of blurring it
/// bigger.
pub fn window() -> egui::IconData {
    const SCALE: usize = 4;
    let (side, pixels) = pixels();
    let rgba = pixels
        .chunks_exact(side * 4)
        .flat_map(|row| {
            let row: Vec<u8> =
                row.as_chunks::<4>().0.iter().flat_map(|pixel| pixel.repeat(SCALE)).collect();
            std::iter::repeat_n(row, SCALE).flatten()
        })
        .collect();
    let side = (side * SCALE) as u32;
    egui::IconData { rgba, width: side, height: side }
}

/// The icon as a texture, made once.
fn texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("tgradish icon");
    if let Some(texture) = ctx.data(|data| data.get_temp(id)) {
        return texture;
    }
    let (side, pixels) = pixels();
    let image = egui::ColorImage::from_rgba_unmultiplied([side, side], &pixels);
    let texture = ctx.load_texture("icon", image, egui::TextureOptions::NEAREST);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

/// Side in points of the icon with as many screen pixels for each of its
/// own as fit in `room` points, and at least one.
pub fn side(ctx: &egui::Context, room: f32) -> f32 {
    fit(texture(ctx).size()[0] as f32, ctx.pixels_per_point(), room)
}

/// Side in points of `pixels` of pixel art at `ppp` screen pixels per point,
/// as [`side`] chooses it.
fn fit(pixels: f32, ppp: f32, room: f32) -> f32 {
    (room * ppp / pixels).floor().max(1.0) * pixels / ppp
}

/// Draws the icon at [`side`] for `room`.
pub fn show(ui: &mut egui::Ui, room: f32) -> egui::Response {
    let shown = side(ui.ctx(), room);
    let (rect, response) = ui.allocate_exact_size(vec2(shown, shown), egui::Sense::hover());
    // from a screen pixel's corner, so the icon's pixels cover whole ones
    let ppp = ui.ctx().pixels_per_point();
    let rect = Rect::from_min_size((rect.min * ppp).round() / ppp, rect.size());
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    ui.painter().image(texture(ui.ctx()).id(), rect, uv, Color32::WHITE);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_by_whole_pixels() {
        let (width, pixels) = pixels();
        assert_eq!((width, pixels.len()), (48, 48 * 48 * 4));
        let icon = window();
        assert_eq!((icon.width, icon.rgba.len()), (192, 192 * 192 * 4));
        // a pixel of the radish, a square of 4 by 4
        let pixel = &pixels[(30 * width + 29) * 4..][..4];
        assert_eq!(pixel[3], 255);
        for row in 120..124 {
            for column in 116..120 {
                assert_eq!(&icon.rgba[(row * 192 + column) * 4..][..4], pixel);
            }
        }
        // never smaller than its own pixels
        for (ppp, room, shown) in
            [(1.0, 48.0, 48.0), (1.25, 48.0, 38.4), (2.0, 48.0, 48.0), (1.0, 20.0, 48.0)]
        {
            assert!((fit(48.0, ppp, room) - shown).abs() < 1e-3, "{ppp} {room}");
        }
    }
}

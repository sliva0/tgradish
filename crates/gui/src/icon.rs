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

/// The icon's side and straight RGBA pixels, each of its pixels a square
/// of `scale`.
pub(crate) fn scaled(scale: usize) -> (usize, Vec<u8>) {
    let (side, pixels) = pixels();
    let rgba = pixels
        .chunks_exact(side * 4)
        .flat_map(|row| {
            let row: Vec<u8> =
                row.as_chunks::<4>().0.iter().flat_map(|pixel| pixel.repeat(scale)).collect();
            std::iter::repeat_n(row, scale).flatten()
        })
        .collect();
    (side * scale, rgba)
}

/// The icon for the taskbar and the like, four times as large: systems
/// shrink it to the size they want instead of blurring it bigger.
pub(crate) fn window() -> egui::IconData {
    let (side, rgba) = scaled(4);
    egui::IconData { rgba, width: side as u32, height: side as u32 }
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
pub(crate) fn side(ctx: &egui::Context, room: f32) -> f32 {
    fit(texture(ctx).size()[0] as f32, ctx.pixels_per_point(), room)
}

/// [`side`] for `room`, where the icon fits in it with at least a screen
/// pixel for each of its own.
pub(crate) fn fitting(ctx: &egui::Context, room: f32) -> Option<f32> {
    within(texture(ctx).size()[0] as f32, ctx.pixels_per_point(), room)
}

/// Side in points of `pixels` of pixel art at `ppp` screen pixels per point,
/// as [`side`] chooses it.
fn fit(pixels: f32, ppp: f32, room: f32) -> f32 {
    (room * ppp / pixels).floor().max(1.0) * pixels / ppp
}

/// [`fit`], where a screen pixel for each of the art's fits in `room`.
fn within(pixels: f32, ppp: f32, room: f32) -> Option<f32> {
    // a hair over, which a fractional scale's points may come to
    (room * ppp + 0.01 >= pixels).then(|| fit(pixels, ppp, room))
}

/// Draws the icon at [`side`] for `room`.
pub(crate) fn show(ui: &mut egui::Ui, room: f32) -> egui::Response {
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
        // where it fits: smaller by whole screen pixels, down to one for
        // each of its own, and not at all in less room
        for (ppp, room, shown) in [
            (1.0, 48.0, Some(48.0)),
            (1.0, 47.0, None),
            (1.25, 40.0, Some(38.4)),
            (1.25, 38.4, Some(38.4)),
            (1.25, 38.0, None),
            (2.0, 47.0, Some(24.0)),
            (2.0, 23.0, None),
            (3.0, 48.0, Some(48.0)),
            (3.0, 40.0, Some(32.0)),
        ] {
            let fitted = within(48.0, ppp, room);
            assert_eq!(fitted.map(|side| (side * 100.0).round() / 100.0), shown, "{ppp} {room}");
        }
    }

    /// The GUI's icon over the list, where it was drawn last: its texture's
    /// mesh, as [`show`] paints it.
    fn drawn(harness: &egui_kittest::Harness<'_, crate::app::App>) -> Option<Rect> {
        let id = texture(&harness.ctx).id();
        harness.output().shapes.iter().find_map(|clipped| match &clipped.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == id => Some(mesh.calc_bounds()),
            _ => None,
        })
    }

    #[test]
    fn shows_the_icon_over_the_list_at_the_smallest_size() {
        use egui_kittest::kittest::{NodeT, Queryable};
        // at whole scales and fractional ones
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let mut harness = egui_kittest::Harness::builder()
                .with_size([crate::MIN_WIDTH, crate::MIN_HEIGHT])
                .with_pixels_per_point(ppp)
                .build_eframe(|cc| {
                    let app = crate::app::App::with(
                        crate::config::Config::default(),
                        None,
                        tgradish_core::presets::Presets::builtin(),
                    );
                    app.style(&cc.egui_ctx);
                    app
                });
            harness.run_steps(3);
            let icon = drawn(&harness).unwrap_or_else(|| panic!("no icon at {ppp}"));
            // whole, at a screen pixel or more for each of its own, and
            // clear of the buttons beside it
            assert!(icon.width() * ppp >= 48.0 - 1e-3, "{ppp}: {icon:?}");
            let add = crate::l10n::fl!("add-files");
            let buttons = harness.query_all(egui_kittest::kittest::by().label(&add));
            let first = buttons
                .filter_map(|node| node.accesskit_node().bounding_box())
                .min_by(|a, b| a.y0.total_cmp(&b.y0))
                .expect("Add files over the list");
            assert!(f64::from(icon.max.x) <= first.x0, "{ppp}: {icon:?} {first:?}");
        }
    }
}

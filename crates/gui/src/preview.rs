//! Plays a sticker's frames, over a checkerboard so transparency shows.

use eframe::egui;
use tgradish_core::tgs::Preview;

/// Frames turned into textures, made once per preview.
pub struct Player {
    frames: Vec<(egui::TextureHandle, u32)>,
    size: egui::Vec2,
    ticks: u32,
}

impl Player {
    pub fn new(ctx: &egui::Context, name: &str, preview: &Preview) -> Player {
        let frames = preview
            .frames
            .iter()
            .enumerate()
            .map(|(index, (rgba, ticks))| {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [preview.width as usize, preview.height as usize],
                    rgba,
                );
                let texture = ctx.load_texture(
                    format!("{name}-{index}"),
                    image,
                    egui::TextureOptions::NEAREST,
                );
                (texture, *ticks)
            })
            .collect();
        Player {
            frames,
            size: egui::vec2(preview.width as f32, preview.height as f32),
            ticks: preview.frames.iter().map(|(_, ticks)| ticks).sum::<u32>().max(1),
        }
    }

    /// Shows the frame due now, fitted into `side` points, and asks for a
    /// repaint when the next one is due.
    pub fn show(&self, ui: &mut egui::Ui, side: f32) {
        let time = ui.input(|input| input.time);
        let tick = ((time * 60.0) as u64 % u64::from(self.ticks)) as u32;
        let mut start = 0;
        let mut shown = &self.frames[0];
        for frame in &self.frames {
            if tick < start + frame.1 {
                shown = frame;
                break;
            }
            start += frame.1;
        }
        let scale = side / self.size.x.max(self.size.y);
        let size = self.size * scale;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        checkerboard(&painter, rect);
        let image = egui::Rect::from_center_size(rect.center(), size);
        painter.image(
            shown.0.id(),
            image,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        if self.frames.len() > 1 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

fn checkerboard(painter: &egui::Painter, rect: egui::Rect) {
    let square = 12.0;
    let (light, dark) = (egui::Color32::from_gray(200), egui::Color32::from_gray(160));
    painter.rect_filled(rect, 0.0, light);
    let columns = (rect.width() / square).ceil() as i32;
    let rows = (rect.height() / square).ceil() as i32;
    for row in 0..rows {
        for column in (row % 2..columns).step_by(2) {
            let min = rect.min + egui::vec2(column as f32 * square, row as f32 * square);
            let cell = egui::Rect::from_min_size(min, egui::vec2(square, square)).intersect(rect);
            painter.rect_filled(cell, 0.0, dark);
        }
    }
}

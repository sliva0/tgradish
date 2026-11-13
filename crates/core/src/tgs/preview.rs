//! What a `.tgs` shows, drawn from the art it was made of, for previews.

use crate::preview::Preview;

/// The longest side of a `.tgs` preview: plenty for a window, and within
/// what any GPU takes as a texture.
pub(crate) const PREVIEW_MAX_SIDE: u32 = 1024;

/// What `anim` shows, `stretch`ed square as the sticker is, or not.
pub(super) fn draw(anim: &tgradish_tgs::PixelAnim, stretch: bool) -> Preview {
    let grid = anim.grid();
    let width = *grid.columns().last().unwrap_or(&0);
    let height = *grid.rows().last().unwrap_or(&0);
    // input pixels per preview pixel: an art pixel, unless the canvas
    // cuts those at the sides, which then show only in part
    let unit = if grid.cut_at_sides() { 1 } else { grid.scale() };
    let step = unit.max(width.max(height).div_ceil(PREVIEW_MAX_SIDE)).max(1);
    let most = width.max(height).div_ceil(step);
    // the cell under the middle of each step, or stretched, of each of
    // as many preview pixels as along the longer side
    let cells = |edges: &[u32], size: u32| -> Vec<usize> {
        let middles: Vec<u32> = if stretch {
            (0..most)
                .map(|i| ((2 * i + 1) as u64 * size as u64 / (2 * most) as u64) as u32)
                .collect()
        } else {
            (0..size.div_ceil(step)).map(|i| i * step + step / 2).collect()
        };
        middles
            .into_iter()
            .map(|x| edges.partition_point(|&edge| edge <= x.min(size - 1)) - 1)
            .collect()
    };
    let (columns, rows) = (cells(grid.columns(), width), cells(grid.rows(), height));
    let frames = anim
        .frames()
        .iter()
        .map(|frame| {
            let mut rgba = Vec::with_capacity(columns.len() * rows.len() * 4);
            for &row in &rows {
                for &column in &columns {
                    let colour = frame.pixels[row * anim.width() as usize + column];
                    rgba.extend(anim.palette()[colour as usize]);
                }
            }
            (rgba, frame.ticks)
        })
        .collect();
    Preview { width: columns.len() as u32, height: rows.len() as u32, frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use tgradish_tgs::frames;

    #[test]
    fn previews_a_pixel_per_art_pixel() {
        let anim = |width: u32, height: u32, colour: &dyn Fn(u32, u32) -> [u8; 4]| {
            let rgba =
                (0..height).flat_map(|y| (0..width).flat_map(move |x| colour(x, y))).collect();
            let frame = frames::Frame { rgba, duration: Duration::from_millis(100) };
            let animation = frames::Animation::new(width, height, vec![frame]).unwrap();
            tgradish_tgs::normalise(&animation, &Default::default()).unwrap().0
        };
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        // 3x2 art at 4x
        let art = anim(12, 8, &|x, y| if (x / 4 + y / 4) % 2 == 0 { red } else { blue });
        let preview = draw(&art, false);
        assert_eq!((preview.width, preview.height), (3, 2));
        assert_eq!(preview.frames[0].0, [red, blue, red, blue, red, blue].concat());
        // stretched square: its rows twice as high
        let preview = draw(&art, true);
        assert_eq!((preview.width, preview.height), (3, 3));
        let rows = [[red, blue, red], [blue, red, blue], [blue, red, blue]];
        assert_eq!(preview.frames[0].0, rows.concat().concat());
        // too wide to show a pixel per art pixel
        let wide = anim(4000, 2, &|x, _| if x % 2 == 0 { red } else { blue });
        let preview = draw(&wide, false);
        assert_eq!((preview.width, preview.height), (1000, 1));
        assert_eq!(preview.frames[0].0.len(), 1000 * 4);
    }
}

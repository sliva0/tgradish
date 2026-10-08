//! Encoders: [`PixelAnim`] in, [`Scene`] out.

use crate::normalise::PixelAnim;
use crate::scene::{Group, Layer, Scene, Shape};

/// The simplest correct encoding, a baseline for tests and benchmarks:
/// every frame is its own layer, with one group per colour made of the
/// colour's horizontal runs. It breaks the seam invariant wherever two
/// colours meet.
pub fn runs(anim: &PixelAnim) -> Scene {
    let width = anim.width();
    let mut layers = Vec::new();
    let mut start = 0;
    for frame in anim.frames() {
        let mut groups: Vec<Group> = (0..anim.palette().len() as u16)
            .map(|colour| Group { colour, ..Group::default() })
            .collect();
        for (y, row) in frame.pixels.chunks_exact(width as usize).enumerate() {
            let mut x = 0;
            while x < row.len() {
                let colour = row[x];
                let run = row[x..].iter().take_while(|&&c| c == colour).count();
                if colour != 0 {
                    groups[colour as usize].shapes.push(Shape::Rect {
                        x: x as u32,
                        y: y as u32,
                        width: run as u32,
                        height: 1,
                    });
                }
                x += run;
            }
        }
        groups.retain(|group| !group.shapes.is_empty());
        let end = start + frame.ticks;
        layers.push(Layer { from: start, to: end, groups });
        start = end;
    }
    Scene { width, height: anim.height(), ticks: start, layers }
}

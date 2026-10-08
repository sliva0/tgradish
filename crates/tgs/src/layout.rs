//! Places a [`Scene`] on the 512x512 canvas and turns it into Lottie: the
//! art is scaled to fill the canvas along its longer side and centred.
//!
//! Lottie coordinates are in art pixels (the grid's scale), measured from
//! half an art pixel before where the art pixel grid starts. Rectangles
//! are written by their centre, and most are an odd number of art pixels
//! wide or high (often one), so their centres come out whole: shorter
//! numbers. Inner edges are at halves even when the canvas cut art pixels.

use crate::limits::telegram::CANVAS;
use crate::lottie::{Animation, Item, Layer, Transform};
use crate::normalise::{Grid, PixelAnim};
use crate::scene::{Scene, Shape};

/// How far Lottie coordinates start before the art pixel grid, in art
/// pixels.
const SHIFT: f64 = 0.5;

/// Where the cells of a [`PixelAnim`] land.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    columns: Vec<f64>,
    rows: Vec<f64>,
    /// Layer coordinates to canvas pixels.
    pub transform: Transform,
}

impl Placement {
    pub fn new(grid: &Grid) -> Placement {
        let width = f64::from(*grid.columns.last().unwrap());
        let height = f64::from(*grid.rows.last().unwrap());
        let unit = grid.scale();
        let origin_of = |edges: &[u32]| if edges.len() > 2 { edges[1] % unit } else { 0 };
        // where the art pixel grid starts: the first inner edge, modulo the scale
        let (origin_x, origin_y) = (origin_of(&grid.columns), origin_of(&grid.rows));
        let unit = f64::from(unit);
        let coordinates = |edges: &[u32], origin: u32| {
            edges.iter().map(|&edge| (f64::from(edge) - f64::from(origin)) / unit - SHIFT).collect()
        };
        // canvas pixels per input pixel
        let fit = f64::from(CANVAS) / width.max(height);
        let canvas = f64::from(CANVAS);
        Placement {
            columns: coordinates(&grid.columns, origin_x),
            rows: coordinates(&grid.rows, origin_y),
            transform: Transform {
                position: [
                    (canvas - width * fit) / 2.0 + (f64::from(origin_x) + SHIFT * unit) * fit,
                    (canvas - height * fit) / 2.0 + (f64::from(origin_y) + SHIFT * unit) * fit,
                ],
                scale: fit * unit,
            },
        }
    }

    /// Layer coordinates of a cell corner.
    pub fn point(&self, column: u32, row: u32) -> [f64; 2] {
        [self.columns[column as usize], self.rows[row as usize]]
    }

    /// Canvas pixels of a cell corner.
    pub fn canvas(&self, column: u32, row: u32) -> [f64; 2] {
        let [x, y] = self.point(column, row);
        let Transform { position, scale } = self.transform;
        [position[0] + x * scale, position[1] + y * scale]
    }
}

/// The Lottie for a scene of `anim`'s cells.
pub fn lay_out(scene: &Scene, anim: &PixelAnim, name: Option<String>) -> Animation {
    let placement = Placement::new(anim.grid());
    let layers = scene
        .layers
        .iter()
        .rev()
        .map(|layer| Layer {
            from: layer.from,
            to: layer.to,
            transform: placement.transform,
            items: layer
                .groups
                .iter()
                .rev()
                .map(|group| {
                    let mut items: Vec<Item> = group
                        .shapes
                        .iter()
                        .map(|shape| match shape {
                            &Shape::Rect { x, y, width, height } => {
                                let [x0, y0] = placement.point(x, y);
                                let [x1, y1] = placement.point(x + width, y + height);
                                Item::Rect {
                                    centre: [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
                                    size: [x1 - x0, y1 - y0],
                                }
                            }
                            Shape::Path(corners) => Item::Path(
                                corners.iter().map(|&(x, y)| placement.point(x, y)).collect(),
                            ),
                        })
                        .collect();
                    items.push(Item::Fill {
                        colour: anim.palette()[group.colour as usize],
                        rule: group.rule,
                    });
                    items.push(Item::GroupTransform);
                    Item::Group(items)
                })
                .collect(),
        })
        .collect();
    Animation { name, ticks: scene.ticks, layers }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_and_scales() {
        // 2x art, 40x20 input pixels: fills the width, centred vertically
        let grid =
            Grid { columns: (0..=40).step_by(2).collect(), rows: (0..=20).step_by(2).collect() };
        let placement = Placement::new(&grid);
        assert_eq!(placement.transform, Transform { position: [12.8, 140.8], scale: 25.6 });
        assert_eq!(placement.point(20, 10), [19.5, 9.5]);
        assert_eq!(placement.canvas(20, 10), [512.0, 384.0]);
    }

    #[test]
    fn keeps_inner_edges_on_halves_when_art_pixels_are_cut() {
        // 3x art cut by 2 pixels on the left
        let grid = Grid { columns: vec![0, 1, 4, 7], rows: vec![0, 3, 6] };
        let placement = Placement::new(&grid);
        assert_eq!(placement.point(1, 0), [-0.5, -0.5]);
        assert_eq!(placement.point(3, 2), [1.5, 1.5]);
        assert!((placement.point(0, 0)[0] - (-1.0 / 3.0 - 0.5)).abs() < 1e-12);
        let [left, _] = placement.canvas(0, 0);
        let [right, _] = placement.canvas(3, 0);
        assert!(left.abs() < 1e-9 && (right - 512.0).abs() < 1e-9);
    }
}

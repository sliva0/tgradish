//! Places a [`Scene`] on the 512×512 canvas and builds a Lottie [`Animation`].
//! Non-square art is scaled to fit its longer side and centred, or stretched
//! to fill the canvas.
//!
//! Lottie coordinates are measured in art pixels (the grid scale), shifted by
//! half an art pixel before the grid origin. Rectangles are written by centre
//! and most have odd width or height (often 1), so their centres become whole
//! numbers and take fewer JSON bytes. Interior edges stay on half-integers
//! even when outer art pixels were clipped.

use crate::limits::telegram::CANVAS;
use crate::lottie::{Animation, Item, Layer, Transform};
use crate::normalise::{Grid, PixelAnim};
use crate::scene::{Group, Layer as SceneLayer, Scene, Shape};

/// Coordinate shift before the pixel grid origin, in art pixels.
const SHIFT: f64 = 0.5;

/// Cell-to-canvas placement for a [`PixelAnim`].
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    columns: Vec<f64>,
    rows: Vec<f64>,
    /// Transform from layer coordinates to canvas pixels.
    pub transform: Transform,
}

impl Placement {
    /// Computes placement for `grid`. Non-square art fits along its longer
    /// side unless `stretch` is true, in which case it fills the whole canvas.
    pub fn new(grid: &Grid, stretch: bool) -> Placement {
        let width = f64::from(*grid.columns.last().unwrap());
        let height = f64::from(*grid.rows.last().unwrap());
        let (origin_x, origin_y) = grid.origin();
        let unit = f64::from(grid.scale());
        let coordinates = |edges: &[u32], origin: u32| {
            edges.iter().map(|&edge| (f64::from(edge) - f64::from(origin)) / unit - SHIFT).collect()
        };
        // Canvas pixels per input pixel along x and y.
        let canvas = f64::from(CANVAS);
        let (across, down) = match stretch {
            true => (canvas / width, canvas / height),
            false => (canvas / width.max(height), canvas / width.max(height)),
        };
        Placement {
            columns: coordinates(&grid.columns, origin_x),
            rows: coordinates(&grid.rows, origin_y),
            transform: Transform {
                position: [
                    (canvas - width * across) / 2.0 + (f64::from(origin_x) + SHIFT * unit) * across,
                    (canvas - height * down) / 2.0 + (f64::from(origin_y) + SHIFT * unit) * down,
                ],
                scale: [across * unit, down * unit],
            },
        }
    }

    /// Returns the layer coordinates of a cell corner.
    pub fn point(&self, column: u32, row: u32) -> [f64; 2] {
        [self.columns[column as usize], self.rows[row as usize]]
    }

    /// Returns the canvas pixel coordinates of a cell corner.
    #[cfg(any(test, feature = "internals"))]
    pub fn canvas(&self, column: u32, row: u32) -> [f64; 2] {
        let [x, y] = self.point(column, row);
        let Transform { position, scale } = self.transform;
        [position[0] + x * scale[0], position[1] + y * scale[1]]
    }
}

/// Converts `scene` into a Lottie [`Animation`] using `anim`'s grid and
/// palette, optionally stretching it to fill the square canvas.
pub fn lay_out(scene: &Scene, anim: &PixelAnim, name: Option<String>, stretch: bool) -> Animation {
    let placement = Placement::new(anim.grid(), stretch);
    let layers = scene
        .layers
        .iter()
        .rev()
        .map(|layer| Layer {
            id: None,
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
                    items.push(Item::GroupTransform { opacity: opacity(layer, group) });
                    Item::Group(items)
                })
                .collect(),
            hidden: layer.hidden.clone(),
        })
        .collect();
    Animation { name, ticks: scene.ticks, layers }
}

/// Builds hold keyframes on a group's opacity so it shows only during its
/// visible spans within `layer`.
fn opacity(layer: &SceneLayer, group: &Group) -> Vec<(u32, u8)> {
    let Some(&(first, _)) = group.shown.first() else { return Vec::new() };
    let mut keys = Vec::new();
    if first > layer.from {
        keys.push((layer.from, 0));
    }
    for &(start, end) in &group.shown {
        keys.push((start, 100));
        if end < layer.to {
            keys.push((end, 0));
        }
    }
    keys
}

/// Computes Telegram's server complexity ([`crate::limits::telegram::cost`])
/// for what [`lay_out`] generates from `scene`. Each group adds 3 shape items
/// (itself, its fill, and its transform).
pub fn cost(scene: &Scene) -> usize {
    let groups = scene.layers.iter().flat_map(|layer| &layer.groups);
    let shapes = groups.map(|group| group.shapes.len() + 3).sum();
    crate::limits::telegram::cost(shapes, scene.layers.len())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::frames::{Animation as Frames, Frame};

    use super::*;
    use crate::check::check;
    use crate::lottie::Style;
    use crate::normalise::{Options, normalise};
    use crate::scene::{FillRule, Group, Layer as SceneLayer};

    #[test]
    fn costs_what_check_counts() {
        let rects = |count: u32| (0..count).map(|x| Shape::Rect { x, y: 0, width: 1, height: 1 });
        let group = |colour, count| Group {
            colour,
            rule: FillRule::NonZero,
            shapes: rects(count).collect(),
            shown: Vec::new(),
        };
        let scene = Scene {
            width: 8,
            height: 1,
            ticks: 2,
            layers: vec![
                SceneLayer {
                    from: 0,
                    to: 2,
                    groups: vec![group(0, 3), group(1, 2)],
                    hidden: Vec::new(),
                },
                SceneLayer { from: 1, to: 2, groups: vec![group(0, 8)], hidden: Vec::new() },
            ],
        };
        // red and blue in turn, so both are in the palette
        let rgba =
            (0..8).flat_map(|x| if x % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] });
        let frame = Frame { rgba: rgba.collect(), duration: Duration::from_millis(100) };
        let input = Frames::new(8, 1, vec![frame]).unwrap();
        let options = Options { keep_canvas: true, ..Options::default() };
        let (anim, _) = normalise(input.clone(), &options).unwrap();
        let json = lay_out(&scene, &anim, None, false).to_json(Style::default());
        let (stats, _) = check(json.as_bytes(), None).unwrap();
        assert_eq!(cost(&scene), crate::limits::telegram::cost(stats.shapes, stats.layers));
    }

    #[test]
    fn centres_and_scales() {
        // 2x art, 40x20 input pixels: fills the width, centred vertically
        let grid =
            Grid { columns: (0..=40).step_by(2).collect(), rows: (0..=20).step_by(2).collect() };
        let placement = Placement::new(&grid, false);
        let transform = Transform { position: [12.8, 140.8], scale: [25.6, 25.6] };
        assert_eq!(placement.transform, transform);
        assert_eq!(placement.point(20, 10), [19.5, 9.5]);
        assert_eq!(placement.canvas(20, 10), [512.0, 384.0]);
        // stretched to fill the canvas, twice as much down
        let placement = Placement::new(&grid, true);
        let transform = Transform { position: [12.8, 25.6], scale: [25.6, 51.2] };
        assert_eq!(placement.transform, transform);
        assert_eq!(placement.canvas(0, 0), [0.0, 0.0]);
        assert_eq!(placement.canvas(20, 10), [512.0, 512.0]);
    }

    #[test]
    fn keeps_inner_edges_on_halves_when_art_pixels_are_cut() {
        // 3x art cut by 2 pixels on the left
        let grid = Grid { columns: vec![0, 1, 4, 7], rows: vec![0, 3, 6] };
        let placement = Placement::new(&grid, false);
        assert_eq!(placement.point(1, 0), [-0.5, -0.5]);
        assert_eq!(placement.point(3, 2), [1.5, 1.5]);
        assert!((placement.point(0, 0)[0] - (-1.0 / 3.0 - 0.5)).abs() < 1e-12);
        let [left, _] = placement.canvas(0, 0);
        let [right, _] = placement.canvas(3, 0);
        assert!(left.abs() < 1e-9 && (right - 512.0).abs() < 1e-9);
    }
}

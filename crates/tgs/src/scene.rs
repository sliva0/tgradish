//! Intermediate scene representation produced by encoders: paint groups (one
//! fill over rectangles or rectilinear paths) stacked bottom to top inside
//! timed layers. Coordinates are cell indices on the [`PixelAnim`] grid.
//!
//! Two invariants can be verified directly on a scene without a renderer:
//! - rasterising the scene reproduces the normalised frames exactly;
//! - the seam invariant: wherever two 8-adjacent opaque cells display
//!   different paint groups, a group of the lower cell's colour drawn under
//!   the upper group (usually the lower group itself) covers both cells so
//!   anti-aliased inner edges blend over the right colour rather than
//!   transparency.

#[cfg(any(test, feature = "internals"))]
use crate::normalise::PixelAnim;

/// An animation represented as layered paint groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scene {
    /// Width in cells.
    pub width: u32,
    /// Height in cells.
    pub height: u32,
    /// Duration in 60 fps ticks.
    pub ticks: u32,
    /// Layers ordered bottom to top.
    pub layers: Vec<Layer>,
}

/// Paint groups sharing a visibility lifetime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layer {
    /// Start tick (inclusive).
    pub from: u32,
    /// End tick (exclusive).
    pub to: u32,
    /// Sorted tick spans within `from..to` during which the layer is hidden.
    pub hidden: Vec<(u32, u32)>,
    /// Paint groups ordered bottom to top.
    pub groups: Vec<Group>,
}

/// Fill rule for a paint group. The painter uses [`FillRule::NonZero`] only;
/// [`FillRule::EvenOdd`] is used by `tgs-lab` to test renderers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FillRule {
    /// Non-zero winding rule.
    #[default]
    NonZero,
    /// Even-odd winding rule.
    EvenOdd,
}

/// Shapes filled together with one palette colour.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    /// Palette index.
    pub colour: u16,
    /// Fill rule.
    pub rule: FillRule,
    /// Shapes in this group.
    pub shapes: Vec<Shape>,
    /// Sorted tick spans where the group is shown while its layer is visible;
    /// empty means shown for the layer's full lifetime.
    pub shown: Vec<(u32, u32)>,
}

/// Geometry inside a [`Group`]. The painter emits rectangles only; paths are
/// used by `tgs-lab` probes and renderer checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// Axis-aligned rectangle of cells.
    Rect {
        /// Left column.
        x: u32,
        /// Top row.
        y: u32,
        /// Width in cells.
        width: u32,
        /// Height in cells.
        height: u32,
    },
    // Unused without `internals` because the painter emits only rectangles.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    /// Closed rectilinear polygon of alternating horizontal and vertical edges.
    /// Clockwise (screen coordinates, y down) adds area under non-zero fill;
    /// counter-clockwise cuts holes.
    Path(Vec<(u32, u32)>),
}

#[cfg(any(test, feature = "internals"))]
/// Rasterised colour state of one cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Transparent (nothing painted).
    Clear,
    /// Single palette colour (opaque, or translucent over transparency).
    Colour(u16),
    /// Translucent colour painted over another colour. Renderers round blends
    /// differently, so [`Scene::compare`] treats every blend as a mismatch;
    /// encoders never draw translucent paint over existing paint.
    Blend,
}

#[cfg(any(test, feature = "internals"))]
/// First difference found between a [`Scene`] and a [`PixelAnim`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// Scene dimensions or tick count differ (`(width, height, ticks)`).
    Size {
        /// Expected `(width, height, ticks)` from the animation.
        expected: (u32, u32, u32),
        /// Actual `(width, height, ticks)` of the scene.
        got: (u32, u32, u32),
    },
    /// First cell that renders differently.
    Cell {
        /// Tick (60 fps frame index).
        tick: u32,
        /// Cell column.
        x: u32,
        /// Cell row.
        y: u32,
        /// Expected palette index.
        expected: u16,
        /// Actual rasterised cell state.
        got: Paint,
    },
}

#[cfg(any(test, feature = "internals"))]
/// A pair of adjacent opaque cells violating the seam invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seam {
    /// First tick where the seam occurs.
    pub tick: u32,
    /// The cell showing the upper group, where the lower group's colour must
    /// also be painted.
    pub cell: (u32, u32),
    /// Adjacent cell displaying the lower paint group.
    pub neighbour: (u32, u32),
}

#[cfg(any(test, feature = "internals"))]
impl Shape {
    /// The box around the shape: `x0, y0, x1, y1`, ends excluded.
    fn bounds(&self) -> (u32, u32, u32, u32) {
        match *self {
            Shape::Rect { x, y, width, height } => {
                (x, y, x.saturating_add(width), y.saturating_add(height))
            }
            Shape::Path(ref corners) => {
                corners.iter().fold((u32::MAX, u32::MAX, 0, 0), |(x0, y0, x1, y1), &(x, y)| {
                    (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                })
            }
        }
    }

    /// Adds the shape's vertical edges crossing row `y` to `winding`, which
    /// holds the change in winding number at each column edge from `left`.
    /// Edges past its end are left out: no cell before them counts them.
    fn add_crossings(&self, y: u32, left: u32, winding: &mut [i32]) {
        let mut add = |x: u32, change: i32| {
            if let Some(edge) = winding.get_mut((x - left) as usize) {
                *edge += change;
            }
        };
        match *self {
            Shape::Rect { x, y: top, width, height } => {
                if (top..top.saturating_add(height)).contains(&y) {
                    add(x, 1);
                    add(x.saturating_add(width), -1);
                }
            }
            Shape::Path(ref corners) => {
                for (index, &(x, y0)) in corners.iter().enumerate() {
                    let (x1, y1) = corners[(index + 1) % corners.len()];
                    if x1 != x || y0 == y1 {
                        continue;
                    }
                    // a clockwise outline goes up on its left side
                    if y1 < y0 && (y1..y0).contains(&y) {
                        add(x, 1);
                    } else if y0 < y1 && (y0..y1).contains(&y) {
                        add(x, -1);
                    }
                }
            }
        }
    }
}

#[cfg(any(test, feature = "internals"))]
/// The cells a group fills, kept for the box around its shapes only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    left: u32,
    top: u32,
    width: u32,
    cells: Vec<bool>,
}

#[cfg(any(test, feature = "internals"))]
impl Coverage {
    pub fn contains(&self, x: u32, y: u32) -> bool {
        let (Some(dx), Some(dy)) = (x.checked_sub(self.left), y.checked_sub(self.top)) else {
            return false;
        };
        dx < self.width && self.cells.get((dy * self.width + dx) as usize).copied().unwrap_or(false)
    }

    /// Filled cells, row by row.
    pub fn cells(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let width = self.width.max(1);
        self.cells.iter().enumerate().filter(|(_, filled)| **filled).map(move |(index, _)| {
            (self.left + index as u32 % width, self.top + index as u32 / width)
        })
    }
}

#[cfg(any(test, feature = "internals"))]
impl Group {
    /// Cells the group fills of a scene `width` by `height`. What is past
    /// its edges is cut off, so a shape reaching far past them costs no more
    /// than one that ends at them.
    pub(crate) fn coverage(&self, width: u32, height: u32) -> Coverage {
        let (left, top, right, bottom) = self
            .shapes
            .iter()
            .map(Shape::bounds)
            .fold((u32::MAX, u32::MAX, 0, 0), |(x0, y0, x1, y1), (a, b, c, d)| {
                (x0.min(a), y0.min(b), x1.max(c), y1.max(d))
            });
        let (right, bottom) = (right.min(width), bottom.min(height));
        if left >= right || top >= bottom {
            return Coverage { left: 0, top: 0, width: 0, cells: Vec::new() };
        }
        let width = right - left;
        let mut cells = Vec::with_capacity(width as usize * (bottom - top) as usize);
        let mut winding = vec![0i32; width as usize + 1];
        for y in top..bottom {
            winding.fill(0);
            for shape in &self.shapes {
                shape.add_crossings(y, left, &mut winding);
            }
            let mut count = 0;
            for &change in &winding[..width as usize] {
                count += change;
                cells.push(match self.rule {
                    FillRule::NonZero => count != 0,
                    FillRule::EvenOdd => count % 2 != 0,
                });
            }
        }
        Coverage { left, top, width, cells }
    }
}

#[cfg(any(test, feature = "internals"))]
/// Whether `colour` of `palette` is opaque, or past its end.
fn opaque(palette: &[[u8; 4]], colour: u16) -> bool {
    palette.get(usize::from(colour)).is_none_or(|rgba| rgba[3] == 255)
}

#[cfg(any(test, feature = "internals"))]
impl Layer {
    /// Returns whether this layer is visible at `tick`.
    pub fn shown(&self, tick: u32) -> bool {
        (self.from..self.to).contains(&tick)
            && !self.hidden.iter().any(|&(start, end)| (start..end).contains(&tick))
    }
}

#[cfg(any(test, feature = "internals"))]
impl Scene {
    /// Sorted ticks where layer or group visibility changes, spanning
    /// `0..=ticks`.
    fn changes(&self) -> Vec<u32> {
        let mut ticks = vec![0, self.ticks];
        for layer in &self.layers {
            ticks.extend([layer.from.min(self.ticks), layer.to.min(self.ticks)]);
            let groups = layer.groups.iter().flat_map(|group| &group.shown);
            for &(start, end) in layer.hidden.iter().chain(groups) {
                ticks.extend([start.min(self.ticks), end.min(self.ticks)]);
            }
        }
        ticks.sort_unstable();
        ticks.dedup();
        ticks
    }

    /// Visible groups at `tick`, ordered bottom to top.
    fn visible(&self, tick: u32) -> impl Iterator<Item = &Group> {
        self.layers
            .iter()
            .filter(move |layer| layer.shown(tick))
            .flat_map(|layer| &layer.groups)
            .filter(move |group| {
                group.shown.is_empty()
                    || group.shown.iter().any(|&(start, end)| (start..end).contains(&tick))
            })
    }

    /// Rasterises the scene at `tick`, using `palette` alpha to detect blends.
    /// Out-of-range colour indices count as opaque.
    pub(crate) fn rasterise(&self, tick: u32, palette: &[[u8; 4]]) -> Vec<Paint> {
        let mut cells = vec![Paint::Clear; self.width as usize * self.height as usize];
        for group in self.visible(tick) {
            let opaque = opaque(palette, group.colour);
            for (x, y) in group.coverage(self.width, self.height).cells() {
                let cell = &mut cells[(y * self.width + x) as usize];
                *cell = match *cell {
                    Paint::Clear => Paint::Colour(group.colour),
                    _ if opaque => Paint::Colour(group.colour),
                    _ => Paint::Blend,
                };
            }
        }
        cells
    }

    /// Returns the first mismatch between this scene and `anim`, or `None` if
    /// they match exactly.
    pub fn compare(&self, anim: &PixelAnim) -> Option<Mismatch> {
        let expected = (anim.width(), anim.height(), anim.ticks());
        let got = (self.width, self.height, self.ticks);
        if got != expected {
            return Some(Mismatch::Size { expected, got });
        }
        let changes = self.changes();
        let mut start = 0;
        for frame in anim.frames() {
            let end = start + frame.ticks;
            let inside = changes.iter().copied().filter(|&tick| tick > start && tick < end);
            for tick in std::iter::once(start).chain(inside) {
                let cells = self.rasterise(tick, anim.palette());
                for (index, (&expected, &got)) in frame.pixels.iter().zip(&cells).enumerate() {
                    let wanted = if expected == 0 { Paint::Clear } else { Paint::Colour(expected) };
                    if got != wanted {
                        let (x, y) = (index as u32 % self.width, index as u32 / self.width);
                        return Some(Mismatch::Cell { tick, x, y, expected, got });
                    }
                }
            }
            start = end;
        }
        None
    }

    /// Finds every pair of 8-adjacent opaque cells violating the seam
    /// invariant. Translucent cells are excluded because they cannot be painted
    /// over backing colours. Out-of-range palette indices count as opaque.
    pub fn seams(&self, palette: &[[u8; 4]]) -> Vec<Seam> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = Vec::new();
        let changes = self.changes();
        for &tick in &changes[..changes.len() - 1] {
            let groups: Vec<&Group> = self.visible(tick).collect();
            let coverage: Vec<Coverage> =
                groups.iter().map(|group| group.coverage(self.width, self.height)).collect();
            // the topmost group at each cell, and whether it is opaque
            let mut top: Vec<Option<usize>> = vec![None; w * h];
            for (index, cover) in coverage.iter().enumerate() {
                for (x, y) in cover.cells() {
                    top[y as usize * w + x as usize] = Some(index);
                }
            }
            let opaque = |cell: usize| top[cell].is_some_and(|g| opaque(palette, groups[g].colour));
            for y in 0..h {
                for x in 0..w {
                    let a = y * w + x;
                    // right, down-left, down, down-right: every pair once
                    for (dx, dy) in [(1, 0), (-1, 1), (0, 1), (1, 1)] {
                        let (nx, ny) = (x as isize + dx, y as isize + dy);
                        if nx < 0 || nx >= w as isize || ny >= h as isize {
                            continue;
                        }
                        let b = ny as usize * w + nx as usize;
                        if !(opaque(a) && opaque(b)) || top[a] == top[b] {
                            continue;
                        }
                        let (low, high) = if top[a] < top[b] { (a, b) } else { (b, a) };
                        let (hx, hy) = ((high % w) as u32, (high / w) as u32);
                        // one group of the lower colour, under the upper group,
                        // painted over both cells
                        let (lx, ly) = ((low % w) as u32, (low / w) as u32);
                        let colour = groups[top[low].unwrap()].colour;
                        let under = (0..top[high].unwrap()).any(|g| {
                            groups[g].colour == colour
                                && coverage[g].contains(hx, hy)
                                && coverage[g].contains(lx, ly)
                        });
                        if !under {
                            let at = |cell: usize| ((cell % w) as u32, (cell / w) as u32);
                            out.push(Seam { tick, cell: at(high), neighbour: at(low) });
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::frames::{Animation, Frame};

    use super::*;
    use crate::encode;
    use crate::normalise::{Options, normalise};

    const PALETTE: [[u8; 4]; 4] = [[0; 4], [255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 128]];

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Shape {
        Shape::Rect { x, y, width, height }
    }

    fn group(colour: u16, shapes: Vec<Shape>) -> Group {
        Group { colour, rule: FillRule::NonZero, shapes, shown: Vec::new() }
    }

    fn scene(width: u32, height: u32, groups: Vec<Group>) -> Scene {
        Scene {
            width,
            height,
            ticks: 1,
            layers: vec![Layer { from: 0, to: 1, groups, hidden: Vec::new() }],
        }
    }

    fn colours(scene: &Scene) -> Vec<Paint> {
        scene.rasterise(0, &PALETTE)
    }

    #[test]
    fn fills_paths_with_holes() {
        // a 4x4 square with a 2x2 hole cut by a counter-clockwise outline
        let outer = Shape::Path(vec![(0, 0), (4, 0), (4, 4), (0, 4)]);
        let hole = Shape::Path(vec![(1, 1), (1, 3), (3, 3), (3, 1)]);
        let ring = scene(4, 4, vec![group(1, vec![outer.clone(), hole.clone()])]);
        let filled = |scene: &Scene| -> Vec<bool> {
            colours(scene).iter().map(|paint| *paint != Paint::Clear).collect()
        };
        let mut expected = vec![true; 16];
        for index in [5, 6, 9, 10] {
            expected[index] = false;
        }
        assert_eq!(filled(&ring), expected);

        // the even-odd rule cuts holes whatever the direction
        let clockwise_hole = Shape::Path(vec![(1, 1), (3, 1), (3, 3), (1, 3)]);
        let mut even_odd = group(1, vec![outer.clone(), clockwise_hole.clone()]);
        even_odd.rule = FillRule::EvenOdd;
        assert_eq!(filled(&scene(4, 4, vec![even_odd])), expected);
        // the non-zero rule doesn't
        let solid = scene(4, 4, vec![group(1, vec![outer, clockwise_hole])]);
        assert_eq!(filled(&solid), vec![true; 16]);
        // overlapping rectangles are one fill
        let overlap = scene(3, 1, vec![group(3, vec![rect(0, 0, 2, 1), rect(1, 0, 2, 1)])]);
        assert_eq!(colours(&overlap), vec![Paint::Colour(3); 3]);
    }

    #[test]
    fn blends_translucent_colours_over_others() {
        let over =
            scene(2, 1, vec![group(1, vec![rect(0, 0, 1, 1)]), group(3, vec![rect(0, 0, 2, 1)])]);
        assert_eq!(colours(&over), [Paint::Blend, Paint::Colour(3)]);
        // opaque paint covers a blend
        let covered =
            scene(1, 1, vec![group(3, vec![rect(0, 0, 1, 1)]), group(1, vec![rect(0, 0, 1, 1)])]);
        assert_eq!(colours(&covered), [Paint::Colour(1)]);
    }

    #[test]
    fn checks_the_seam_invariant() {
        // red | green side by side: anti-aliasing leaks between them
        let apart =
            scene(2, 1, vec![group(1, vec![rect(0, 0, 1, 1)]), group(2, vec![rect(1, 0, 1, 1)])]);
        let seams = apart.seams(&PALETTE);
        assert_eq!(seams, [Seam { tick: 0, cell: (1, 0), neighbour: (0, 0) }]);
        // red reaching under green is fine
        let under =
            scene(2, 1, vec![group(1, vec![rect(0, 0, 2, 1)]), group(2, vec![rect(1, 0, 1, 1)])]);
        assert!(under.seams(&PALETTE).is_empty());
        // corners count: red at (0, 0), green at (1, 1)
        let diagonal =
            scene(2, 2, vec![group(1, vec![rect(0, 0, 1, 1)]), group(2, vec![rect(1, 1, 1, 1)])]);
        assert_eq!(diagonal.seams(&PALETTE).len(), 1);
        // two groups of one colour have a seam between them too
        let split =
            scene(2, 1, vec![group(1, vec![rect(0, 0, 1, 1)]), group(1, vec![rect(1, 0, 1, 1)])]);
        assert_eq!(split.seams(&PALETTE).len(), 1);
        // translucent cells don't count
        let translucent =
            scene(2, 1, vec![group(1, vec![rect(0, 0, 1, 1)]), group(3, vec![rect(1, 0, 1, 1)])]);
        assert!(translucent.seams(&PALETTE).is_empty());
    }

    #[test]
    fn run_encoding_is_exact() {
        let colour = |frame: usize, x: u32, y: u32| -> [u8; 4] {
            PALETTE[(x as usize + y as usize * 2 + frame) % 4]
        };
        let frames = (0..3)
            .map(|frame| Frame {
                rgba: (0..5 * 4).flat_map(|i| colour(frame, i % 5, i / 5)).collect(),
                duration: Duration::from_millis(100),
            })
            .collect();
        let input = Animation::new(5, 4, frames).unwrap();
        let (anim, _) =
            normalise(input.clone(), &Options { keep_canvas: true, ..Options::default() }).unwrap();
        let scene = encode::runs(&anim);
        assert_eq!(scene.compare(&anim), None);
        assert!(!scene.seams(anim.palette()).is_empty());

        let mut broken = scene.clone();
        broken.layers[1].groups.pop();
        assert!(matches!(broken.compare(&anim), Some(Mismatch::Cell { tick: 6, .. })));
        let mut short = scene;
        short.ticks -= 1;
        let (width, height, ticks) = (anim.width(), anim.height(), anim.ticks());
        let size =
            Mismatch::Size { expected: (width, height, ticks), got: (width, height, ticks - 1) };
        assert_eq!(short.compare(&anim), Some(size));

        // colours past the palette and rectangles past the edge don't panic
        let odd = self::scene(2, 1, vec![group(9, vec![rect(1, 0, u32::MAX, 1)])]);
        assert_eq!(colours(&odd), [Paint::Clear, Paint::Colour(9)]);
        assert!(odd.seams(&PALETTE).is_empty());
        // nor cost more than ones that end at it
        let huge = group(9, vec![rect(0, 0, 1, u32::MAX), rect(1, 0, u32::MAX, u32::MAX)]);
        assert_eq!(huge.coverage(2, 1).cells().collect::<Vec<_>>(), [(0, 0), (1, 0)]);
    }
}

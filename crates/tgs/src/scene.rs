//! What encoders produce: paint groups (one fill over rectangles and
//! rectilinear paths) stacked bottom to top, in layers that are each shown
//! for a range of 60 fps frames. Coordinates are in cells of the
//! [`PixelAnim`] grid.
//!
//! Two properties are checked here rather than in a renderer:
//! - rasterising the scene reproduces the normalised frames exactly;
//! - the seam invariant (see `docs/tgs.md`): wherever two 8-adjacent
//!   opaque cells show different paint groups, the lower group also covers
//!   the other cell, so anti-aliased inner edges are drawn over the right
//!   colour instead of over transparency.

use crate::normalise::PixelAnim;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scene {
    /// Size in cells.
    pub width: u32,
    pub height: u32,
    /// Length in 60 fps frames.
    pub ticks: u32,
    /// Bottom first.
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layer {
    /// Shown for frames `from..to`.
    pub from: u32,
    pub to: u32,
    /// Bottom first.
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// Shapes filled together as one path with one palette colour.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub colour: u16,
    pub rule: FillRule,
    pub shapes: Vec<Shape>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Rect {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// A closed outline through corners, alternating horizontal and
    /// vertical edges. Clockwise (on screen, y down) like rectangles, so
    /// with the non-zero rule it adds to them; counter-clockwise cuts holes.
    Path(Vec<(u32, u32)>),
}

/// A cell of a rasterised scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Clear,
    Colour(u16),
    /// A translucent colour over another colour. Renderers blend these and
    /// round differently, so [`Scene::compare`] counts every blend as wrong,
    /// even one that would come out right; encoders never draw translucent
    /// colours over anything.
    Blend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub tick: u32,
    pub x: u32,
    pub y: u32,
    pub expected: u16,
    pub got: Paint,
}

/// A pair of neighbouring cells that breaks the seam invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seam {
    pub tick: u32,
    /// The cell the lower group should also cover.
    pub cell: (u32, u32),
    /// Its neighbour, which shows the lower group.
    pub neighbour: (u32, u32),
}

impl Shape {
    /// The box around the shape: `x0, y0, x1, y1`, ends excluded.
    fn bounds(&self) -> (u32, u32, u32, u32) {
        match *self {
            Shape::Rect { x, y, width, height } => (x, y, x + width, y + height),
            Shape::Path(ref corners) => {
                corners.iter().fold((u32::MAX, u32::MAX, 0, 0), |(x0, y0, x1, y1), &(x, y)| {
                    (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                })
            }
        }
    }

    /// Adds the shape's vertical edges crossing row `y` to `winding`, which
    /// holds the change in winding number at each column edge from `left`.
    fn add_crossings(&self, y: u32, left: u32, winding: &mut [i32]) {
        match *self {
            Shape::Rect { x, y: top, width, height } => {
                if (top..top + height).contains(&y) {
                    winding[(x - left) as usize] += 1;
                    winding[(x + width - left) as usize] -= 1;
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
                        winding[(x - left) as usize] += 1;
                    } else if y0 < y1 && (y0..y1).contains(&y) {
                        winding[(x - left) as usize] -= 1;
                    }
                }
            }
        }
    }
}

/// The cells a group fills, kept for the box around its shapes only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    left: u32,
    top: u32,
    width: u32,
    cells: Vec<bool>,
}

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

impl Group {
    /// Cells the group fills.
    pub fn coverage(&self) -> Coverage {
        let (left, top, right, bottom) = self
            .shapes
            .iter()
            .map(Shape::bounds)
            .fold((u32::MAX, u32::MAX, 0, 0), |(x0, y0, x1, y1), (a, b, c, d)| {
                (x0.min(a), y0.min(b), x1.max(c), y1.max(d))
            });
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

impl Scene {
    /// Frames where the set of visible layers changes, from 0 to `ticks`.
    fn changes(&self) -> Vec<u32> {
        let mut ticks = vec![0, self.ticks];
        for layer in &self.layers {
            ticks.extend([layer.from.min(self.ticks), layer.to.min(self.ticks)]);
        }
        ticks.sort_unstable();
        ticks.dedup();
        ticks
    }

    /// Visible groups at a frame, bottom first.
    fn visible(&self, tick: u32) -> impl Iterator<Item = &Group> {
        self.layers
            .iter()
            .filter(move |layer| (layer.from..layer.to).contains(&tick))
            .flat_map(|layer| &layer.groups)
    }

    /// The scene at a frame, with `palette` alpha deciding what blends.
    pub fn rasterise(&self, tick: u32, palette: &[[u8; 4]]) -> Vec<Paint> {
        let mut cells = vec![Paint::Clear; self.width as usize * self.height as usize];
        for group in self.visible(tick) {
            let opaque = palette[group.colour as usize][3] == 255;
            for (x, y) in group.coverage().cells() {
                if x >= self.width || y >= self.height {
                    continue;
                }
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

    /// The first cell that differs from `anim`, if any. A scene of another
    /// size or length differs at its first cell.
    pub fn compare(&self, anim: &PixelAnim) -> Option<Mismatch> {
        let differs = |tick| Mismatch { tick, x: 0, y: 0, expected: 0, got: Paint::Blend };
        if (self.width, self.height, self.ticks) != (anim.width(), anim.height(), anim.ticks()) {
            return Some(differs(0));
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
                        return Some(Mismatch { tick, x, y, expected, got });
                    }
                }
            }
            start = end;
        }
        None
    }

    /// Every place the seam invariant is broken. Only pairs of opaque cells
    /// count: a translucent colour can't be drawn over anything.
    pub fn seams(&self, palette: &[[u8; 4]]) -> Vec<Seam> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = Vec::new();
        let changes = self.changes();
        for &tick in &changes[..changes.len() - 1] {
            let groups: Vec<&Group> = self.visible(tick).collect();
            let coverage: Vec<Coverage> = groups.iter().map(|group| group.coverage()).collect();
            // the topmost group at each cell, and whether it is opaque
            let mut top: Vec<Option<usize>> = vec![None; w * h];
            for (index, cover) in coverage.iter().enumerate() {
                for (x, y) in cover.cells() {
                    if (x as usize) < w && (y as usize) < h {
                        top[y as usize * w + x as usize] = Some(index);
                    }
                }
            }
            let opaque = |cell: usize| {
                top[cell].is_some_and(|g| palette[groups[g].colour as usize][3] == 255)
            };
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
                        if !coverage[top[low].unwrap()].contains(hx, hy) {
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

    use tgradish_frames::{Animation, Frame};

    use super::*;
    use crate::encode;
    use crate::normalise::{Options, normalise};

    const PALETTE: [[u8; 4]; 4] = [[0; 4], [255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 128]];

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Shape {
        Shape::Rect { x, y, width, height }
    }

    fn group(colour: u16, shapes: Vec<Shape>) -> Group {
        Group { colour, rule: FillRule::NonZero, shapes }
    }

    fn scene(width: u32, height: u32, groups: Vec<Group>) -> Scene {
        Scene { width, height, ticks: 1, layers: vec![Layer { from: 0, to: 1, groups }] }
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
            normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap();
        let scene = encode::runs(&anim);
        assert_eq!(scene.compare(&anim), None);
        assert!(!scene.seams(anim.palette()).is_empty());

        let mut broken = scene.clone();
        broken.layers[1].groups.pop();
        assert!(matches!(broken.compare(&anim), Some(Mismatch { tick: 6, .. })));
        let mut short = scene;
        short.ticks -= 1;
        assert!(short.compare(&anim).is_some());
    }
}

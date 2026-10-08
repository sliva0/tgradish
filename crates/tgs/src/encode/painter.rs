//! Encoder v1 (see "Encode: the model" in `docs/tgs.md`): painter's
//! layers that keep the seam invariant, rectangle covers and lifetimes.
//!
//! Colours are drawn in one order for the whole animation. A colour's
//! shape must hold its own cells and every 8-neighbour that shows a later
//! opaque colour (the seam invariant), and may hold any other cell of a
//! later opaque colour, which paints over it. Translucent cells are never
//! drawn over, since renderers would blend them.
//!
//! A colour keeps one shape for as long as one shape fits every frame,
//! which becomes a layer's lifetime. So each frame still has exactly one
//! group per colour, and the invariant holds in every frame.
//!
//! What a colour costs depends only on which colours are drawn after it,
//! not on their order, so the cheapest order is found exactly over subsets
//! for few colours, and greedily otherwise.

use super::cover::{Rect, cover};
use super::mask::Mask;
use crate::limits::TLOTTIE;
use crate::normalise::PixelAnim;
use crate::scene::{FillRule, Group, Layer, Scene, Shape};

/// Estimated cost of a rectangle and of a group, in the same units: a
/// group's fill and wrapping take about one and a half rectangles' bytes.
const RECT_COST: usize = 2;
const GROUP_COST: usize = 3;
/// Up to this many colours, the order is searched exactly.
const EXACT_ORDER: usize = 10;
/// Above this many colours, the order isn't searched at all.
const GREEDY_ORDER: usize = 256;

/// How hard to look for a small encoding.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// A quick guess at the drawing order: larger colours first.
    Fast,
    /// The order with the fewest rectangles and groups.
    #[default]
    Balanced,
    /// Starting from that, swaps neighbouring colours in the order while
    /// the real compressed size goes down.
    Best,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Keep shapes alive over frames where they don't change. Without it,
    /// every frame is drawn in full.
    pub lifetimes: bool,
    pub effort: Effort,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { lifetimes: true, effort: Effort::default() }
    }
}

/// Most encodings [`Effort::Best`] tries.
const BEST_TRIES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error(
        "a colour needs {rects} rectangles in one frame, Telegram's renderer allows {}",
        TLOTTIE.max_paint_source_items_per_layer
    )]
    TooManyRects { rects: usize },
    #[error("the animation needs {layers} layers, Telegram's renderer allows {}", TLOTTIE.max_painted_shape_layers)]
    TooManyLayers { layers: usize },
}

/// One colour's shape over frames `from..to` (not 60 fps ticks).
struct Piece {
    from: usize,
    to: usize,
    rects: Vec<Rect>,
}

fn cost(pieces: &[Piece]) -> usize {
    pieces.iter().map(|piece| piece.rects.len() * RECT_COST + GROUP_COST).sum()
}

struct Painter<'a> {
    anim: &'a PixelAnim,
    /// Cells of each colour in each frame: `cells[frame][colour]`.
    cells: Vec<Vec<Vec<u32>>>,
    lifetimes: bool,
}

impl Painter<'_> {
    fn opaque(&self, colour: u16) -> bool {
        self.anim.palette()[colour as usize][3] == 255
    }

    fn frames(&self) -> usize {
        self.cells.len()
    }

    fn empty(&self) -> Mask {
        Mask::new(self.anim.width(), self.anim.height())
    }

    fn own(&self, colour: u16, frame: usize) -> Mask {
        let width = self.anim.width();
        let mut mask = self.empty();
        for &index in &self.cells[frame][colour as usize] {
            mask.set(index % width, index / width);
        }
        mask
    }

    /// The colours present, quickest guess first: larger bounding boxes,
    /// then more cells.
    fn guessed_order(&self) -> Vec<u16> {
        let colours = self.anim.palette().len();
        let width = self.anim.width();
        let mut bounds = vec![(u32::MAX, u32::MAX, 0u32, 0u32); colours];
        let mut counts = vec![0usize; colours];
        for frame in &self.cells {
            for (colour, cells) in frame.iter().enumerate() {
                for &index in cells {
                    let (x, y) = (index % width, index / width);
                    let b = &mut bounds[colour];
                    *b = (b.0.min(x), b.1.min(y), b.2.max(x + 1), b.3.max(y + 1));
                }
                counts[colour] += cells.len();
            }
        }
        let mut order: Vec<u16> = (1..colours as u16).filter(|&c| counts[c as usize] > 0).collect();
        let area = |c: u16| {
            let (x0, y0, x1, y1) = bounds[c as usize];
            u64::from(x1 - x0) * u64::from(y1 - y0)
        };
        order.sort_by_key(|&c| {
            (std::cmp::Reverse(area(c)), std::cmp::Reverse(counts[c as usize]), c)
        });
        order
    }

    /// The pieces of `colour` when `later[frame]` holds the cells of the
    /// opaque colours drawn after it.
    fn pieces(&self, colour: u16, later: &[Mask]) -> Result<Vec<Piece>, EncodeError> {
        let mut pieces = Vec::new();
        // the open piece: its first frame, the cells it must and may cover
        let mut open: Option<(usize, Mask, Mask)> = None;
        let close = |from, to, must: &Mask, may: &Mask| {
            let rects = cover(must, may);
            if rects.len() > TLOTTIE.max_paint_source_items_per_layer {
                return Err(EncodeError::TooManyRects { rects: rects.len() });
            }
            Ok(Piece { from, to, rects })
        };
        for (frame, later) in later.iter().enumerate() {
            let own = self.own(colour, frame);
            let mut must = own.clone();
            if self.opaque(colour) {
                let mut reach = own.grown();
                reach.intersect(later);
                must.union(&reach);
            }
            let mut may = own.clone();
            may.union(later);
            if let Some((start, union, within)) = &mut open {
                let mut joined = union.clone();
                joined.union(&must);
                let mut narrowed = within.clone();
                narrowed.intersect(&may);
                if self.lifetimes && joined.is_subset(&narrowed) {
                    *union = joined;
                    *within = narrowed;
                    continue;
                }
                pieces.push(close(*start, frame, union, within)?);
                open = None;
            }
            if !own.is_empty() {
                open = Some((frame, must, may));
            }
        }
        if let Some((start, union, within)) = open {
            pieces.push(close(start, self.frames(), &union, &within)?);
        }
        Ok(pieces)
    }

    fn cost_of(&self, colour: u16, later: &[Mask]) -> usize {
        self.pieces(colour, later).map_or(usize::MAX / 4, |pieces| cost(&pieces))
    }

    /// The cheapest order, by dynamic programming over the sets of colours
    /// drawn on top.
    fn exact_order(&self, colours: &[u16]) -> Vec<u16> {
        let n = colours.len();
        let own: Vec<Vec<Mask>> =
            colours.iter().map(|&c| (0..self.frames()).map(|f| self.own(c, f)).collect()).collect();
        let later_of = |set: usize| -> Vec<Mask> {
            let mut later = vec![self.empty(); self.frames()];
            for (i, masks) in own.iter().enumerate() {
                if set >> i & 1 == 1 && self.opaque(colours[i]) {
                    later.iter_mut().zip(masks).for_each(|(l, m)| l.union(m));
                }
            }
            later
        };
        // best[set]: the cheapest way to draw `set` on top of the rest,
        // and which of its colours goes lowest
        let mut best = vec![(0usize, 0usize); 1 << n];
        for set in 1..1usize << n {
            best[set] = (0..n)
                .filter(|&i| set >> i & 1 == 1)
                .map(|i| {
                    let above = set & !(1 << i);
                    (best[above].0 + self.cost_of(colours[i], &later_of(above)), i)
                })
                .min()
                .unwrap();
        }
        let mut order = Vec::with_capacity(n);
        let mut set = (1 << n) - 1;
        while set != 0 {
            let lowest = best[set].1;
            order.push(colours[lowest]);
            set &= !(1 << lowest);
        }
        order
    }

    /// Builds the order from the top: each step places the colour that
    /// loses the least by being drawn there rather than at the bottom.
    fn greedy_order(&self, colours: &[u16]) -> Vec<u16> {
        let mut everything = vec![self.empty(); self.frames()];
        for &colour in colours.iter().filter(|&&c| self.opaque(c)) {
            for (f, mask) in everything.iter_mut().enumerate() {
                mask.union(&self.own(colour, f));
            }
        }
        // cells of different colours never overlap, so "every colour but
        // this one" is a subtraction
        let at_bottom: Vec<usize> = colours
            .iter()
            .map(|&colour| {
                let later: Vec<Mask> = everything
                    .iter()
                    .enumerate()
                    .map(|(f, mask)| {
                        let mut later = mask.clone();
                        later.subtract(&self.own(colour, f));
                        later
                    })
                    .collect();
                self.cost_of(colour, &later)
            })
            .collect();
        let mut remaining: Vec<usize> = (0..colours.len()).collect();
        let mut later = vec![self.empty(); self.frames()];
        let mut top_down = Vec::with_capacity(colours.len());
        while !remaining.is_empty() {
            let (position, &index) = remaining
                .iter()
                .enumerate()
                .min_by_key(|&(_, &i)| {
                    (self.cost_of(colours[i], &later) as i64 - at_bottom[i] as i64, i)
                })
                .unwrap();
            remaining.remove(position);
            let colour = colours[index];
            top_down.push(colour);
            if self.opaque(colour) {
                for (f, mask) in later.iter_mut().enumerate() {
                    mask.union(&self.own(colour, f));
                }
            }
        }
        top_down.reverse();
        top_down
    }

    fn encode(&self, order: &[u16]) -> Result<Scene, EncodeError> {
        // from the top colour down, so `later` grows as colours are done
        let mut later = vec![self.empty(); self.frames()];
        let mut pieces = Vec::new();
        for (rank, &colour) in order.iter().enumerate().rev() {
            for piece in self.pieces(colour, &later)? {
                pieces.push((rank, colour, piece));
            }
            if self.opaque(colour) {
                for (f, mask) in later.iter_mut().enumerate() {
                    mask.union(&self.own(colour, f));
                }
            }
        }
        pieces.sort_by_key(|(rank, _, piece)| (*rank, piece.from));

        let mut starts = Vec::with_capacity(self.frames() + 1);
        starts.push(0);
        for frame in self.anim.frames() {
            starts.push(starts.last().unwrap() + frame.ticks);
        }
        Ok(Scene {
            width: self.anim.width(),
            height: self.anim.height(),
            ticks: *starts.last().unwrap(),
            layers: stack(pieces, &starts)?,
        })
    }
}

/// Encodes `anim`. [`Effort::Best`] compares candidates by `score`
/// (smaller is better), typically the compressed size of their Lottie;
/// without one it works like [`Effort::Balanced`].
pub fn painter(
    anim: &PixelAnim,
    settings: &Settings,
    score: Option<&dyn Fn(&Scene) -> usize>,
) -> Result<Scene, EncodeError> {
    let mut cells = Vec::with_capacity(anim.frames().len());
    for frame in anim.frames() {
        let mut by_colour = vec![Vec::new(); anim.palette().len()];
        for (index, &colour) in frame.pixels.iter().enumerate() {
            by_colour[colour as usize].push(index as u32);
        }
        cells.push(by_colour);
    }
    let mut painter = Painter { anim, cells, lifetimes: settings.lifetimes };
    if settings.lifetimes {
        match painter.search(settings.effort, score) {
            // a layer per frame stays within the limit for up to 180 frames
            Err(EncodeError::TooManyLayers { .. }) => painter.lifetimes = false,
            result => return result,
        }
    }
    painter.search(settings.effort, score)
}

impl Painter<'_> {
    fn search(
        &self,
        effort: Effort,
        score: Option<&dyn Fn(&Scene) -> usize>,
    ) -> Result<Scene, EncodeError> {
        let guess = self.guessed_order();
        let searched = || match guess.len() {
            n if n <= EXACT_ORDER => self.exact_order(&guess),
            n if n <= GREEDY_ORDER => self.greedy_order(&guess),
            _ => guess.clone(),
        };
        let score = match (effort, score) {
            (Effort::Fast, _) => return self.encode(&guess),
            (Effort::Balanced, _) | (Effort::Best, None) => return self.encode(&searched()),
            (Effort::Best, Some(score)) => score,
        };
        // the better of both starting orders, then neighbours swapped while
        // that helps
        let try_order = |order: &[u16]| self.encode(order).ok().map(|scene| (score(&scene), scene));
        let mut order = searched();
        let mut best = try_order(&order);
        if let Some(guessed) = try_order(&guess)
            && best.as_ref().is_none_or(|(size, _)| guessed.0 < *size)
        {
            (order, best) = (guess, Some(guessed));
        }
        let Some((mut smallest, mut scene)) = best else {
            // no order fits tlottie's limits; report why
            return self.encode(&order);
        };
        let mut tries = 2;
        let mut improved = true;
        while improved && tries < BEST_TRIES {
            improved = false;
            for i in 0..order.len().saturating_sub(1) {
                if tries >= BEST_TRIES {
                    break;
                }
                tries += 1;
                order.swap(i, i + 1);
                match try_order(&order) {
                    Some((size, candidate)) if size < smallest => {
                        (smallest, scene) = (size, candidate);
                        improved = true;
                    }
                    _ => order.swap(i, i + 1),
                }
            }
        }
        Ok(scene)
    }
}

/// Puts pieces, in drawing order, into layers: a piece joins the highest
/// layer with the same lifetime when no layer above that one is shown at
/// the same time and tlottie's per-layer limits allow; otherwise it starts
/// a layer on top.
fn stack(pieces: Vec<(usize, u16, Piece)>, starts: &[u32]) -> Result<Vec<Layer>, EncodeError> {
    // tlottie counts every group, rectangle, fill and group transform as a
    // shape, and every rectangle a fill paints as a paint source
    let fits = |groups: usize, rects: usize| {
        rects <= TLOTTIE.max_paint_source_items_per_layer
            && groups <= TLOTTIE.max_paints_per_layer
            && rects + 3 * groups <= TLOTTIE.max_shapes_per_layer
    };
    // per layer: frames, groups and rectangles so far
    let mut layers: Vec<(usize, usize, Vec<Group>, usize)> = Vec::new();
    for (_, colour, piece) in pieces {
        let mut target = None;
        for (index, (from, to, groups, rects)) in layers.iter().enumerate().rev() {
            if (*from, *to) == (piece.from, piece.to) {
                if fits(groups.len() + 1, rects + piece.rects.len()) {
                    target = Some(index);
                }
                break;
            }
            if *from < piece.to && piece.from < *to {
                break;
            }
        }
        let count = piece.rects.len();
        let group = Group {
            colour,
            rule: FillRule::NonZero,
            shapes: piece
                .rects
                .into_iter()
                .map(|(x, y, width, height)| Shape::Rect { x, y, width, height })
                .collect(),
        };
        match target {
            Some(index) => {
                layers[index].2.push(group);
                layers[index].3 += count;
            }
            None => layers.push((piece.from, piece.to, vec![group], count)),
        }
    }
    if layers.len() > TLOTTIE.max_painted_shape_layers {
        return Err(EncodeError::TooManyLayers { layers: layers.len() });
    }
    Ok(layers
        .into_iter()
        .map(|(from, to, groups, _)| Layer { from: starts[from], to: starts[to], groups })
        .collect())
}

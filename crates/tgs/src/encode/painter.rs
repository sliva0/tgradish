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
use crate::limits::{TLOTTIE, telegram};
use crate::normalise::PixelAnim;
use crate::scene::{FillRule, Group, Layer, Scene, Shape};

/// Estimated cost of a rectangle and of a group, in the same units: a
/// group's fill and wrapping take about one and a half rectangles' bytes.
const RECT_COST: usize = 2;
const GROUP_COST: usize = 3;
/// What a layer of its own adds, measured on the corpus.
const LAYER_COST: usize = 10;
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
    /// Keep the unchanged part of a colour alive over frames where the
    /// rest changes, when that is cheaper.
    pub split: bool,
    pub effort: Effort,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { lifetimes: true, split: true, effort: Effort::default() }
    }
}

/// Most encodings [`Effort::Best`] tries.
const BEST_TRIES: usize = 200;

const fn min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

/// Per layer: the stricter of tlottie's and Telegram's limits.
const MAX_SHAPES: usize = min(TLOTTIE.max_shapes_per_layer, telegram::MAX_SHAPES_PER_LAYER);
/// Rectangles of one colour in one frame: a layer holding only them adds
/// their group, its fill and its transform.
const MAX_GROUP_RECTS: usize = min(TLOTTIE.max_paint_source_items_per_layer, MAX_SHAPES - 3);
const MAX_LAYERS: usize = min(TLOTTIE.max_painted_shape_layers, telegram::MAX_LAYERS);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("a colour needs {rects} rectangles in one frame, Telegram allows {MAX_GROUP_RECTS}")]
    TooManyRects { rects: usize },
    #[error("the animation needs {layers} layers, Telegram allows {MAX_LAYERS}")]
    TooManyLayers { layers: usize },
}

/// One colour's shape over frames `from..to` (not 60 fps ticks).
struct Piece {
    from: usize,
    to: usize,
    rects: Vec<Rect>,
}

fn cost(pieces: &[(u8, Piece)]) -> usize {
    pieces.iter().fold(0, |total: usize, (_, piece)| {
        total.saturating_add(piece.rects.len() * RECT_COST + GROUP_COST)
    })
}

/// The cost of a colour that can't be drawn at all within tlottie's
/// limits; sums of it saturate.
const IMPOSSIBLE: usize = usize::MAX;

/// [`cost`] of a split colour: its deltas mostly need layers of their own,
/// since its core lies between them and the next colour's.
fn split_cost(pieces: &[(u8, Piece)]) -> usize {
    let deltas = pieces.iter().filter(|(place, _)| *place == 0).count();
    cost(pieces).saturating_add(deltas * LAYER_COST)
}

/// A colour's core lasts while it keeps at least this share of each
/// frame's cells; lower and higher were both worse over the corpus.
const CORE_SHARE: f64 = 0.15;

/// The 8 neighbours of a cell, inside the mask's bounds.
fn core_neighbours(cell: (u32, u32), mask: &Mask) -> impl Iterator<Item = (u32, u32)> + '_ {
    let (x, y) = cell;
    (y.saturating_sub(1)..=(y + 1).min(mask.height() - 1)).flat_map(move |ny| {
        (x.saturating_sub(1)..=(x + 1).min(mask.width() - 1))
            .filter(move |&nx| (nx, ny) != (x, y))
            .map(move |nx| (nx, ny))
    })
}

struct Painter<'a> {
    anim: &'a PixelAnim,
    /// Cells of each colour in each frame: `cells[frame][colour]`.
    cells: Vec<Vec<Vec<u32>>>,
    lifetimes: bool,
    /// Split colours into cores and deltas where that is cheaper.
    split: bool,
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
        // palette indices fit u16, the palette's length may not
        let mut order: Vec<u16> =
            (1..colours).filter(|&c| counts[c] > 0).map(|c| c as u16).collect();
        let area = |c: u16| {
            let (x0, y0, x1, y1) = bounds[c as usize];
            u64::from(x1 - x0) * u64::from(y1 - y0)
        };
        order.sort_by_key(|&c| {
            (std::cmp::Reverse(area(c)), std::cmp::Reverse(counts[c as usize]), c)
        });
        order
    }

    /// Pieces drawing, in each frame, the cells `needs[frame]` says it
    /// must (nothing when `None`) within those it may. A piece lives on
    /// while one shape fits every frame.
    fn lifetime_pieces(&self, needs: Vec<(Option<Mask>, Mask)>) -> Result<Vec<Piece>, EncodeError> {
        let mut pieces = Vec::new();
        // the open piece: its first frame, the cells it must and may cover
        let mut open: Option<(usize, Mask, Mask)> = None;
        let close = |from, to, must: &Mask, may: &Mask| {
            let rects = cover(must, may);
            if rects.len() > MAX_GROUP_RECTS {
                return Err(EncodeError::TooManyRects { rects: rects.len() });
            }
            Ok(Piece { from, to, rects })
        };
        let frames = needs.len();
        for (frame, (must, may)) in needs.into_iter().enumerate() {
            if let Some((start, union, within)) = &mut open {
                let mut joined = union.clone();
                if let Some(must) = &must {
                    joined.union(must);
                }
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
            if let Some(must) = must {
                open = Some((frame, must, may));
            }
        }
        if let Some((start, union, within)) = open {
            pieces.push(close(start, frames, &union, &within)?);
        }
        Ok(pieces)
    }

    /// What `colour` must and may cover in a frame, given the cells of the
    /// opaque colours after it, when nothing else of it is drawn.
    fn needs(&self, own: &Mask, later: &Mask, opaque: bool) -> (Option<Mask>, Mask) {
        let mut may = own.clone();
        may.union(later);
        if own.is_empty() {
            return (None, may);
        }
        let mut must = own.clone();
        if opaque {
            let mut reach = own.grown();
            reach.intersect(later);
            must.union(&reach);
        }
        (Some(must), may)
    }

    /// The pieces of `colour` when `later[frame]` holds the cells of the
    /// opaque colours drawn after it, each with its place under the colour:
    /// 0 for a part drawn below the rest, 1 otherwise.
    fn pieces(&self, colour: u16, later: &[Mask]) -> Result<Vec<(u8, Piece)>, EncodeError> {
        let own: Vec<Mask> = (0..self.frames()).map(|f| self.own(colour, f)).collect();
        let whole = self.lifetime_pieces(
            own.iter()
                .zip(later)
                .map(|(own, later)| self.needs(own, later, self.opaque(colour)))
                .collect(),
        )?;
        let whole: Vec<(u8, Piece)> = whole.into_iter().map(|piece| (1, piece)).collect();
        if !self.lifetimes || !self.split || !self.opaque(colour) {
            return Ok(whole);
        }
        match self.split_pieces(&own, later) {
            Ok(split) if split_cost(&split) < cost(&whole) => Ok(split),
            _ => Ok(whole),
        }
    }

    /// The cores of a colour: for each stretch of frames where most of its
    /// cells keep it, those cells, drawn once for the stretch. `later[frame]`
    /// holds the cells of the opaque groups drawn above the cores. Returns
    /// the core pieces and each frame's core cells.
    fn cores(
        &self,
        own: &[Mask],
        later: &[Mask],
    ) -> Result<(Vec<Piece>, Vec<Option<Mask>>), EncodeError> {
        let frames = own.len();
        let mut pieces = Vec::new();
        let mut at = Vec::with_capacity(frames);
        let mut start = 0;
        while start < frames {
            // the stretch: frames while the cells kept from its first are
            // most of each frame's cells
            let mut core = own[start].clone();
            let mut end = start + 1;
            while end < frames {
                let mut kept = core.clone();
                kept.intersect(&own[end]);
                if kept.is_empty() || (kept.count() as f64) < CORE_SHARE * own[end].count() as f64 {
                    break;
                }
                core = kept;
                end += 1;
            }
            let span = start..end;
            start = end;
            if span.len() == 1 {
                at.push(None);
                continue;
            }
            // cells under later colours in every frame of the stretch, which
            // the core may cover without ever showing
            let mut hidden = later[span.start].clone();
            for f in span.clone() {
                hidden.intersect(&later[f]);
            }
            // the core shows exactly its cells, so it must reach under every
            // neighbour that a later colour shows in some frame; that is only
            // possible where the neighbour is the core's or always hidden
            loop {
                let reached = |(x, y): (u32, u32)| span.clone().any(|f| later[f].get(x, y));
                let unsure: Vec<(u32, u32)> = core
                    .cells()
                    .filter(|&cell| {
                        core_neighbours(cell, &core)
                            .any(|n| reached(n) && !core.get(n.0, n.1) && !hidden.get(n.0, n.1))
                    })
                    .collect();
                if unsure.is_empty() {
                    break;
                }
                for (x, y) in unsure {
                    core.clear(x, y);
                }
            }
            if core.is_empty() {
                at.extend(span.map(|_| None));
                continue;
            }
            let mut must = core.clone();
            for f in span.clone() {
                let mut reach = core.grown();
                reach.intersect(&later[f]);
                must.union(&reach);
            }
            let mut may = core.clone();
            may.union(&hidden);
            let rects = cover(&must, &may);
            if rects.len() > MAX_GROUP_RECTS {
                return Err(EncodeError::TooManyRects { rects: rects.len() });
            }
            pieces.push(Piece { from: span.start, to: span.end, rects });
            at.extend(span.map(|_| Some(core.clone())));
        }
        Ok((pieces, at))
    }

    /// A colour as cores (see [`Painter::cores`]), and below them a delta
    /// with the rest of each frame.
    ///
    /// A delta lies directly under its core, so where they meet the delta
    /// must reach under the core's cells (the seam invariant between two
    /// groups), which is fine: the core paints the same colour over it.
    fn split_pieces(&self, own: &[Mask], later: &[Mask]) -> Result<Vec<(u8, Piece)>, EncodeError> {
        let (cores, at) = self.cores(own, later)?;
        let deltas = own
            .iter()
            .zip(later)
            .zip(at)
            .map(|((own, later), core)| {
                let Some(core) = core else {
                    return self.needs(own, later, true);
                };
                // the cells the core doesn't draw; the delta must also reach
                // under the core's where they meet
                let mut rest = own.clone();
                rest.subtract(&core);
                let mut above = later.clone();
                above.union(&core);
                let (must, mut may) = self.needs(&rest, &above, true);
                may.union(own);
                (must, may)
            })
            .collect();
        let mut out: Vec<(u8, Piece)> = cores.into_iter().map(|piece| (1, piece)).collect();
        out.extend(self.lifetime_pieces(deltas)?.into_iter().map(|piece| (0, piece)));
        Ok(out)
    }

    fn cost_of(&self, colour: u16, later: &[Mask]) -> usize {
        self.pieces(colour, later).map_or(IMPOSSIBLE, |pieces| cost(&pieces))
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
                    (best[above].0.saturating_add(self.cost_of(colours[i], &later_of(above))), i)
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
                    (self.cost_of(colours[i], &later) as i128 - at_bottom[i] as i128, i)
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
        let pieces = self.ordered_pieces(order)?;
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

    /// Every colour's pieces in `order`, each split colour's delta just
    /// below its cores, sorted by drawing order.
    fn ordered_pieces(&self, order: &[u16]) -> Result<Vec<(usize, u16, Piece)>, EncodeError> {
        // from the top colour down, so `later` grows as colours are done
        let mut later = vec![self.empty(); self.frames()];
        let mut pieces = Vec::new();
        for (rank, &colour) in order.iter().enumerate().rev() {
            for (place, piece) in self.pieces(colour, &later)? {
                pieces.push((rank * 2 + usize::from(place), colour, piece));
            }
            if self.opaque(colour) {
                for (f, mask) in later.iter_mut().enumerate() {
                    mask.union(&self.own(colour, f));
                }
            }
        }
        pieces.sort_by_key(|(rank, _, piece)| (*rank, piece.from));
        Ok(pieces)
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
    let mut painter = Painter { anim, cells, lifetimes: settings.lifetimes, split: settings.split };
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
/// the same time and the per-layer limits allow; otherwise it starts a
/// layer on top.
fn stack(pieces: Vec<(usize, u16, Piece)>, starts: &[u32]) -> Result<Vec<Layer>, EncodeError> {
    // tlottie counts every group, rectangle, fill and group transform as a
    // shape, and every rectangle a fill paints as a paint source
    let fits = |groups: usize, rects: usize| {
        rects <= TLOTTIE.max_paint_source_items_per_layer
            && groups <= TLOTTIE.max_paints_per_layer
            && rects + 3 * groups <= MAX_SHAPES
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
    if layers.len() > MAX_LAYERS {
        return Err(EncodeError::TooManyLayers { layers: layers.len() });
    }
    Ok(layers
        .into_iter()
        .map(|(from, to, groups, _)| Layer { from: starts[from], to: starts[to], groups })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tgradish_frames::{Animation, Frame};

    use super::*;
    use crate::normalise::{Options, normalise};

    #[test]
    fn survives_orders_that_cant_be_drawn() {
        // five translucent colours in diagonal stripes: every order needs
        // more rectangles than a layer takes
        let size = 161u32;
        let colours = [
            [255, 0, 0, 100],
            [0, 255, 0, 100],
            [0, 0, 255, 100],
            [9, 9, 9, 100],
            [200, 200, 0, 100],
        ];
        let rgba =
            (0..size * size).flat_map(|i| colours[((i % size + i / size) % 5) as usize]).collect();
        let input =
            Animation::new(size, size, vec![Frame { rgba, duration: Duration::from_millis(100) }])
                .unwrap();
        let anim = normalise(&input, &Options::default()).unwrap().0;
        assert!(matches!(
            painter(&anim, &Settings::default(), None),
            Err(EncodeError::TooManyRects { .. })
        ));
    }
}

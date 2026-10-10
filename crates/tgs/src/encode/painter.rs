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
/// Side of the tiles a colour can be cut into, in cells: 16 and 64 were
/// both worse.
const TILE: u32 = 32;
/// Up to this many colours, the order is searched exactly.
const EXACT_ORDER: usize = 10;
/// Above this many colours times cells times frames, the order is only
/// guessed whatever the effort: searching it would take minutes. The
/// corpus comes to at most 11 million, `blue_ball_machine` to 1.9 billion.
const SEARCH_WORK: u64 = 200_000_000;
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
    /// Draw a shape that comes back later once, in a layer hidden in
    /// between.
    pub reuse: bool,
    pub effort: Effort,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { lifetimes: true, split: true, reuse: true, effort: Effort::default() }
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
    /// Where the piece's tile comes among its colour's, bottom first; 0
    /// when the colour isn't tiled.
    tile: u32,
}

fn cost(pieces: &[(u8, Piece)]) -> usize {
    pieces.iter().fold(0, |total: usize, (_, piece)| {
        total.saturating_add(piece.rects.len() * RECT_COST + GROUP_COST)
    })
}

/// The cost of a colour that can't be drawn at all within tlottie's
/// limits; sums of it saturate.
const IMPOSSIBLE: usize = usize::MAX;

/// [`cost`] of a tiled colour: tiles with lifetimes of their own need
/// layers of their own, about one for each piece past the first in each
/// tile.
fn tiled_cost(pieces: &[(u8, Piece)]) -> usize {
    let tiles: std::collections::BTreeSet<u32> =
        pieces.iter().map(|(_, piece)| piece.tile).collect();
    let extra = pieces.len().saturating_sub(tiles.len());
    cost(pieces).saturating_add(extra * LAYER_COST)
}

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
    reuse: bool,
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
    fn lifetime_pieces(
        &self,
        needs: Vec<(Option<Mask>, Mask)>,
        opaque: bool,
    ) -> Result<Vec<Piece>, EncodeError> {
        let mut pieces = Vec::new();
        // the open piece: its first frame, the cells it must and may cover
        let mut open: Option<(usize, Mask, Mask)> = None;
        let close = |from, to, must: &Mask, may: &Mask| -> Result<Vec<Piece>, EncodeError> {
            let groups = covers(must, may, opaque)?;
            Ok(groups.into_iter().map(|rects| Piece { from, to, rects, tile: 0 }).collect())
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
                pieces.extend(close(*start, frame, union, within)?);
                open = None;
            }
            if let Some(must) = must {
                open = Some((frame, must, may));
            }
        }
        if let Some((start, union, within)) = open {
            pieces.extend(close(start, frames, &union, &within)?);
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
    fn pieces(
        &self,
        colour: u16,
        later: &[Mask],
        tiles: bool,
    ) -> Result<Vec<(u8, Piece)>, EncodeError> {
        let own: Vec<Mask> = (0..self.frames()).map(|f| self.own(colour, f)).collect();
        // everything but tiles happens within a cell of the colour's cells:
        // worked out in a window of that, and moved back
        let mut any = self.empty();
        own.iter().for_each(|mask| any.union(mask));
        let Some((x, y, w, h)) = any.bounds() else { return Ok(Vec::new()) };
        let (left, top) = (x.saturating_sub(1), y.saturating_sub(1));
        let width = (x + w + 1).min(self.anim.width()) - left;
        let height = (y + h + 1).min(self.anim.height()) - top;
        let crop = |masks: &[Mask]| -> Vec<Mask> {
            masks.iter().map(|mask| mask.crop(left, top, width, height)).collect()
        };
        let (own_window, later_window) = (crop(&own), crop(later));
        let moved = |mut pieces: Vec<(u8, Piece)>| {
            for (_, piece) in &mut pieces {
                for rect in &mut piece.rects {
                    rect.0 += left;
                    rect.1 += top;
                }
            }
            pieces
        };
        let whole = self.lifetime_pieces(
            own_window
                .iter()
                .zip(&later_window)
                .map(|(own, later)| self.needs(own, later, self.opaque(colour)))
                .collect(),
            self.opaque(colour),
        )?;
        let whole: Vec<(u8, Piece)> = moved(whole.into_iter().map(|piece| (1, piece)).collect());
        if !self.lifetimes || !self.opaque(colour) {
            return Ok(whole);
        }
        let mut best = (cost(&whole), whole);
        if self.split {
            for patch in [false, true] {
                if let Ok(split) = self.split_pieces(&own_window, &later_window, patch)
                    && split_cost(&split) < best.0
                {
                    best = (split_cost(&split), moved(split));
                }
            }
        }
        if tiles
            && let Ok(tiled) = self.tiled_pieces(&own, later)
            && tiled_cost(&tiled) < best.0
        {
            best = (tiled_cost(&tiled), tiled);
        }
        Ok(best.1)
    }

    /// A colour cut into square tiles of the canvas, each with lifetimes of
    /// its own, so a change in one tile leaves the others' pieces alone.
    /// Tiles are drawn in reading order, and a tile also covers the cells
    /// of its colour in later tiles next to its own: the seam invariant
    /// between two tiles, which shows every cell in its own tile's group.
    fn tiled_pieces(&self, own: &[Mask], later: &[Mask]) -> Result<Vec<(u8, Piece)>, EncodeError> {
        let (width, height) = (self.anim.width(), self.anim.height());
        let size = TILE;
        let mut any = self.empty();
        own.iter().for_each(|mask| any.union(mask));
        let Some((x0, y0, w, h)) = any.bounds() else { return Ok(Vec::new()) };
        if w <= size && h <= size {
            return Err(EncodeError::TooManyRects { rects: 0 });
        }
        let mut pieces = Vec::new();
        let (columns, rows) = (width.div_ceil(size), height.div_ceil(size));
        for tile in 0..columns * rows {
            let (left, top) = ((tile % columns) * size, (tile / columns) * size);
            let (right, bottom) = ((left + size).min(width), (top + size).min(height));
            if right <= x0 || left >= x0 + w || bottom <= y0 || top >= y0 + h {
                continue;
            }
            // the tile and a cell around it is all its pieces can touch, so
            // they are worked out in a window of that
            let (wl, wt) = (left.saturating_sub(1), top.saturating_sub(1));
            let (ww, wh) = ((right + 1).min(width) - wl, (bottom + 1).min(height) - wt);
            let mut region = Mask::new(ww, wh);
            let mut after = Mask::new(ww, wh);
            for y in 0..wh {
                for x in 0..ww {
                    let (cx, cy) = (wl + x, wt + y);
                    if (left..right).contains(&cx) && (top..bottom).contains(&cy) {
                        region.set(x, y);
                    } else if cy >= bottom || (cy >= top && cx >= right) {
                        after.set(x, y);
                    }
                }
            }
            let needs: Vec<(Option<Mask>, Mask)> = own
                .iter()
                .zip(later)
                .map(|(own, later)| {
                    let own = own.crop(wl, wt, ww, wh);
                    let later = later.crop(wl, wt, ww, wh);
                    let mut mine = own.clone();
                    mine.intersect(&region);
                    let mut theirs = own;
                    theirs.intersect(&after);
                    let mut may = mine.clone();
                    may.union(&later);
                    may.union(&theirs);
                    if mine.is_empty() {
                        return (None, may);
                    }
                    let mut reach = later;
                    reach.union(&theirs);
                    reach.intersect(&mine.grown());
                    let mut must = mine;
                    must.union(&reach);
                    (Some(must), may)
                })
                .collect();
            for mut piece in self.lifetime_pieces(needs, true)? {
                piece.tile = tile + 1;
                for rect in &mut piece.rects {
                    rect.0 += wl;
                    rect.1 += wt;
                }
                pieces.push((1, piece));
            }
        }
        Ok(pieces)
    }

    /// The cores of a colour: for each stretch of frames where most of its
    /// cells keep it, those cells, drawn once for the stretch. `later[frame]`
    /// holds the cells of the opaque groups drawn above the cores.
    ///
    /// A core must have its colour painted under every later colour next
    /// to it. Without `patch`, it does that itself, so it can only keep
    /// cells whose neighbours are its own or always hidden; with `patch`,
    /// the deltas below it do that where the neighbour isn't always hidden.
    fn cores(&self, own: &[Mask], later: &[Mask], patch: bool) -> Result<Cores, EncodeError> {
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
            let reached = |(x, y): (u32, u32)| span.clone().any(|f| later[f].get(x, y));
            let unsure = |core: &Mask| -> Vec<(u32, u32)> {
                core.cells()
                    .filter(|&cell| {
                        core_neighbours(cell, core)
                            .any(|n| reached(n) && !core.get(n.0, n.1) && !hidden.get(n.0, n.1))
                    })
                    .collect()
            };
            if !patch {
                loop {
                    let cells = unsure(&core);
                    if cells.is_empty() {
                        break;
                    }
                    cells.into_iter().for_each(|(x, y)| core.clear(x, y));
                }
            }
            if core.is_empty() {
                at.extend(span.map(|_| None));
                continue;
            }
            let mut must = core.clone();
            for f in span.clone() {
                let mut reach = core.grown();
                reach.intersect(if patch { &hidden } else { &later[f] });
                must.union(&reach);
            }
            let mut may = core.clone();
            may.union(&hidden);
            for rects in covers(&must, &may, true)? {
                pieces.push(Piece { from: span.start, to: span.end, rects, tile: 0 });
            }
            at.extend(span.map(|_| Some((core.clone(), hidden.clone()))));
        }
        Ok((pieces, at))
    }

    /// A colour as cores (see [`Painter::cores`]), and below them a delta
    /// with the rest of each frame.
    ///
    /// A delta lies directly under its core, so where they meet the delta
    /// must reach under the core's cells (the seam invariant between two
    /// groups), which is fine: the core paints the same colour over it.
    fn split_pieces(
        &self,
        own: &[Mask],
        later: &[Mask],
        patch: bool,
    ) -> Result<Vec<(u8, Piece)>, EncodeError> {
        let (cores, at) = self.cores(own, later, patch)?;
        let deltas = own
            .iter()
            .zip(later)
            .zip(at)
            .map(|((own, later), core)| {
                let Some((core, hidden)) = core else {
                    return self.needs(own, later, true);
                };
                // the cells the core doesn't draw; the delta must also reach
                // under the core's where they meet
                let mut rest = own.clone();
                rest.subtract(&core);
                let mut above = later.clone();
                above.union(&core);
                let (mut must, mut may) = self.needs(&rest, &above, true);
                may.union(own);
                // and paint the colour under later colours next to the core
                // where the core doesn't, over the core's cells beside them
                // too, so one group spans each such edge
                let mut under = core.grown();
                under.intersect(later);
                under.subtract(&hidden);
                if patch && !under.is_empty() {
                    let mut beside = under.grown();
                    beside.intersect(&core);
                    under.union(&beside);
                    let must = must.get_or_insert_with(|| Mask::new(own.width(), own.height()));
                    must.union(&under);
                }
                (must, may)
            })
            .collect();
        let mut out: Vec<(u8, Piece)> = cores.into_iter().map(|piece| (1, piece)).collect();
        out.extend(self.lifetime_pieces(deltas, true)?.into_iter().map(|piece| (0, piece)));
        Ok(out)
    }

    fn cost_of(&self, colour: u16, later: &[Mask]) -> usize {
        self.pieces(colour, later, false).map_or(IMPOSSIBLE, |pieces| cost(&pieces))
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
        // a colour's cost only depends on the colours over it within a cell
        // of its own, so after each step only the costs of colours next to
        // the one taken change
        let union = |colour: u16| {
            let mut cells = self.empty();
            (0..self.frames()).for_each(|f| cells.union(&self.own(colour, f)));
            cells
        };
        let cells: Vec<Mask> = colours.iter().map(|&colour| union(colour)).collect();
        let around: Vec<Mask> = cells.iter().map(Mask::grown).collect();
        let mut remaining: Vec<usize> = (0..colours.len()).collect();
        let mut later = vec![self.empty(); self.frames()];
        let mut costs: Vec<Option<usize>> = vec![None; colours.len()];
        let mut top_down = Vec::with_capacity(colours.len());
        while !remaining.is_empty() {
            for &i in &remaining {
                if costs[i].is_none() {
                    costs[i] = Some(self.cost_of(colours[i], &later));
                }
            }
            let (position, &index) = remaining
                .iter()
                .enumerate()
                .min_by_key(|&(_, &i)| (costs[i].unwrap() as i128 - at_bottom[i] as i128, i))
                .unwrap();
            remaining.remove(position);
            let colour = colours[index];
            top_down.push(colour);
            if self.opaque(colour) {
                for (f, mask) in later.iter_mut().enumerate() {
                    mask.union(&self.own(colour, f));
                }
                for &i in &remaining {
                    if around[i].intersects(&cells[index]) {
                        costs[i] = None;
                    }
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
            layers: stack(pieces, &starts, self.reuse)?,
        })
    }

    /// Every colour's pieces in `order`, each split colour's delta just
    /// below its cores, sorted by drawing order.
    fn ordered_pieces(&self, order: &[u16]) -> Result<Vec<(usize, u16, Piece)>, EncodeError> {
        // from the top colour down, so `later` grows as colours are done
        let mut later = vec![self.empty(); self.frames()];
        let mut pieces = Vec::new();
        for (rank, &colour) in order.iter().enumerate().rev() {
            for (place, piece) in self.pieces(colour, &later, true)? {
                pieces.push((rank * 2 + usize::from(place), colour, piece));
            }
            if self.opaque(colour) {
                for (f, mask) in later.iter_mut().enumerate() {
                    mask.union(&self.own(colour, f));
                }
            }
        }
        pieces.sort_by_key(|(rank, _, piece)| (*rank, piece.tile, piece.from));
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
    let mut painter = Painter {
        anim,
        cells,
        lifetimes: settings.lifetimes,
        split: settings.split,
        reuse: settings.reuse,
    };
    if settings.lifetimes {
        match painter.search(settings.effort, score) {
            // a layer per frame stays within the limit for up to 180
            // frames, and a colour's rectangles in one frame are fewer
            // than over a merged lifetime
            Err(EncodeError::TooManyLayers { .. } | EncodeError::TooManyRects { .. }) => {
                painter.lifetimes = false
            }
            result => return painter.without_reuse_if_smaller(result, score),
        }
    }
    let result = painter.search(settings.effort, score);
    painter.without_reuse_if_smaller(result, score)
}

impl Painter<'_> {
    /// The scene `result` holds, or that order stacked without reuse when
    /// `score` finds that smaller: reused pieces can't share layers with
    /// others, which sometimes costs more than drawing them again.
    fn without_reuse_if_smaller(
        &mut self,
        result: Result<(Vec<u16>, Scene), EncodeError>,
        score: Option<&dyn Fn(&Scene) -> usize>,
    ) -> Result<Scene, EncodeError> {
        let (order, scene) = result?;
        let Some(score) = score.filter(|_| self.reuse) else { return Ok(scene) };
        self.reuse = false;
        let plain = self.encode(&order);
        self.reuse = true;
        match plain {
            Ok(plain) if score(&plain) < score(&scene) => Ok(plain),
            _ => Ok(scene),
        }
    }

    /// The drawing order `effort` finds, and its scene.
    fn search(
        &self,
        effort: Effort,
        score: Option<&dyn Fn(&Scene) -> usize>,
    ) -> Result<(Vec<u16>, Scene), EncodeError> {
        let guess = self.guessed_order();
        // searching the order costs about one encode per colour or more
        let cells = u64::from(self.anim.width()) * u64::from(self.anim.height());
        let heavy = guess.len() as u64 * cells * self.frames() as u64 > SEARCH_WORK;
        let effort = if heavy { Effort::Fast } else { effort };
        let searched = || match guess.len() {
            n if n <= EXACT_ORDER => self.exact_order(&guess),
            n if n <= GREEDY_ORDER => self.greedy_order(&guess),
            _ => guess.clone(),
        };
        let encoded = |order: Vec<u16>| self.encode(&order).map(|scene| (order, scene));
        let score = match (effort, score) {
            (Effort::Fast, _) => return encoded(guess.clone()),
            (Effort::Balanced, _) | (Effort::Best, None) => return encoded(searched()),
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
            return encoded(order);
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
        Ok((order, scene))
    }
}

/// Rectangles covering `must` within `may` (see [`cover`]), in groups a
/// layer can take: when there are too many, the cells are split into two
/// bands of rows that share their middle row, recursively. The lower band
/// covers the shared row too, which keeps the seam invariant between the
/// two, and painting an opaque colour over itself changes nothing. A
/// translucent colour would blend with itself there, so it can't be split.
fn covers(must: &Mask, may: &Mask, opaque: bool) -> Result<Vec<Vec<Rect>>, EncodeError> {
    let rects = cover(must, may);
    if rects.len() <= MAX_GROUP_RECTS {
        return Ok(vec![rects]);
    }
    let Some((_, top, _, height)) = must.bounds() else { return Ok(Vec::new()) };
    if !opaque || height < 3 {
        return Err(EncodeError::TooManyRects { rects: rects.len() });
    }
    let (middle, bottom) = (top + height / 2, top + height);
    let mut groups = covers(&must.rows(top, middle + 1), &may.rows(top, middle + 1), opaque)?;
    groups.extend(covers(&must.rows(middle, bottom), &may.rows(middle, bottom), opaque)?);
    Ok(groups)
}

/// Spans of frames, `from..to`, in order.
type Spans = Vec<(usize, usize)>;

/// A colour's core pieces, and each frame's core cells with the cells
/// hidden under later colours over its stretch (`None` without a core).
type Cores = (Vec<Piece>, Vec<Option<(Mask, Mask)>>);

/// Puts pieces, in drawing order, into layers: a piece joins the highest
/// layer with the same lifetime when no layer above that one is shown at
/// the same time and the per-layer limits allow; otherwise it starts a
/// layer on top. With `reuse`, pieces of one colour and place with the same
/// rectangles become one, shown over all their lifetimes.
fn stack(
    pieces: Vec<(usize, u16, Piece)>,
    starts: &[u32],
    reuse: bool,
) -> Result<Vec<Layer>, EncodeError> {
    // tlottie counts every group, rectangle, fill and group transform as a
    // shape, and every rectangle a fill paints as a paint source
    let fits = |groups: usize, rects: usize| {
        rects <= TLOTTIE.max_paint_source_items_per_layer
            && groups <= TLOTTIE.max_paints_per_layer
            && rects + 3 * groups <= MAX_SHAPES
    };
    // pieces with every span of frames they are shown for, in drawing order
    let mut shown: Vec<(u16, Vec<Rect>, Spans)> = Vec::new();
    let mut same: std::collections::HashMap<(usize, u32, u16, Vec<Rect>), usize> =
        std::collections::HashMap::new();
    for (rank, colour, piece) in pieces {
        let span = (piece.from, piece.to);
        let key = (rank, piece.tile, colour, piece.rects.clone());
        if reuse && let Some(&index) = same.get(&key) {
            let spans = &mut shown[index].2;
            match spans.last_mut() {
                Some(last) if last.1 == span.0 => last.1 = span.1,
                _ => spans.push(span),
            }
            continue;
        }
        if reuse {
            same.insert(key, shown.len());
        }
        shown.push((colour, piece.rects, vec![span]));
    }
    let overlap = |a: &[(usize, usize)], b: &[(usize, usize)]| {
        a.iter().any(|&(from, to)| {
            b.iter().any(|&(other_from, other_to)| from < other_to && other_from < to)
        })
    };
    let group = |colour: u16, rects: &[Rect]| Group {
        colour,
        rule: FillRule::NonZero,
        shapes: rects
            .iter()
            .map(|&(x, y, width, height)| Shape::Rect { x, y, width, height })
            .collect(),
        shown: Vec::new(),
    };
    let ticks = |spans: &[(usize, usize)]| -> Vec<(u32, u32)> {
        spans.iter().map(|&(from, to)| (starts[from], starts[to])).collect()
    };
    // per layer: spans, groups and rectangles so far
    let mut layers: Vec<(Spans, Vec<Group>, usize)> = Vec::new();
    for (colour, rects, spans) in &shown {
        let mut target = None;
        for (index, (layer_spans, groups, count)) in layers.iter().enumerate().rev() {
            if layer_spans == spans {
                if fits(groups.len() + 1, count + rects.len()) {
                    target = Some(index);
                }
                break;
            }
            if overlap(layer_spans, spans) {
                break;
            }
        }
        match target {
            Some(index) => {
                layers[index].1.push(group(*colour, rects));
                layers[index].2 += rects.len();
            }
            None => layers.push((spans.clone(), vec![group(*colour, rects)], rects.len())),
        }
    }
    if layers.len() <= MAX_LAYERS {
        return Ok(layers
            .into_iter()
            .map(|(spans, groups, _)| {
                let shown = ticks(&spans);
                Layer {
                    from: shown[0].0,
                    to: shown[shown.len() - 1].1,
                    hidden: shown.windows(2).map(|pair| (pair[0].1, pair[1].0)).collect(),
                    groups,
                }
            })
            .collect());
    }
    // A layer for each lifetime is too many: fill layers in drawing order
    // instead, each group shown over its own spans.
    let mut layers: Vec<(Vec<Group>, usize)> = Vec::new();
    for (colour, rects, spans) in &shown {
        let mut group = group(*colour, rects);
        group.shown = ticks(spans);
        match layers.last_mut() {
            Some((groups, count)) if fits(groups.len() + 1, *count + rects.len()) => {
                groups.push(group);
                *count += rects.len();
            }
            _ => layers.push((vec![group], rects.len())),
        }
    }
    if layers.len() > MAX_LAYERS {
        return Err(EncodeError::TooManyLayers { layers: layers.len() });
    }
    Ok(layers
        .into_iter()
        .map(|(mut groups, _)| {
            let from = groups.iter().map(|group| group.shown[0].0).min().unwrap();
            let to = groups.iter().map(|group| group.shown[group.shown.len() - 1].1).max().unwrap();
            // groups shown whenever their layer is need no keyframes
            for group in &mut groups {
                if group.shown == [(from, to)] {
                    group.shown.clear();
                }
            }
            Layer { from, to, hidden: Vec::new(), groups }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tgradish_frames::{Animation, Frame};

    use super::*;
    use crate::normalise::{Options, normalise};

    #[test]
    fn splits_colours_too_big_for_a_layer() {
        // isolated pixels swapping two colours: kept for both frames, a
        // colour covers every one of them, more than a layer takes, so it
        // is drawn in bands
        let size = 129u32;
        let frames = (0..2)
            .map(|frame| {
                let rgba = (0..size * size)
                    .flat_map(|index| {
                        let (x, y) = (index % size, index / size);
                        match (x % 2, y % 2, (x / 2 + y / 2 + frame) % 2) {
                            (0, 0, 0) => [230, 40, 40, 255],
                            (0, 0, _) => [40, 40, 230, 255],
                            _ => [0; 4],
                        }
                    })
                    .collect();
                Frame { rgba, duration: Duration::from_millis(100) }
            })
            .collect();
        let input = Animation::new(size, size, frames).unwrap();
        let anim =
            normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap().0;
        let scene = painter(&anim, &Settings::default(), None).unwrap();
        assert_eq!(scene.compare(&anim), None);
        for layer in &scene.layers {
            let rects: usize = layer.groups.iter().map(|group| group.shapes.len()).sum();
            assert!(rects + 3 * layer.groups.len() <= MAX_SHAPES);
        }
    }

    #[test]
    fn draws_a_shape_that_comes_back_once() {
        // red and blue halves, then a green cell alone, then the halves
        // again: nothing can stay alive through the middle frame
        let frame = |cells: [[u8; 4]; 8]| Frame {
            rgba: cells.into_iter().flatten().collect(),
            duration: Duration::from_millis(100),
        };
        let (red, blue, green, clear) =
            ([230, 40, 40, 255], [40, 40, 230, 255], [40, 230, 40, 255], [0; 4]);
        let halves = [red, red, red, red, blue, blue, blue, blue];
        let alone = [green, clear, clear, clear, clear, clear, clear, clear];
        let input = Animation::new(8, 1, vec![frame(halves), frame(alone), frame(halves)]).unwrap();
        let anim =
            normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap().0;
        let groups = |settings: &Settings| {
            let scene = painter(&anim, settings, None).unwrap();
            assert_eq!(scene.compare(&anim), None);
            assert_eq!(scene.seams(anim.palette()), []);
            let hidden = scene.layers.iter().filter(|layer| !layer.hidden.is_empty()).count();
            (scene.layers.iter().map(|layer| layer.groups.len()).sum::<usize>(), hidden)
        };
        let plain = groups(&Settings { reuse: false, ..Settings::default() });
        let reused = groups(&Settings::default());
        assert_eq!(plain.1, 0);
        assert!(reused.0 < plain.0 && reused.1 > 0, "{plain:?} {reused:?}");
    }

    #[test]
    fn fills_layers_in_order_when_lifetimes_need_too_many() {
        // every piece overlaps the one before in time but not in lifetime,
        // so each would need a layer of its own
        let count = MAX_LAYERS + 100;
        let pieces = (0..count)
            .map(|i| {
                let from = i % 2;
                let piece = Piece { from, to: from + 2, rects: vec![(i as u32, 0, 1, 1)], tile: 0 };
                (i, (i % 2) as u16 + 1, piece)
            })
            .collect();
        let starts = [0, 10, 20, 30];
        let layers = stack(pieces, &starts, false).unwrap();
        assert!(layers.len() < 5, "{}", layers.len());
        // drawing order is kept, and every group is shown when its piece was
        let groups: Vec<&Group> = layers.iter().flat_map(|layer| &layer.groups).collect();
        assert_eq!(groups.len(), count);
        for (i, group) in groups.iter().enumerate() {
            assert_eq!(group.shapes, [Shape::Rect { x: i as u32, y: 0, width: 1, height: 1 }]);
            let (from, to) = (starts[i % 2], starts[i % 2 + 2]);
            let layer = layers.iter().find(|layer| layer.groups.contains(group)).unwrap();
            let shown = if group.shown.is_empty() {
                vec![(layer.from, layer.to)]
            } else {
                group.shown.clone()
            };
            assert_eq!(shown, [(from, to)]);
        }
    }

    #[test]
    fn keeps_a_track_whole_under_a_rolling_ball() {
        // a black track, and a blue 2x2 ball rolling along on top of it
        let (width, height) = (24u32, 4u32);
        let frames = (0..6)
            .map(|frame| {
                let rgba = (0..width * height)
                    .flat_map(|index| {
                        let (x, y) = (index % width, index / width);
                        match (x, y) {
                            _ if (frame * 3..frame * 3 + 2).contains(&x) && (1..3).contains(&y) => {
                                [40, 40, 230, 255]
                            }
                            (_, 3) => [10, 10, 10, 255],
                            _ => [0; 4],
                        }
                    })
                    .collect();
                Frame { rgba, duration: Duration::from_millis(100) }
            })
            .collect();
        let input = Animation::new(width, height, frames).unwrap();
        let anim =
            normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap().0;
        let scene = painter(&anim, &Settings::default(), None).unwrap();
        assert_eq!(scene.compare(&anim), None);
        assert_eq!(scene.seams(anim.palette()), []);
        // the track lives through every frame
        let black = anim.palette().iter().position(|&c| c == [10, 10, 10, 255]).unwrap() as u16;
        let whole = |layer: &Layer, group: &Group| {
            group.colour == black
                && group.shown.is_empty()
                && layer.hidden.is_empty()
                && (layer.from, layer.to) == (0, scene.ticks)
        };
        assert!(
            scene.layers.iter().any(|layer| layer.groups.iter().any(|group| whole(layer, group)))
        );
    }

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

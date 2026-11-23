//! Colour drawing-order search: heuristic ordering, exact subset DP for small
//! palettes, greedy ordering for medium palettes, and compressed-size hill
//! climbing under [`Effort::Best`].

use super::{Effort, EncodeError, IMPOSSIBLE, Mask, Painter, Scene, cost};

/// Maximum colour count for exact subset DP search.
const EXACT_ORDER: usize = 10;
/// Maximum `colours * cells * frames` work budget before falling back to
/// heuristic order regardless of effort, because searching would take minutes
/// (the corpus peaks at 11 million; `blue_ball_machine` hits 1.9 billion).
const SEARCH_WORK: u64 = 200_000_000;
/// Above this many colours, order search is skipped completely and the
/// heuristic order is used.
const GREEDY_ORDER: usize = 256;
/// Maximum full encode+compress trials under [`Effort::Best`] so animations
/// with many colours still finish in bounded time.
const BEST_TRIES: usize = 200;

impl Painter<'_> {
    /// The colours present, quickest guess first: larger bounding boxes,
    /// then more cells.
    pub(super) fn guessed_order(&self) -> Vec<u16> {
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
            // any order will do: encoding it stops at once
            if (self.cancelled)() {
                return colours.to_vec();
            }
            self.at(set as f64 / (1usize << n) as f64);
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
        self.greedy(colours, true)
    }

    /// [`Painter::greedy_order`], working out only the costs a step can
    /// change when `cached`. Encoding always caches; working out every
    /// cost again is for the test that checks caching changes nothing.
    pub(super) fn greedy(&self, colours: &[u16], cached: bool) -> Vec<u16> {
        let mut everything = vec![self.empty(); self.frames()];
        for &colour in colours.iter().filter(|&&c| self.opaque(c)) {
            for (f, mask) in everything.iter_mut().enumerate() {
                mask.union(&self.own(colour, f));
            }
        }
        // what each colour costs at the bottom, then each colour placed,
        // as half of the work each
        let steps = 2.0 * colours.len() as f64;
        // cells of different colours never overlap, so "every colour but
        // this one" is a subtraction
        let mut at_bottom = Vec::with_capacity(colours.len());
        for &colour in colours {
            // any order will do: encoding it stops at once
            if (self.cancelled)() {
                return colours.to_vec();
            }
            self.at(at_bottom.len() as f64 / steps);
            let later: Vec<Mask> = everything
                .iter()
                .enumerate()
                .map(|(f, mask)| {
                    let mut later = mask.clone();
                    later.subtract(&self.own(colour, f));
                    later
                })
                .collect();
            at_bottom.push(self.cost_of(colour, &later));
        }
        // a colour's pieces are worked out in a window of its cells and a
        // cell around them, where rectangles can reach over any later
        // colour: after each step only the costs of colours whose window
        // holds the one taken change
        let union = |colour: u16| {
            let mut cells = self.empty();
            (0..self.frames()).for_each(|f| cells.union(&self.own(colour, f)));
            cells
        };
        let cells: Vec<Mask> = colours.iter().map(|&colour| union(colour)).collect();
        let around: Vec<Mask> = cells
            .iter()
            .map(|cells| {
                let (width, height) = (self.anim.width(), self.anim.height());
                let Some((x, y, w, h)) = cells.bounds() else { return self.empty() };
                let (left, top) = (x.saturating_sub(1), y.saturating_sub(1));
                let (right, bottom) = ((x + w + 1).min(width), (y + h + 1).min(height));
                Mask::filled(width, height, (left, top, right - left, bottom - top))
            })
            .collect();
        let mut remaining: Vec<usize> = (0..colours.len()).collect();
        let mut later = vec![self.empty(); self.frames()];
        let mut costs: Vec<Option<usize>> = vec![None; colours.len()];
        let mut top_down = Vec::with_capacity(colours.len());
        while !remaining.is_empty() {
            self.at((colours.len() + top_down.len()) as f64 / steps);
            for &i in &remaining {
                if costs[i].is_none() {
                    if (self.cancelled)() {
                        return colours.to_vec();
                    }
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
                    if !cached || around[i].intersects(&cells[index]) {
                        costs[i] = None;
                    }
                }
            }
        }
        top_down.reverse();
        top_down
    }

    /// The scene `result` holds, or that order stacked without reuse when
    /// `score` finds that smaller: reused pieces can't share layers with
    /// others, which sometimes costs more than drawing them again.
    pub(super) fn without_reuse_if_smaller(
        &mut self,
        result: Result<(Vec<u16>, Scene), EncodeError>,
        score: Option<&dyn Fn(&Scene) -> usize>,
    ) -> Result<Scene, EncodeError> {
        let (order, scene) = result?;
        let Some(score) = score.filter(|_| self.reuse) else { return Ok(scene) };
        let (from, to) = self.plain.get();
        self.part(from, to);
        self.reuse = false;
        let plain = self.encode(&order);
        self.reuse = true;
        match plain {
            Err(EncodeError::Cancelled) => Err(EncodeError::Cancelled),
            Ok(plain) if score(&plain) < score(&scene) => Ok(plain),
            _ => Ok(scene),
        }
    }

    /// The drawing order `effort` finds, and its scene.
    pub(super) fn search(
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
        // how long each part takes, about, in encodes, as measured on the
        // reference art: searching the order exactly costs a quarter of
        // an encode per set of colours, greedily a quarter per colour;
        // then an encode of each order to start from, the neighbours
        // swapped in about three passes over the order, and one drawn
        // again without reuse
        let searching = match (effort, guess.len()) {
            (Effort::Fast, _) => 0.0,
            (_, n) if n <= EXACT_ORDER => (1usize << n) as f64 / 4.0,
            (_, n) if n <= GREEDY_ORDER => n as f64 / 4.0,
            _ => 0.0,
        };
        let improves = effort == Effort::Best && score.is_some();
        let (starts, swaps) = match improves {
            true => (2.0, (3 * guess.len().saturating_sub(1)).min(BEST_TRIES - 2) as f64),
            false => (1.0, 0.0),
        };
        let plain = if score.is_some() && self.reuse { 1.0 } else { 0.0 };
        let work = searching + starts + swaps + plain;
        let share = |encodes: f64| encodes / work;
        let (searched_by, started_by) = (share(searching), share(searching + starts));
        let swapped_by = share(searching + starts + swaps);
        self.plain.set((swapped_by, 1.0));
        self.part(0.0, searched_by);
        let searched = || {
            let order = searched();
            self.part(searched_by, started_by);
            order
        };
        let encoded = |order: Vec<u16>| self.encode(&order).map(|scene| (order, scene));
        let score = match (effort, score) {
            (Effort::Fast, _) => {
                self.part(searched_by, started_by);
                return encoded(guess.clone());
            }
            (Effort::Balanced, _) | (Effort::Best, None) => return encoded(searched()),
            (Effort::Best, Some(score)) => score,
        };
        // the better of both starting orders, then neighbours swapped while
        // that helps
        // an order that doesn't fit tlottie's limits is no candidate; a
        // stop ends the search
        let try_order = |order: &[u16]| match self.encode(order) {
            Ok(scene) => Ok(Some((score(&scene), scene))),
            Err(EncodeError::Cancelled) => Err(EncodeError::Cancelled),
            Err(_) => Ok(None),
        };
        let mut order = searched();
        let first = share(searching + 1.0);
        self.part(searched_by, first);
        let mut best = try_order(&order)?;
        self.part(first, started_by);
        if let Some(guessed) = try_order(&guess)?
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
        // how many passes over the order it takes isn't known until one
        // changes nothing: each pass goes over half of what is left of
        // swapping, and never behind the share of the most tries it may
        // take
        let pairs = order.len().saturating_sub(1).max(1);
        let swept = |pass: i32, pair: usize, tries: usize| {
            let passes = 1.0 - 0.5f64.powi(pass) * (1.0 - 0.5 * pair as f64 / pairs as f64);
            let tried = (tries - 2) as f64 / (BEST_TRIES - 2) as f64;
            started_by + (swapped_by - started_by) * passes.max(tried)
        };
        let mut pass = 0;
        while improved && tries < BEST_TRIES {
            improved = false;
            for i in 0..order.len().saturating_sub(1) {
                if tries >= BEST_TRIES {
                    break;
                }
                if (self.cancelled)() {
                    return Err(EncodeError::Cancelled);
                }
                self.part(swept(pass, i, tries), swept(pass, i + 1, tries + 1));
                tries += 1;
                order.swap(i, i + 1);
                match try_order(&order)? {
                    Some((size, candidate)) if size < smallest => {
                        (smallest, scene) = (size, candidate);
                        improved = true;
                    }
                    _ => order.swap(i, i + 1),
                }
            }
            pass += 1;
        }
        Ok((order, scene))
    }
}

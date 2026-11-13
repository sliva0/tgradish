//! The order colours are drawn in: guessed, searched exactly over
//! subsets for few colours, greedily for more, and with [`Effort::Best`]
//! improved by real packed sizes.

use super::{Effort, EncodeError, IMPOSSIBLE, Mask, Painter, Scene, cost};

/// Up to this many colours, the order is searched exactly.
const EXACT_ORDER: usize = 10;
/// Above this many colours times cells times frames, the order is only
/// guessed whatever the effort: searching it would take minutes. The
/// corpus comes to at most 11 million, `blue_ball_machine` to 1.9 billion.
const SEARCH_WORK: u64 = 200_000_000;
/// Above this many colours, the order isn't searched at all.
const GREEDY_ORDER: usize = 256;
/// Most encodings [`Effort::Best`] tries, each a whole encode scored by
/// packing it, so that art with many colours still ends.
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
        // cells of different colours never overlap, so "every colour but
        // this one" is a subtraction
        let mut at_bottom = Vec::with_capacity(colours.len());
        for &colour in colours {
            // any order will do: encoding it stops at once
            if (self.cancelled)() {
                return colours.to_vec();
            }
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
        let encoded = |order: Vec<u16>| self.encode(&order).map(|scene| (order, scene));
        let score = match (effort, score) {
            (Effort::Fast, _) => return encoded(guess.clone()),
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
        let mut best = try_order(&order)?;
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
        while improved && tries < BEST_TRIES {
            improved = false;
            for i in 0..order.len().saturating_sub(1) {
                if tries >= BEST_TRIES {
                    break;
                }
                if (self.cancelled)() {
                    return Err(EncodeError::Cancelled);
                }
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
        }
        Ok((order, scene))
    }
}

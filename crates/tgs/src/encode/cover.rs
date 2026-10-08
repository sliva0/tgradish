//! Covers a set of cells with few rectangles that stay inside a larger
//! allowed set: the cells a paint group must fill, and those it may.
//!
//! The rectangles never overlap. Renderers anti-alias a path by adding up
//! the coverage of its parts, so where two overlapping rectangles share an
//! outer edge, the pixels along it count twice and come out too strong.
//! Rectangles that only touch are fine: their shared edges cancel out.

use super::mask::Mask;

/// `x, y, width, height` in cells.
pub type Rect = (u32, u32, u32, u32);

/// Disjoint rectangles inside `allowed` whose union holds every cell of
/// `must` (which must be inside `allowed`), sorted by row, then column.
///
/// Greedy: the first uncovered cell, in reading order, seeds a rectangle
/// in the allowed cells no rectangle has taken yet; of the widest
/// rectangles for each height it can reach, the one covering the most
/// uncovered cells wins.
pub fn cover(must: &Mask, allowed: &Mask) -> Vec<Rect> {
    debug_assert!(must.is_subset(allowed));
    let Some((left, top, width, height)) = must.bounds() else { return Vec::new() };
    let (w, h) = (width as usize, height as usize);
    let local = |mask: &Mask| -> Vec<bool> {
        let mut cells = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                cells[y * w + x] = mask.get(left + x as u32, top + y as u32);
            }
        }
        cells
    };
    // allowed cells no rectangle has taken yet
    let mut free = local(allowed);
    let mut uncovered = local(must);
    // free cells from each cell downwards, without a gap
    let mut down = vec![0usize; w * h];
    let update_down = |down: &mut [usize],
                       free: &[bool],
                       columns: std::ops::RangeInclusive<usize>,
                       bottom: usize| {
        for y in (0..=bottom).rev() {
            for x in columns.clone() {
                down[y * w + x] = if free[y * w + x] {
                    1 + if y + 1 < h { down[(y + 1) * w + x] } else { 0 }
                } else {
                    0
                };
            }
        }
    };
    update_down(&mut down, &free, 0..=w - 1, h - 1);
    // uncovered cells before each column, per row
    let prefix_row = |row: &[bool]| -> Vec<u32> {
        let mut sums = Vec::with_capacity(w + 1);
        sums.push(0);
        for &cell in row {
            sums.push(sums.last().unwrap() + u32::from(cell));
        }
        sums
    };
    let mut prefix: Vec<Vec<u32>> = uncovered.chunks_exact(w).map(prefix_row).collect();

    let mut out = Vec::new();
    let mut seed = 0;
    while seed < w * h {
        if !uncovered[seed] {
            seed += 1;
            continue;
        }
        let (sx, sy) = (seed % w, seed / w);
        // (score, x0, x1, y1): columns x0..=x1, rows sy..=y1
        let mut best = (0, sx, sx, sy);
        let mut rows = usize::MAX;
        let mut x1 = sx;
        while x1 < w && free[sy * w + x1] {
            rows = rows.min(down[sy * w + x1]);
            // only the widest rectangle of each height can win
            let last = x1 + 1 == w || down[sy * w + x1 + 1] < rows;
            if last {
                let y1 = sy + rows - 1;
                let mut x0 = sx;
                while x0 > 0 && down[sy * w + x0 - 1] >= rows {
                    x0 -= 1;
                }
                let score: u32 = (sy..=y1).map(|y| prefix[y][x1 + 1] - prefix[y][x0]).sum();
                if score > best.0 {
                    best = (score, x0, x1, y1);
                }
            }
            x1 += 1;
        }
        let (_, x0, x1, y1) = best;
        for y in sy..=y1 {
            uncovered[y * w + x0..=y * w + x1].fill(false);
            free[y * w + x0..=y * w + x1].fill(false);
            prefix[y] = prefix_row(&uncovered[y * w..(y + 1) * w]);
        }
        update_down(&mut down, &free, x0..=x1, y1);
        out.push((left + x0 as u32, top + sy as u32, (x1 - x0 + 1) as u32, (y1 - sy + 1) as u32));
    }
    out.sort_unstable_by_key(|&(x, y, _, _)| (y, x));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(rows: &[&str]) -> Mask {
        let mut mask = Mask::new(rows[0].len() as u32, rows.len() as u32);
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    mask.set(x as u32, y as u32);
                }
            }
        }
        mask
    }

    fn check(must: &Mask, allowed: &Mask, rects: &[Rect]) {
        let mut covered = Mask::new(must.width(), must.height());
        for &(x, y, w, h) in rects {
            for cy in y..y + h {
                for cx in x..x + w {
                    assert!(allowed.get(cx, cy), "{rects:?} leaves the allowed cells at {cx},{cy}");
                    assert!(!covered.get(cx, cy), "{rects:?} overlap at {cx},{cy}");
                    covered.set(cx, cy);
                }
            }
        }
        assert!(must.is_subset(&covered), "{rects:?} misses cells");
    }

    #[test]
    fn covers_with_few_rectangles() {
        let ring = mask(&["#####", "#...#", "#...#", "#####"]);
        let rects = cover(&ring, &ring);
        check(&ring, &ring, &rects);
        assert_eq!(rects.len(), 4);
        // when the hole may be filled, one rectangle does
        let full = mask(&["#####", "#####", "#####", "#####"]);
        assert_eq!(cover(&ring, &full), [(0, 0, 5, 4)]);

        // without overlaps, a plus takes three
        let plus = mask(&[".#.", "###", ".#."]);
        let rects = cover(&plus, &plus);
        check(&plus, &plus, &rects);
        assert_eq!(rects.len(), 3);

        // an L reaching left below its seed
        let l = mask(&["..#", "..#", "###"]);
        let rects = cover(&l, &l);
        check(&l, &l, &rects);
        assert_eq!(rects.len(), 2);
        assert!(cover(&Mask::new(3, 3), &l).is_empty());
    }

    #[test]
    fn covers_random_shapes() {
        // a fixed pseudo-random sequence
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..200 {
            let (w, h) = (1 + next() % 20, 1 + next() % 20);
            let mut must = Mask::new(w as u32, h as u32);
            let mut allowed = Mask::new(w as u32, h as u32);
            for y in 0..h as u32 {
                for x in 0..w as u32 {
                    match next() % 4 {
                        0 => must.set(x, y),
                        1 => allowed.set(x, y),
                        _ => {}
                    }
                }
            }
            allowed.union(&must);
            check(&must, &allowed, &cover(&must, &allowed));
        }
    }
}

//! The grids of cells art is read on: how colour edges fall, the grid
//! they make, and moving pixels onto a grid of a given scale.

/// Where cells start, in input pixels from the top left of the crop. Made
/// by normalising, so there is always a cell at least.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub(crate) columns: Vec<u32>,
    pub(crate) rows: Vec<u32>,
}

impl Grid {
    /// Where each column of cells starts, from 0, and then the crop's
    /// width: one more entry than there are columns.
    pub fn columns(&self) -> &[u32] {
        &self.columns
    }

    /// Where each row of cells starts, from 0, and then the crop's height:
    /// one more entry than there are rows.
    pub fn rows(&self) -> &[u32] {
        &self.rows
    }

    /// The largest size every cell is a multiple of: the art's pixel scale
    /// when the input is an exact upscale. Along a side of three cells or
    /// more, those at its ends don't count, since the canvas or a crop can
    /// cut art pixels there.
    pub fn scale(&self) -> u32 {
        let sizes = |edges: &[u32]| {
            let sizes: Vec<u32> = edges.windows(2).map(|w| w[1] - w[0]).collect();
            match sizes.len() {
                0..=2 => sizes,
                n => sizes[1..n - 1].to_vec(),
            }
        };
        sizes(&self.columns).into_iter().chain(sizes(&self.rows)).fold(0, gcd)
    }

    /// Where the art's pixel grid starts across and down: how far into the
    /// first cell the second begins, modulo the scale. Not 0 where the
    /// canvas or a crop cut the first art pixels.
    pub fn origin(&self) -> (u32, u32) {
        let unit = self.scale().max(1);
        let origin = |edges: &[u32]| if edges.len() > 2 { edges[1] % unit } else { 0 };
        (origin(&self.columns), origin(&self.rows))
    }

    /// Whether the canvas or a crop cut art pixels at the sides, so the
    /// cells there are smaller than the rest.
    pub fn cut_at_sides(&self) -> bool {
        let unit = self.scale().max(1);
        let cut = |edges: &[u32]| match edges {
            [first, second, .., before_last, last] if edges.len() > 2 => {
                !(second - first).is_multiple_of(unit) || !(last - before_last).is_multiple_of(unit)
            }
            _ => false,
        };
        cut(&self.columns) || cut(&self.rows)
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Colour changes between neighbouring pixels, over every row (or
/// column) of every frame: `columns[x]` counts changes between columns
/// `x - 1` and `x`. Outside the crop counts as transparent, so the first
/// and last entries count visible pixels on the border.
pub(crate) struct Edges {
    columns: Vec<u64>,
    rows: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridFit {
    /// Where the grid starts, from the crop's top left.
    pub(crate) offset: (u32, u32),
    /// Share of colour edges on the grid.
    pub(crate) fit: f64,
}

impl Edges {
    pub(crate) fn count(frames: &[Vec<u32>], width: u32, height: u32) -> Edges {
        let (w, h) = (width as usize, height as usize);
        let mut columns = vec![0; w + 1];
        let mut rows = vec![0; h + 1];
        let visible = |line: &[u32]| line.iter().filter(|&&pixel| pixel != 0).count() as u64;
        for pixels in frames {
            for (y, line) in pixels.chunks_exact(w).enumerate() {
                columns[0] += u64::from(line[0] != 0);
                columns[w] += u64::from(line[w - 1] != 0);
                for x in 1..w {
                    columns[x] += u64::from(line[x] != line[x - 1]);
                }
                if y > 0 {
                    let above = &pixels[(y - 1) * w..y * w];
                    rows[y] += line.iter().zip(above).filter(|(a, b)| a != b).count() as u64;
                }
            }
            rows[0] += visible(&pixels[..w]);
            rows[h] += visible(&pixels[(h - 1) * w..]);
        }
        Edges { columns, rows }
    }

    /// Columns and rows that differ from their neighbour in some frame
    /// start a new cell; the rest are copies and join the cell before them.
    pub(crate) fn grid(&self) -> Grid {
        let edges = |counts: &[u64]| {
            let size = counts.len() - 1;
            let mut edges: Vec<u32> =
                (0..size).filter(|&i| i == 0 || counts[i] > 0).map(|i| i as u32).collect();
            edges.push(size as u32);
            edges
        };
        Grid { columns: edges(&self.columns), rows: edges(&self.rows) }
    }

    /// How well colour edges line up with a grid of `scale`, which is at
    /// least 1, at its best offset. The `borders` of the crop count as edges where they lie on
    /// the art's grid, as those of the visible pixels do; where a crop cut
    /// through art pixels they would mislead, and only edges inside count.
    pub(crate) fn fit(&self, scale: u32, borders: bool) -> GridFit {
        let skip = usize::from(!borders);
        let counted = |counts: &[u64]| counts.len() - 2 * skip;
        let best = |counts: &[u64]| {
            let mut on_grid = vec![0u64; scale as usize];
            for (at, count) in counts.iter().enumerate().skip(skip).take(counted(counts)) {
                on_grid[at % scale as usize] += count;
            }
            let (offset, &count) = on_grid
                .iter()
                .enumerate()
                .max_by_key(|&(offset, count)| (*count, std::cmp::Reverse(offset)))
                .unwrap();
            (offset as u32, count)
        };
        let ((x, on_columns), (y, on_rows)) = (best(&self.columns), best(&self.rows));
        let sum = |counts: &[u64]| counts.iter().skip(skip).take(counted(counts)).sum::<u64>();
        let total = sum(&self.columns) + sum(&self.rows);
        let fit = if total == 0 { 1.0 } else { (on_columns + on_rows) as f64 / total as f64 };
        GridFit { offset: (x, y), fit }
    }
}

/// Fills every cell of the grid with its most common colour (the one at
/// its centre on ties, then the largest). Returns how many pixels changed.
pub(crate) fn snap(
    pixels: &mut [u32],
    width: u32,
    height: u32,
    scale: u32,
    offset: (u32, u32),
) -> u64 {
    let (columns, rows) =
        (grid_starts(offset.0, width, scale), grid_starts(offset.1, height, scale));
    snap_to(pixels, width, &columns, &rows)
}

/// Edges of a grid of `scale`, which is at least 1, along a side of `size`
/// pixels, starting at `offset`: from 0 to the size, cells at the ends cut
/// short.
pub(crate) fn grid_starts(offset: u32, size: u32, scale: u32) -> Vec<u32> {
    let mut starts = vec![0];
    starts.extend((offset % scale..size).step_by(scale as usize).filter(|&start| start > 0));
    starts.push(size);
    starts
}

/// Fills every cell between `columns` and `rows` (edges in pixels, from 0
/// to the size) with its most common colour, the one at its centre on
/// ties. Returns how many pixels changed.
pub(crate) fn snap_to(pixels: &mut [u32], width: u32, columns: &[u32], rows: &[u32]) -> u64 {
    let w = width as usize;
    let mut changed = 0;
    let mut cell: Vec<u32> = Vec::new();
    for ys in rows.windows(2) {
        for xs in columns.windows(2) {
            let centre = pixels[(ys[0] + (ys[1] - ys[0]) / 2) as usize * w
                + (xs[0] + (xs[1] - xs[0]) / 2) as usize];
            cell.clear();
            for y in ys[0]..ys[1] {
                cell.extend_from_slice(
                    &pixels[y as usize * w + xs[0] as usize..y as usize * w + xs[1] as usize],
                );
            }
            // the most common colour; on ties the centre's, then the largest
            cell.sort_unstable();
            let colour = cell
                .chunk_by(|a, b| a == b)
                .map(|run| (run.len(), run[0] == centre, run[0]))
                .max()
                .unwrap()
                .2;
            for y in ys[0]..ys[1] {
                for pixel in
                    &mut pixels[y as usize * w + xs[0] as usize..y as usize * w + xs[1] as usize]
                {
                    if *pixel != colour {
                        *pixel = colour;
                        changed += 1;
                    }
                }
            }
        }
    }
    changed
}

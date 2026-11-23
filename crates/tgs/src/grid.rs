//! Pixel-grid detection and snapping: counting colour edges, building cell
//! grids, and snapping pixels onto a target grid scale.

/// Cell boundary positions in input pixels, measured from the top-left of the
/// crop. Produced by normalisation and always contains at least one cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub(crate) columns: Vec<u32>,
    pub(crate) rows: Vec<u32>,
}

impl Grid {
    /// Column start positions from `0` up to the crop width (`columns + 1`
    /// entries).
    pub fn columns(&self) -> &[u32] {
        &self.columns
    }

    /// Row start positions from `0` up to the crop height (`rows + 1` entries).
    pub fn rows(&self) -> &[u32] {
        &self.rows
    }

    /// Greatest common divisor of interior cell sizes: the art's pixel scale
    /// when the input is an exact upscale. For axes with three or more cells,
    /// the outer cells are excluded because the canvas or a crop can clip edge
    /// art pixels.
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

    /// Start offset `(x, y)` of the pixel grid modulo [`Grid::scale`]. Non-zero
    /// when the canvas or a crop clipped the leading art pixels.
    pub fn origin(&self) -> (u32, u32) {
        let unit = self.scale().max(1);
        let origin = |edges: &[u32]| if edges.len() > 2 { edges[1] % unit } else { 0 };
        (origin(&self.columns), origin(&self.rows))
    }

    /// Whether the canvas or a crop clipped the outer cells on either axis to
    /// a non-multiple of [`Grid::scale`].
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

pub(crate) fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Colour transitions between adjacent pixels across all frames:
/// `columns[x]` counts changes between columns `x - 1` and `x`. Pixels outside
/// the crop count as transparent, so border entries count visible edge pixels.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct Edges {
    columns: Vec<u64>,
    rows: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridFit {
    /// Grid origin offset from the crop's top-left corner.
    pub(crate) offset: (u32, u32),
    /// Fraction of colour edges lying on the grid.
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

    /// Equivalent to [`Edges::count`], computed directly from indexed cells
    /// without expanding to full pixel buffers. `columns` and `rows` are pixel
    /// boundaries from `0` to size; colour `0` is transparent and indices are
    /// distinct colours.
    pub(crate) fn of_cells<'a>(
        columns: &[u32],
        rows: &[u32],
        frames: impl IntoIterator<Item = &'a [u16]>,
    ) -> Edges {
        let (wide, high) = (columns.len() - 1, rows.len() - 1);
        let (w, h) = (*columns.last().unwrap() as usize, *rows.last().unwrap() as usize);
        let mut column_counts = vec![0u64; w + 1];
        let mut row_counts = vec![0u64; h + 1];
        let width = |cell: usize| u64::from(columns[cell + 1] - columns[cell]);
        // the cells of a line that are visible, or that differ from another
        // line's, as the pixels they stand for
        let visible = |line: &[u16]| {
            line.iter()
                .enumerate()
                .filter(|&(_, &cell)| cell != 0)
                .map(|(x, _)| width(x))
                .sum::<u64>()
        };
        for cells in frames {
            for (y, line) in cells.chunks_exact(wide).enumerate() {
                let height = u64::from(rows[y + 1] - rows[y]);
                column_counts[0] += height * u64::from(line[0] != 0);
                column_counts[w] += height * u64::from(line[wide - 1] != 0);
                for x in 1..wide {
                    column_counts[columns[x] as usize] +=
                        height * u64::from(line[x] != line[x - 1]);
                }
                if y > 0 {
                    let above = &cells[(y - 1) * wide..y * wide];
                    let changed: u64 = (0..wide).filter(|&x| line[x] != above[x]).map(width).sum();
                    row_counts[rows[y] as usize] += changed;
                }
            }
            row_counts[0] += visible(&cells[..wide]);
            row_counts[h] += visible(&cells[(high - 1) * wide..]);
        }
        Edges { columns: column_counts, rows: row_counts }
    }

    /// Starts a new cell wherever a column or row differs from its neighbour
    /// in any frame; identical consecutive lines merge into the previous cell.
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

    /// Measures how well colour edges align with a grid of `scale` (>= 1) at
    /// its best offset. Include `borders` when crop edges lie on the art's
    /// grid; exclude them when a crop may have cut through art pixels.
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

/// Fills each grid cell with its most common colour (breaking ties by the
/// centre pixel, then highest packed value). Returns the changed pixel count.
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

/// Grid cell boundaries of spacing `scale` (>= 1) across `size` pixels,
/// starting at `offset` (from `0` to `size`, with partial edge cells).
pub(crate) fn grid_starts(offset: u32, size: u32, scale: u32) -> Vec<u32> {
    let mut starts = vec![0];
    starts.extend((offset % scale..size).step_by(scale as usize).filter(|&start| start > 0));
    starts.push(size);
    starts
}

/// Fills each cell defined by `columns` and `rows` with its most common colour
/// (breaking ties by centre pixel, then highest packed value). Returns the
/// changed pixel count.
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

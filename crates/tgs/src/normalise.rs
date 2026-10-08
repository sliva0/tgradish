//! Turns decoded frames into what the encoder works on: palette-indexed
//! frames on Telegram's 60 fps grid, cropped and reduced to the art's own
//! pixels.
//!
//! Everything here is lossless except what the options ask for: a time
//! window, trimming or speeding up long inputs, and a forced pixel scale.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tgradish_frames::Animation;

use crate::limits::telegram::{FPS, MAX_FRAMES};
use crate::{Error, Result};

const NANOS: u64 = 1_000_000_000;
/// Longest stretch Telegram allows, in nanoseconds.
const MAX_NANOS: u64 = MAX_FRAMES as u64 * NANOS / FPS as u64;
/// Largest pixel scale looked for when the art isn't exactly on a grid.
const MAX_LIKELY_SCALE: u32 = 16;
/// Share of colour edges that must lie on a scale's grid to suggest it.
const LIKELY_FIT: f64 = 0.8;
/// More colours than this and the input probably isn't pixel art.
const MANY_COLOURS: usize = 256;

/// What to do with inputs longer than Telegram's 3 seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Long {
    /// Play everything faster.
    #[default]
    SpeedUp,
    /// Keep the first 3 seconds.
    Trim,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Keep the input's canvas instead of cropping to the visible pixels.
    pub keep_canvas: bool,
    /// Size of one art pixel in input pixels. Detected when not given;
    /// forcing one moves pixels that are off its grid onto it (lossy).
    pub pixel_scale: Option<u32>,
    /// Where in the input to start.
    pub start: Duration,
    /// How much of the input to use, from `start`.
    pub length: Option<Duration>,
    pub long: Long,
}

/// One frame of palette indices, row by row, and how many 60 fps frames
/// it is shown for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelFrame {
    pub pixels: Vec<u16>,
    pub ticks: u32,
}

/// Where cells start, in input pixels from the top left of the crop. Holds
/// one more entry than there are cells: the last one is the crop's size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub columns: Vec<u32>,
    pub rows: Vec<u32>,
}

impl Grid {
    /// The largest size every cell is a multiple of: the art's pixel scale
    /// when the input is an exact upscale. Cells on the edges don't count
    /// when there are others, since the canvas can cut art pixels.
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
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A normalised animation. Cells are the largest blocks of input pixels
/// that are one colour in every frame, so the grid can be uneven; colour 0
/// is transparent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelAnim {
    width: u32,
    height: u32,
    grid: Grid,
    palette: Vec<[u8; 4]>,
    frames: Vec<PixelFrame>,
}

impl PixelAnim {
    /// Width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Straight RGBA colours; entry 0 is transparent.
    pub fn palette(&self) -> &[[u8; 4]] {
        &self.palette
    }

    pub fn frames(&self) -> &[PixelFrame] {
        &self.frames
    }

    /// Length in 60 fps frames.
    pub fn ticks(&self) -> u32 {
        self.frames.iter().map(|frame| frame.ticks).sum()
    }

    /// A frame as RGBA at the input's resolution, cropped.
    pub fn rgba(&self, frame: usize) -> Vec<u8> {
        let (columns, rows) = (&self.grid.columns, &self.grid.rows);
        let width = *columns.last().unwrap() as usize;
        let mut out = Vec::with_capacity(width * *rows.last().unwrap() as usize * 4);
        let pixels = &self.frames[frame].pixels;
        for (row, edges) in rows.windows(2).enumerate() {
            let mut line = Vec::with_capacity(width * 4);
            for (column, cell) in columns.windows(2).enumerate() {
                let colour = self.palette[pixels[row * self.width as usize + column] as usize];
                for _ in cell[0]..cell[1] {
                    line.extend(colour);
                }
            }
            for _ in edges[0]..edges[1] {
                out.extend(&line);
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// What normalising did, for the user and the protocol.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub input_width: u32,
    pub input_height: u32,
    pub input_frames: usize,
    /// Length of the input, in seconds.
    pub input_length: f64,
    /// The part of the input that is used, in input pixels.
    pub crop: Rect,
    /// Size in cells (see [`PixelAnim`]).
    pub width: u32,
    pub height: u32,
    /// The art's pixel scale: every cell is a multiple of it.
    pub scale: u32,
    /// A larger scale most of the art follows, with the share of colour
    /// edges on its grid; forcing it snaps the rest.
    pub likely_scale: Option<LikelyScale>,
    /// Input pixels a forced scale changed.
    pub snapped_pixels: u64,
    /// Opaque colours, counting each (colour, alpha) once.
    pub colours: usize,
    pub frames: usize,
    /// Consecutive identical frames joined into one.
    pub merged_frames: usize,
    /// Frames too short to get a whole 60 fps frame.
    pub dropped_frames: usize,
    /// Length in 60 fps frames.
    pub ticks: u32,
    /// How much faster than the input it plays (1 when not sped up).
    pub speed: f64,
    /// Whether a long input was cut at 3 seconds.
    pub trimmed: bool,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LikelyScale {
    pub scale: u32,
    pub fit: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum Warning {
    /// Hundreds of colours: probably not pixel art, so the sticker will be
    /// large.
    ManyColours { colours: usize },
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Packs a pixel into a `u32` with every fully transparent pixel as 0.
fn pack(rgba: &[u8]) -> Vec<u32> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|&pixel| if pixel[3] == 0 { 0 } else { u32::from_le_bytes(pixel) })
        .collect()
}

pub fn normalise(animation: &Animation, options: &Options) -> Result<(PixelAnim, Report)> {
    let input_length = nanos(animation.duration());
    let start = nanos(options.start);
    if start >= input_length {
        return Err(Error::StartPastEnd { start: options.start, length: animation.duration() });
    }
    let mut end = match options.length {
        Some(length) if length.is_zero() => return Err(Error::ZeroLength),
        Some(length) => start.saturating_add(nanos(length)).min(input_length),
        None => input_length,
    };
    let (mut speed, mut trimmed) = (1.0, false);
    if end - start > MAX_NANOS {
        match options.long {
            Long::SpeedUp => speed = (end - start) as f64 / MAX_NANOS as f64,
            Long::Trim => (end, trimmed) = (start + MAX_NANOS, true),
        }
    }

    // frames within the window, joined while identical
    let mut timed: Vec<(Vec<u32>, u64)> = Vec::new();
    let mut merged_frames = 0;
    let mut at = 0;
    for frame in animation.frames() {
        let (from, to) = (at, at + nanos(frame.duration));
        at = to;
        let shown = to.min(end).saturating_sub(from.max(start));
        if shown == 0 {
            continue;
        }
        let pixels = pack(&frame.rgba);
        match timed.last_mut() {
            Some((last, duration)) if *last == pixels => {
                *duration += shown;
                merged_frames += 1;
            }
            _ => timed.push((pixels, shown)),
        }
    }

    // whole 60 fps frames, rounding where each frame starts so the total
    // stays right
    let (numer, denom) = if speed > 1.0 {
        (u128::from(MAX_FRAMES), u128::from(end - start))
    } else {
        (u128::from(FPS), u128::from(NANOS))
    };
    let tick = |at: u64| ((u128::from(at) * numer + denom / 2) / denom) as u32;
    let mut elapsed = 0;
    let mut ticks: Vec<u32> = timed
        .iter()
        .map(|(_, duration)| {
            let from = tick(elapsed);
            elapsed += duration;
            tick(elapsed) - from
        })
        .collect();
    if ticks.iter().all(|&ticks| ticks == 0) {
        let longest = (0..timed.len()).max_by_key(|&index| timed[index].1).unwrap();
        ticks[longest] = 1;
    }
    let mut frames: Vec<(Vec<u32>, u32)> = Vec::new();
    let mut dropped_frames = 0;
    for ((pixels, _), ticks) in timed.into_iter().zip(ticks) {
        if ticks == 0 {
            dropped_frames += 1;
            continue;
        }
        match frames.last_mut() {
            Some((last, total)) if *last == pixels => {
                *total += ticks;
                merged_frames += 1;
            }
            _ => frames.push((pixels, ticks)),
        }
    }

    let (input_width, input_height) = (animation.width(), animation.height());
    let crop = if options.keep_canvas {
        Rect { x: 0, y: 0, width: input_width, height: input_height }
    } else {
        visible_area(&frames, input_width).ok_or(Error::Invisible)?
    };
    let mut pixels: Vec<Vec<u32>> =
        frames.iter().map(|(pixels, _)| cut(pixels, input_width, crop)).collect();
    let mut edges = Edges::count(&pixels, crop.width, crop.height);

    let mut snapped_pixels = 0;
    match options.pixel_scale {
        Some(0) => return Err(Error::ZeroScale),
        Some(scale @ 2..) => {
            // anything larger than the art makes it one cell either way
            let scale = scale.min(crop.width.max(crop.height));
            let offset = edges.fit(scale).offset;
            for frame in &mut pixels {
                snapped_pixels += snap(frame, crop.width, crop.height, scale, offset);
            }
            edges = Edges::count(&pixels, crop.width, crop.height);
        }
        _ => {}
    }

    let grid = edges.grid();
    let (width, height) = (grid.columns.len() as u32 - 1, grid.rows.len() as u32 - 1);
    let mut palette = vec![[0; 4]];
    let mut index = HashMap::from([(0u32, 0u16)]);
    let mut out = Vec::with_capacity(frames.len());
    for (pixels, (_, ticks)) in pixels.iter().zip(&frames) {
        let mut cells = Vec::with_capacity(width as usize * height as usize);
        for &y in &grid.rows[..height as usize] {
            for &x in &grid.columns[..width as usize] {
                let colour = pixels[(y * crop.width + x) as usize];
                let entry = match index.get(&colour) {
                    Some(&entry) => entry,
                    None => {
                        let entry =
                            u16::try_from(palette.len()).map_err(|_| Error::TooManyColours)?;
                        index.insert(colour, entry);
                        palette.push(colour.to_le_bytes());
                        entry
                    }
                };
                cells.push(entry);
            }
        }
        out.push(PixelFrame { pixels: cells, ticks: *ticks });
    }

    let scale = grid.scale();
    let likely_scale = if options.pixel_scale.is_some() {
        None
    } else {
        (scale + 1..=MAX_LIKELY_SCALE)
            .rev()
            .map(|candidate| (candidate, edges.fit(candidate).fit))
            .find(|&(_, fit)| fit >= LIKELY_FIT)
            .map(|(scale, fit)| LikelyScale { scale, fit })
    };
    let colours = palette.len() - 1;
    let mut warnings = Vec::new();
    if colours > MANY_COLOURS {
        warnings.push(Warning::ManyColours { colours });
    }
    let anim = PixelAnim { width, height, grid, palette, frames: out };
    let report = Report {
        input_width,
        input_height,
        input_frames: animation.frames().len(),
        input_length: input_length as f64 / NANOS as f64,
        crop,
        width,
        height,
        scale,
        likely_scale,
        snapped_pixels,
        colours,
        frames: anim.frames.len(),
        merged_frames,
        dropped_frames,
        ticks: anim.ticks(),
        speed,
        trimmed,
        warnings,
    };
    Ok((anim, report))
}

/// The smallest rectangle holding every visible pixel of every frame.
fn visible_area(frames: &[(Vec<u32>, u32)], width: u32) -> Option<Rect> {
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for (pixels, _) in frames {
        for (row, line) in pixels.chunks_exact(width as usize).enumerate() {
            let Some(first) = line.iter().position(|&pixel| pixel != 0) else { continue };
            let last = line.iter().rposition(|&pixel| pixel != 0).unwrap();
            left = left.min(first as u32);
            right = right.max(last as u32 + 1);
            top = top.min(row as u32);
            bottom = bottom.max(row as u32 + 1);
        }
    }
    (left < right).then(|| Rect { x: left, y: top, width: right - left, height: bottom - top })
}

fn cut(pixels: &[u32], width: u32, crop: Rect) -> Vec<u32> {
    let mut out = Vec::with_capacity(crop.width as usize * crop.height as usize);
    for y in crop.y..crop.y + crop.height {
        let start = (y * width + crop.x) as usize;
        out.extend_from_slice(&pixels[start..start + crop.width as usize]);
    }
    out
}

/// Colour changes between neighbouring pixels, over every row (or
/// column) of every frame: `columns[x]` counts changes between columns
/// `x - 1` and `x`. Outside the crop counts as transparent, so the first
/// and last entries count visible pixels on the border.
struct Edges {
    columns: Vec<u64>,
    rows: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GridFit {
    /// Where the grid starts, from the crop's top left.
    offset: (u32, u32),
    /// Share of colour edges on the grid.
    fit: f64,
}

impl Edges {
    fn count(frames: &[Vec<u32>], width: u32, height: u32) -> Edges {
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
    fn grid(&self) -> Grid {
        let edges = |counts: &[u64]| {
            let size = counts.len() - 1;
            let mut edges: Vec<u32> =
                (0..size).filter(|&i| i == 0 || counts[i] > 0).map(|i| i as u32).collect();
            edges.push(size as u32);
            edges
        };
        Grid { columns: edges(&self.columns), rows: edges(&self.rows) }
    }

    /// How well colour edges line up with a grid of `scale`, at its best
    /// offset.
    fn fit(&self, scale: u32) -> GridFit {
        let best = |counts: &[u64]| {
            let mut on_grid = vec![0u64; scale as usize];
            for (at, count) in counts.iter().enumerate() {
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
        let total: u64 = self.columns.iter().chain(&self.rows).sum();
        let fit = if total == 0 { 1.0 } else { (on_columns + on_rows) as f64 / total as f64 };
        GridFit { offset: (x, y), fit }
    }
}

/// Fills every cell of the grid with its most common colour (the one
/// nearest its centre on ties). Returns how many pixels changed.
fn snap(pixels: &mut [u32], width: u32, height: u32, scale: u32, offset: (u32, u32)) -> u64 {
    let starts = |offset: u32, size: u32| {
        let mut starts = vec![0];
        starts.extend((offset % scale..size).step_by(scale as usize).filter(|&start| start > 0));
        starts.push(size);
        starts
    };
    let (columns, rows) = (starts(offset.0, width), starts(offset.1, height));
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

#[cfg(test)]
mod tests {
    use tgradish_frames::Frame;

    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0; 4];

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// Frames of `width`x`height` pixels from a colour function.
    fn animation(
        width: u32,
        height: u32,
        frames: &[Duration],
        colour: impl Fn(usize, u32, u32) -> [u8; 4],
    ) -> Animation {
        let frames = frames
            .iter()
            .enumerate()
            .map(|(index, &duration)| {
                let mut rgba = Vec::new();
                for y in 0..height {
                    for x in 0..width {
                        rgba.extend(colour(index, x, y));
                    }
                }
                Frame { rgba, duration }
            })
            .collect();
        Animation::new(width, height, frames).unwrap()
    }

    /// One colour per frame, 1x1.
    fn flat(frames: &[([u8; 4], Duration)]) -> Animation {
        let durations: Vec<_> = frames.iter().map(|(_, duration)| *duration).collect();
        animation(1, 1, &durations, |index, _, _| frames[index].0)
    }

    /// Art pixel `(x, y)` of a busy pattern where neighbours differ.
    fn art(frame: usize, x: u32, y: u32) -> [u8; 4] {
        [RED, GREEN, BLUE, [9, 9, 9, 255]][(x as usize * 3 + y as usize + frame) % 4]
    }

    /// Which art pixel an input pixel shows, for art pixels starting at
    /// `edges`.
    fn art_index(edges: &[u32], at: u32) -> Option<u32> {
        let index = edges.iter().rposition(|&edge| edge <= at)?;
        (index + 1 < edges.len()).then_some(index as u32)
    }

    fn cropped(animation: &Animation, frame: usize, crop: Rect) -> Vec<u8> {
        let mut out = Vec::new();
        for y in crop.y..crop.y + crop.height {
            for x in crop.x..crop.x + crop.width {
                let pixel = animation.pixel(frame, x, y).unwrap();
                out.extend(if pixel[3] == 0 { CLEAR } else { pixel });
            }
        }
        out
    }

    fn assert_lossless(input: &Animation, anim: &PixelAnim, report: &Report) {
        assert_eq!(anim.frames().len(), input.frames().len());
        for frame in 0..input.frames().len() {
            assert_eq!(anim.rgba(frame), cropped(input, frame, report.crop), "frame {frame}");
        }
    }

    #[test]
    fn reduces_exact_upscales() {
        // 5x4 art at 3x, placed at (4, 5) on a 30x20 canvas
        let input = animation(30, 20, &[ms(100), ms(100)], |frame, x, y| {
            match (x.checked_sub(4).map(|x| x / 3), y.checked_sub(5).map(|y| y / 3)) {
                (Some(x @ 0..5), Some(y @ 0..4)) => art(frame, x, y),
                _ => CLEAR,
            }
        });
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert_eq!(report.crop, Rect { x: 4, y: 5, width: 15, height: 12 });
        assert_eq!((anim.width(), anim.height(), report.scale), (5, 4, 3));
        assert_eq!(report.likely_scale, None);
        assert_eq!(report.colours, 4);
        assert_lossless(&input, &anim, &report);

        let (anim, report) =
            normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap();
        assert_eq!(report.crop, Rect { x: 0, y: 0, width: 30, height: 20 });
        // the transparent margins become cells of their own
        assert_eq!((anim.width(), anim.height(), report.scale), (7, 6, 3));
        assert_lossless(&input, &anim, &report);
    }

    #[test]
    fn allows_art_pixels_cut_by_the_canvas() {
        // 5x4 art at 3x, starting this far left of and above the canvas
        let (left, above) = (2, 1);
        let input = animation(13, 11, &[ms(100)], |frame, x, y| {
            art(frame, (x + left) / 3, (y + above) / 3)
        });
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert_eq!((anim.width(), anim.height(), report.scale), (5, 4, 3));
        assert_eq!(anim.grid().columns, [0, 1, 4, 7, 10, 13]);
        assert_eq!(report.likely_scale, None);
        assert_lossless(&input, &anim, &report);
    }

    #[test]
    fn reduces_uneven_upscales() {
        // 4x2 art at 2.5x by 1.5x, as nearest-neighbour scaling makes it
        let (columns, rows) = ([0, 2, 5, 7, 10], [0, 1, 3]);
        let input = animation(10, 3, &[ms(100)], |frame, x, y| {
            art(frame, art_index(&columns, x).unwrap(), art_index(&rows, y).unwrap())
        });
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert_eq!((anim.width(), anim.height(), report.scale), (4, 2, 1));
        assert_eq!(anim.grid(), &Grid { columns: columns.to_vec(), rows: rows.to_vec() });
        assert_lossless(&input, &anim, &report);
    }

    #[test]
    fn suggests_and_snaps_a_likely_scale() {
        // 12x12 art at 2x, with one input pixel off the grid
        let input = animation(24, 24, &[ms(100), ms(100)], |frame, x, y| {
            if (frame, x, y) == (1, 7, 9) { [1, 2, 3, 255] } else { art(frame, x / 2, y / 2) }
        });
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert_eq!(report.scale, 1);
        let likely = report.likely_scale.unwrap();
        assert_eq!(likely.scale, 2);
        assert!(likely.fit > 0.98, "{likely:?}");
        assert_lossless(&input, &anim, &report);

        let forced = Options { pixel_scale: Some(2), ..Options::default() };
        let (anim, report) = normalise(&input, &forced).unwrap();
        assert_eq!((anim.width(), anim.height(), report.scale), (12, 12, 2));
        assert_eq!(report.snapped_pixels, 1);
        assert_eq!(report.likely_scale, None);
        assert_eq!(
            normalise(&input, &Options { pixel_scale: Some(0), ..forced }),
            Err(Error::ZeroScale)
        );
    }

    #[test]
    fn snaps_to_60_fps() {
        let frames: Vec<_> = (0..7).map(|i| (art(i, 0, 0), ms(70))).collect();
        let (anim, report) = normalise(&flat(&frames), &Options::default()).unwrap();
        let ticks: Vec<u32> = anim.frames().iter().map(|frame| frame.ticks).collect();
        // starts rounded from 0, 4.2, 8.4, 12.6, ... 29.4
        assert_eq!(ticks, [4, 4, 5, 4, 4, 4, 4]);
        assert_eq!((report.ticks, report.speed, report.trimmed), (29, 1.0, false));
    }

    #[test]
    fn merges_and_drops_frames() {
        let input = flat(&[
            (RED, ms(100)),
            (RED, ms(100)),
            (GREEN, ms(5)),
            (RED, ms(100)),
            (BLUE, ms(100)),
        ]);
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        let frames: Vec<_> = anim
            .frames()
            .iter()
            .map(|frame| (anim.palette()[frame.pixels[0] as usize], frame.ticks))
            .collect();
        assert_eq!(frames, [(RED, 18), (BLUE, 6)]);
        assert_eq!((report.merged_frames, report.dropped_frames), (2, 1));

        // a frame too short for 60 fps alone still shows
        let (anim, _) = normalise(&flat(&[(RED, ms(5))]), &Options::default()).unwrap();
        assert_eq!(anim.ticks(), 1);
    }

    #[test]
    fn speeds_up_or_trims_long_inputs() {
        let input = flat(&[(RED, ms(1500)), (GREEN, ms(1500)), (BLUE, ms(1500)), (RED, ms(1500))]);
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 45));
        assert_eq!((report.ticks, report.speed), (180, 2.0));

        let trim = Options { long: Long::Trim, ..Options::default() };
        let (anim, report) = normalise(&input, &trim).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 90));
        assert_eq!((anim.frames().len(), report.trimmed, report.speed), (2, true, 1.0));
    }

    #[test]
    fn uses_a_window() {
        let input = flat(&[(RED, ms(1500)), (GREEN, ms(1500)), (BLUE, ms(1500))]);
        let window = Options { start: ms(1200), length: Some(ms(600)), ..Options::default() };
        let (anim, _) = normalise(&input, &window).unwrap();
        let ticks: Vec<_> = anim.frames().iter().map(|frame| frame.ticks).collect();
        assert_eq!(ticks, [18, 18]);

        let late = Options { start: ms(4500), ..Options::default() };
        assert!(matches!(normalise(&input, &late), Err(Error::StartPastEnd { .. })));
        let empty = Options { length: Some(Duration::ZERO), ..Options::default() };
        assert_eq!(normalise(&input, &empty), Err(Error::ZeroLength));
    }

    #[test]
    fn keeps_alpha_levels_apart() {
        let half = [255, 0, 0, 128];
        let input =
            animation(4, 1, &[ms(100)], |_, x, _| [RED, half, CLEAR, [9, 9, 9, 0]][x as usize]);
        let (anim, report) =
            normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap();
        assert_eq!(report.colours, 2);
        assert_eq!(anim.palette(), [CLEAR, RED, half]);
        // both transparent pixels are one cell
        assert_eq!(anim.width(), 3);

        let invisible = animation(2, 2, &[ms(100)], |_, x, _| [x as u8, 0, 0, 0]);
        assert_eq!(normalise(&invisible, &Options::default()), Err(Error::Invisible));
    }

    #[test]
    fn warns_about_many_colours() {
        let input = animation(300, 1, &[ms(100)], |_, x, _| [x as u8, (x >> 8) as u8, 0, 255]);
        let (_, report) = normalise(&input, &Options::default()).unwrap();
        assert_eq!(report.warnings, [Warning::ManyColours { colours: 300 }]);
    }

    #[test]
    fn handles_extreme_inputs() {
        // every colour a palette holds, and transparent
        let input = animation(256, 256, &[ms(100)], |_, x, y| match y * 256 + x {
            0 => CLEAR,
            i => [i as u8, (i >> 8) as u8, 0, 255],
        });
        let canvas = Options { keep_canvas: true, ..Options::default() };
        let (anim, report) = normalise(&input, &canvas).unwrap();
        assert_eq!(report.colours, 65535);
        assert_eq!(crate::encode::runs(&anim).compare(&anim), None);

        // a forced scale far past the art
        let dot = animation(1, 1, &[ms(100)], |_, _, _| RED);
        let huge = Options { pixel_scale: Some(u32::MAX), ..Options::default() };
        assert_eq!(normalise(&dot, &huge).unwrap().1.width, 1);

        // snapping one cell of 65536 different pixels stays quick
        let started = std::time::Instant::now();
        let whole = Options { pixel_scale: Some(256), ..canvas };
        let (anim, report) = normalise(&input, &whole).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!((anim.width(), report.snapped_pixels), (1, 65535));
    }
}

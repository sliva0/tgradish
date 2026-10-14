//! Turns decoded frames into what the encoder works on: palette-indexed
//! frames on Telegram's 60 fps grid, cropped and reduced to the art's own
//! pixels.
//!
//! Everything here is lossless except what the options ask for: a time
//! window, playing faster or slower, cutting at 3 seconds, and a forced
//! pixel scale.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;
use tgradish_frames::Animation;

use crate::limits::telegram::{FPS, MAX_FRAMES};
use crate::{Error, Result};

const NANOS: u64 = 1_000_000_000;
/// The slowest and fastest an animation may play, as many times as fast as
/// its input.
pub const TEMPOS: std::ops::RangeInclusive<f64> = 0.1..=10.0;
/// Longest stretch Telegram allows, in nanoseconds.
const MAX_NANOS: u64 = MAX_FRAMES as u64 * NANOS / FPS as u64;
/// Half a 60 fps frame, in nanoseconds: what is longer than 3 s by less
/// still rounds to 3 s of frames, like 180 frames of 1/60 s each rounded up
/// to whole nanoseconds.
const HALF_TICK: u64 = NANOS / FPS as u64 / 2;
/// Largest pixel scale looked for when the art isn't exactly on a grid.
const MAX_LIKELY_SCALE: u32 = 16;
/// Share of colour edges that must lie on a scale's grid to suggest it.
const LIKELY_FIT: f64 = 0.8;
/// More colours than this and the input probably isn't pixel art.
const MANY_COLOURS: usize = 256;

/// How art that isn't square fills the square canvas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Square {
    /// Whole, along its longer side, with transparent margins.
    #[default]
    Pad,
    /// Cut to a square in its middle.
    Crop,
    /// Stretched, its art pixels no longer square.
    Stretch,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// Keep the input's canvas instead of cropping to the visible pixels.
    pub keep_canvas: bool,
    /// The part of the input to use, in input pixels; all of it if `None`.
    /// The canvas kept with `keep_canvas` is this part.
    pub crop: Option<Rect>,
    /// Size of one art pixel in input pixels. Detected when not given;
    /// forcing one moves pixels that are off its grid onto it (lossy).
    pub pixel_scale: Option<u32>,
    /// Where in the input to start.
    pub start: Duration,
    /// How much of the input to use, from `start`.
    pub length: Option<Duration>,
    /// How many times as fast as the input it plays. When `None`, inputs
    /// longer than 3 seconds play as fast as fits them in. Whatever still
    /// lasts longer is cut at 3 seconds.
    pub tempo: Option<f64>,
    /// How art that isn't square fills the canvas; only cropping changes
    /// what normalising makes.
    pub square: Square,
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
        let cut = |edges: &[u32]| {
            let (first, last) =
                (edges[1] - edges[0], edges[edges.len() - 1] - edges[edges.len() - 2]);
            edges.len() > 2 && (!first.is_multiple_of(unit) || !last.is_multiple_of(unit))
        };
        cut(&self.columns) || cut(&self.rows)
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
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) grid: Grid,
    pub(crate) palette: Vec<[u8; 4]>,
    pub(crate) frames: Vec<PixelFrame>,
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

    /// Where each frame starts, in 60 fps frames, and then the length.
    pub fn starts(&self) -> Vec<u32> {
        let mut starts = Vec::with_capacity(self.frames.len() + 1);
        starts.push(0);
        for frame in &self.frames {
            starts.push(starts.last().unwrap() + frame.ticks);
        }
        starts
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

    /// A frame as packed pixels (see [`pack`]) at the input's resolution,
    /// cropped: [`PixelAnim::rgba`] without the bytes in between.
    pub(crate) fn packed(&self, frame: usize) -> Vec<u32> {
        let (columns, rows) = (&self.grid.columns, &self.grid.rows);
        let width = *columns.last().unwrap() as usize;
        let palette: Vec<u32> =
            self.palette.iter().map(|&colour| u32::from_le_bytes(colour)).collect();
        let mut out = Vec::with_capacity(width * *rows.last().unwrap() as usize);
        let pixels = &self.frames[frame].pixels;
        for (row, edges) in rows.windows(2).enumerate() {
            // the first line of a row of cells, then copies of it
            let start = out.len();
            for (column, cell) in columns.windows(2).enumerate() {
                let colour = palette[pixels[row * self.width as usize + column] as usize];
                out.extend(std::iter::repeat_n(colour, (cell[1] - cell[0]) as usize));
            }
            for _ in edges[0] + 1..edges[1] {
                out.extend_from_within(start..start + width);
            }
        }
        out
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// What normalising did, for the user and the protocol.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// The art's pixel scale: every cell is a multiple of it, but those a
    /// crop cuts at the sides.
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
    /// How many times as fast as the input it plays.
    pub tempo: f64,
    /// Whether a long input was cut at 3 seconds.
    pub trimmed: bool,
    pub warnings: Vec<Warning>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LikelyScale {
    pub scale: u32,
    pub fit: f64,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum Warning {
    /// Hundreds of colours: probably not pixel art, so the sticker will be
    /// large.
    ManyColours { colours: usize },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::ManyColours { colours } => write!(
                f,
                "{colours} colours, which pixel art seldom has: the sticker will be large"
            ),
        }
    }
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
    let (input_width, input_height) = (animation.width(), animation.height());
    let area =
        options.crop.unwrap_or(Rect { x: 0, y: 0, width: input_width, height: input_height });
    let outside = u64::from(area.x) + u64::from(area.width) > u64::from(input_width)
        || u64::from(area.y) + u64::from(area.height) > u64::from(input_height);
    if area.width == 0 || area.height == 0 || outside {
        return Err(Error::CropOutside { crop: area, width: input_width, height: input_height });
    }
    let (tempo, fitted) = match options.tempo {
        Some(tempo) if !TEMPOS.contains(&tempo) => return Err(Error::Tempo(tempo)),
        Some(tempo) => (tempo, false),
        None if end - start > MAX_NANOS + HALF_TICK => {
            ((end - start) as f64 / MAX_NANOS as f64, true)
        }
        None => (1.0, false),
    };
    // what still plays longer than 3 seconds is cut
    let most = (MAX_NANOS as f64 * tempo).round() as u64;
    let trimmed = !fitted && end - start > most + HALF_TICK;
    if trimmed {
        end = start + most;
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
        let mut pixels = pack(&frame.rgba);
        if options.crop.is_some() {
            pixels = cut(&pixels, input_width, area);
        }
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
    let (numer, denom) = if fitted {
        (u128::from(MAX_FRAMES), u128::from(end - start))
    } else {
        // the tempo in millionths, to stay in whole numbers
        let millionths = (tempo * 1e6).round() as u128;
        (u128::from(FPS) * 1_000_000, u128::from(NANOS) * millionths)
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

    let crop = if options.keep_canvas {
        Rect { x: 0, y: 0, width: area.width, height: area.height }
    } else {
        visible_area(&frames, area.width).ok_or(Error::Invisible)?
    };
    let mut crop = crop;
    let mut pixels: Vec<Vec<u32>> =
        frames.iter().map(|(pixels, _)| cut(pixels, area.width, crop)).collect();
    let mut edges = Edges::count(&pixels, crop.width, crop.height);

    let mut snapped_pixels = 0;
    match options.pixel_scale {
        Some(0) => return Err(Error::ZeroScale),
        Some(scale @ 2..) => {
            // anything larger than the art makes it one cell either way
            let scale = scale.min(crop.width.max(crop.height));
            // the crop is the visible pixels', or the canvas
            let offset = edges.fit(scale, true).offset;
            for frame in &mut pixels {
                snapped_pixels += snap(frame, crop.width, crop.height, scale, offset);
            }
            edges = Edges::count(&pixels, crop.width, crop.height);
        }
        _ => {}
    }

    // a larger scale the art likely has, judged before a square is cut
    // from its middle, through its art pixels
    let likely_scale = |scale: u32| {
        (scale + 1..=MAX_LIKELY_SCALE)
            .rev()
            .map(|candidate| (candidate, edges.fit(candidate, true).fit))
            .find(|&(_, fit)| fit >= LIKELY_FIT)
            .map(|(scale, fit)| LikelyScale { scale, fit })
    };
    let likely_scale = match options.pixel_scale {
        Some(_) => None,
        None => likely_scale(edges.grid().scale()),
    };

    let ticks: Vec<u32> = frames.iter().map(|&(_, ticks)| ticks).collect();
    // cut square before the colours are gathered: those cut off don't count
    if options.square == Square::Crop && crop.width != crop.height {
        let square = middle_square(crop.width, crop.height);
        pixels = pixels.iter().map(|frame| cut(frame, crop.width, square)).collect();
        crop = Rect { x: crop.x + square.x, y: crop.y + square.y, ..square };
        edges = Edges::count(&pixels, crop.width, crop.height);
    }
    let anim = assemble(&pixels, &ticks, crop.width, &edges)?;
    let (width, height, grid) = (anim.width, anim.height, anim.grid.clone());

    let scale = grid.scale();
    // larger than the scale the cut art has, the likely one is no news
    let likely_scale = likely_scale.filter(|likely| likely.scale > scale);
    let colours = anim.palette.len() - 1;
    let mut warnings = Vec::new();
    if colours > MANY_COLOURS {
        warnings.push(Warning::ManyColours { colours });
    }
    let report = Report {
        input_width,
        input_height,
        input_frames: animation.frames().len(),
        input_length: input_length as f64 / NANOS as f64,
        crop: Rect { x: area.x + crop.x, y: area.y + crop.y, ..crop },
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
        tempo,
        trimmed,
        warnings,
    };
    Ok((anim, report))
}

/// Turns frames of packed pixels, `width` wide, into cells and a palette.
/// `edges` must be their [`Edges`].
pub(crate) fn assemble(
    pixels: &[Vec<u32>],
    ticks: &[u32],
    width: u32,
    edges: &Edges,
) -> Result<PixelAnim> {
    let grid = edges.grid();
    let (cells_wide, cells_high) = (grid.columns.len() as u32 - 1, grid.rows.len() as u32 - 1);
    let mut palette = vec![[0; 4]];
    let mut index = HashMap::from([(0u32, 0u16)]);
    let mut frames = Vec::with_capacity(pixels.len());
    for (pixels, &ticks) in pixels.iter().zip(ticks) {
        let mut cells = Vec::with_capacity(cells_wide as usize * cells_high as usize);
        for &y in &grid.rows[..cells_high as usize] {
            for &x in &grid.columns[..cells_wide as usize] {
                let colour = pixels[(y * width + x) as usize];
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
        frames.push(PixelFrame { pixels: cells, ticks });
    }
    Ok(PixelAnim { width: cells_wide, height: cells_high, grid, palette, frames })
}

/// The largest square in the middle of a `width` by `height` picture.
/// It may cut art pixels at its sides: the canvas shows only their part
/// inside it.
fn middle_square(width: u32, height: u32) -> Rect {
    let side = width.min(height);
    Rect { x: (width - side) / 2, y: (height - side) / 2, width: side, height: side }
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
pub(crate) struct Edges {
    columns: Vec<u64>,
    rows: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridFit {
    /// Where the grid starts, from the crop's top left.
    pub(crate) offset: (u32, u32),
    /// Share of colour edges on the grid.
    fit: f64,
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
    /// offset. The `borders` of the crop count as edges where they lie on
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
fn snap(pixels: &mut [u32], width: u32, height: u32, scale: u32, offset: (u32, u32)) -> u64 {
    let (columns, rows) =
        (grid_starts(offset.0, width, scale), grid_starts(offset.1, height, scale));
    snap_to(pixels, width, &columns, &rows)
}

/// Edges of a grid of `scale` along a side of `size` pixels, starting at
/// `offset`: from 0 to the size, cells at the ends cut short.
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

        // cropped square in the middle: the art pixels at its sides cut
        let square = Options { square: Square::Crop, ..Options::default() };
        let (anim, report) = normalise(&input, &square).unwrap();
        assert_eq!(report.crop, Rect { x: 5, y: 5, width: 12, height: 12 });
        assert_eq!(anim.grid().columns, [0, 2, 5, 8, 11, 12]);
        assert_eq!((anim.width(), anim.height(), report.scale), (5, 4, 3));
        assert_lossless(&input, &anim, &report);
        // stretching and padding leave it as it is
        for square in [Square::Pad, Square::Stretch] {
            let (_, report) = normalise(&input, &Options { square, ..Options::default() }).unwrap();
            assert_eq!(report.crop, Rect { x: 4, y: 5, width: 15, height: 12 });
        }
    }

    #[test]
    fn uses_only_the_crop() {
        // the same art, of which the crop holds the top left 2x2 art pixels
        let input = animation(30, 20, &[ms(100), ms(100)], |frame, x, y| {
            match (x.checked_sub(4).map(|x| x / 3), y.checked_sub(5).map(|y| y / 3)) {
                (Some(x @ 0..5), Some(y @ 0..4)) => art(frame, x, y),
                _ => CLEAR,
            }
        });
        let crop = Some(Rect { x: 2, y: 3, width: 8, height: 8 });
        let (anim, report) = normalise(&input, &Options { crop, ..Options::default() }).unwrap();
        assert_eq!(report.crop, Rect { x: 4, y: 5, width: 6, height: 6 });
        assert_eq!((anim.width(), anim.height(), report.scale), (2, 2, 3));
        assert_lossless(&input, &anim, &report);

        let options = Options { crop, keep_canvas: true, ..Options::default() };
        let (_, report) = normalise(&input, &options).unwrap();
        assert_eq!(Some(report.crop), crop);
        let outside = Some(Rect { x: 25, y: 0, width: 8, height: 8 });
        let options = Options { crop: outside, ..Options::default() };
        assert!(matches!(normalise(&input, &options), Err(Error::CropOutside { .. })));
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
        assert_eq!((report.ticks, report.tempo, report.trimmed), (29, 1.0, false));
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
        assert_eq!((report.ticks, report.tempo, report.trimmed), (180, 2.0, false));

        let trim = Options { tempo: Some(1.0), ..Options::default() };
        let (anim, report) = normalise(&input, &trim).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 90));
        assert_eq!((anim.frames().len(), report.trimmed, report.tempo), (2, true, 1.0));

        // faster than it needs, or slower and cut
        let fast = Options { tempo: Some(3.0), ..Options::default() };
        let (anim, report) = normalise(&input, &fast).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 30));
        assert_eq!((report.ticks, report.trimmed), (120, false));
        let slow = Options { tempo: Some(0.5), ..Options::default() };
        let (anim, report) = normalise(&input, &slow).unwrap();
        let ticks: Vec<u32> = anim.frames().iter().map(|frame| frame.ticks).collect();
        assert_eq!((ticks, report.trimmed), (vec![180], true));
        for tempo in [0.0, 1e-7, 11.0, 1e300, f64::NAN] {
            let wrong = Options { tempo: Some(tempo), ..Options::default() };
            assert!(matches!(normalise(&input, &wrong), Err(Error::Tempo(_))), "{tempo}");
        }
    }

    #[test]
    fn takes_3_s_of_60_fps_frames_whole() {
        // 1/60 s in whole nanoseconds is a little longer
        let tick = Duration::from_secs_f64(1.0 / 60.0);
        let frames: Vec<_> =
            (0..180).map(|i| (if i % 2 == 0 { RED } else { BLUE }, tick)).collect();
        for tempo in [None, Some(1.0)] {
            let options = Options { tempo, ..Options::default() };
            let (anim, report) = normalise(&flat(&frames), &options).unwrap();
            assert_eq!((report.ticks, report.tempo, report.trimmed), (180, 1.0, false));
            assert_eq!(anim.frames().len(), 180);
        }
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
        let fast = crate::encode::Settings {
            effort: crate::encode::Effort::Fast,
            ..crate::encode::Settings::default()
        };
        // a group per colour, spread over enough layers for tlottie
        let scene = crate::encode::painter(&anim, &fast, None, &|| false).unwrap();
        assert_eq!(scene.compare(&anim), None);
        let merged = crate::reduce::Reduction::MergeColours { distance: 0.1 }.apply(&anim).unwrap();
        assert!(merged.palette().len() < 65536);

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

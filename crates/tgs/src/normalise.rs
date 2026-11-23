//! Converts decoded frames into the encoder's input representation:
//! palette-indexed frames aligned to Telegram's 60 fps grid, cropped and
//! reduced to the art's own pixel grid.
//!
//! Normalisation is lossless except for explicit options: time windowing,
//! tempo changes, trimming at 3 seconds, and forcing a pixel scale.

use std::collections::HashMap;
use std::time::Duration;

use crate::frames::Animation;
use serde::Serialize;

pub use crate::grid::Grid;
use crate::grid::{Edges, snap};
use crate::limits::telegram::{FPS, MAX_FRAMES};
use crate::{Error, Result};

const NANOS: u64 = 1_000_000_000;
/// Allowed playback speed multiplier range for [`Options::tempo`]. When
/// `tempo` is `None`, inputs longer than 3 seconds are automatically sped up
/// to fit, which can exceed this range for inputs longer than 30 seconds.
pub const TEMPOS: std::ops::RangeInclusive<f64> = 0.1..=10.0;
/// Tolerance for snapping [`Options::start`] and [`Options::length`] to nearby
/// frame boundaries so rounded timestamps cut cleanly between frames: 0.1 ms
/// (a tenth of the shortest frame Aseprite or WebP can time, 1 ms) and at most
/// one quarter of either adjacent frame.
pub const CUT_LEEWAY: Duration = Duration::from_micros(100);
/// Maximum sticker duration allowed by Telegram, in nanoseconds.
const MAX_NANOS: u64 = MAX_FRAMES as u64 * NANOS / FPS as u64;
/// Maximum pixel scale considered when suggesting a likely scale.
const MAX_LIKELY_SCALE: u32 = 16;
/// Minimum share of colour edges on a grid required to suggest a likely scale.
const LIKELY_FIT: f64 = 0.8;
/// Colour count above which a non-pixel-art warning is emitted.
const MANY_COLOURS: usize = 256;

/// How non-square art is fitted into Telegram's square canvas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Square {
    /// Fit whole along the longer side with transparent margins.
    #[default]
    Pad,
    /// Centre-crop to a square.
    Crop,
    /// Stretch to fill the square canvas (distorts aspect ratio).
    Stretch,
}

/// Options for [`normalise`]. Construct with
/// `Options { tempo, ..Options::default() }` so future fields retain defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// Keep the full canvas instead of trimming transparent margins.
    pub keep_canvas: bool,
    /// Optional crop rectangle in input pixels (`None` uses the whole input).
    /// When `keep_canvas` is true, this rectangle is preserved as the canvas.
    pub crop: Option<Rect>,
    /// Forced pixel scale in input pixels per art pixel. Auto-detected when
    /// `None`; setting a value snaps off-grid pixels onto it (lossy).
    pub pixel_scale: Option<u32>,
    /// Start time in the input; snaps to a frame boundary within [`CUT_LEEWAY`].
    pub start: Duration,
    /// Duration to take from `start`; end snaps to a frame boundary within
    /// [`CUT_LEEWAY`].
    pub length: Option<Duration>,
    /// Playback speed multiplier. When `None`, inputs longer than 3 seconds
    /// speed up to fit. Anything still over 3 seconds is trimmed.
    pub tempo: Option<f64>,
    /// How non-square art fills the square canvas (only `Square::Crop` alters
    /// the normalised frames).
    pub square: Square,
}

/// One palette-indexed frame and its duration in 60 fps ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelFrame {
    /// Palette index for each cell, in row-major order.
    pub pixels: Vec<u16>,
    /// Duration in 60 fps ticks (at least 1).
    pub ticks: u32,
}

/// A normalised pixel-art animation. Cells are the largest blocks of input
/// pixels that stay uniform across all frames, so the grid may be uneven;
/// palette index `0` is always transparent. Only [`normalise`] and reductions
/// construct this type so its invariants hold.
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

    /// Cell boundary grid in input pixels.
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Straight RGBA palette; index `0` is transparent.
    pub fn palette(&self) -> &[[u8; 4]] {
        &self.palette
    }

    /// Frames in playback order, with no two consecutive identical frames.
    pub fn frames(&self) -> &[PixelFrame] {
        &self.frames
    }

    /// Total duration in 60 fps ticks.
    pub fn ticks(&self) -> u32 {
        self.frames.iter().map(|frame| frame.ticks).sum()
    }

    /// Start tick of each frame, followed by the total tick count.
    pub fn starts(&self) -> Vec<u32> {
        let mut starts = Vec::with_capacity(self.frames.len() + 1);
        starts.push(0);
        for frame in &self.frames {
            starts.push(starts.last().unwrap() + frame.ticks);
        }
        starts
    }

    /// Returns true if any cell in any frame is non-transparent (index != 0).
    pub(crate) fn shows(&self) -> bool {
        self.frames.iter().any(|frame| frame.pixels.iter().any(|&colour| colour != 0))
    }

    /// Renders `frame` as straight RGBA at the cropped input resolution.
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

    /// Renders `frame` as packed `u32` pixels (see [`pack`]) at the cropped
    /// input resolution; used by tests to compare cell-based reductions
    /// against pixel-based ones.
    #[cfg(test)]
    pub(crate) fn packed(&self, frame: usize) -> Vec<u32> {
        let (columns, rows) = (&self.grid.columns, &self.grid.rows);
        let width = *columns.last().unwrap() as usize;
        let palette: Vec<u32> =
            self.palette.iter().map(|&colour| u32::from_le_bytes(colour)).collect();
        let mut out = Vec::with_capacity(width * *rows.last().unwrap() as usize);
        let pixels = &self.frames[frame].pixels;
        for (row, edges) in rows.windows(2).enumerate() {
            // Write the first scanline of the cell row, then repeat it.
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

/// A pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Rect {
    /// Left offset in pixels.
    pub x: u32,
    /// Top offset in pixels.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Summary of what [`normalise`] produced.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Report {
    /// Input width in pixels.
    pub input_width: u32,
    /// Input height in pixels.
    pub input_height: u32,
    /// Input frame count.
    pub input_frames: usize,
    /// Total input duration in seconds.
    pub input_length: f64,
    /// Used region of the input in input pixels.
    pub crop: Rect,
    /// Width in normalised cells (see [`PixelAnim`]).
    pub width: u32,
    /// Height in normalised cells.
    pub height: u32,
    /// Detected or forced pixel scale: every interior cell size is a multiple
    /// of this value.
    pub scale: u32,
    /// Suggested larger pixel scale if most colour edges align to it.
    pub likely_scale: Option<LikelyScale>,
    /// Number of input pixels modified by a forced pixel scale.
    pub snapped_pixels: u64,
    /// Distinct non-transparent `(r, g, b, a)` colours.
    pub colours: usize,
    /// Kept frame count after merging consecutive identical frames.
    pub frames: usize,
    /// Number of consecutive identical frames merged.
    pub merged_frames: usize,
    /// Number of frames dropped because they rounded to 0 ticks at 60 fps.
    pub dropped_frames: usize,
    /// Output duration in 60 fps ticks.
    pub ticks: u32,
    /// Applied playback speed multiplier.
    pub tempo: f64,
    /// Whether the animation was trimmed at 3 seconds.
    pub trimmed: bool,
    /// Non-fatal warnings about the input.
    pub warnings: Vec<Warning>,
}

/// A larger pixel scale that most colour edges follow.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct LikelyScale {
    /// Suggested scale in input pixels per art pixel.
    pub scale: u32,
    /// Fraction of colour edges aligned to this scale (`0.0..=1.0`).
    pub fit: f64,
}

/// Non-fatal warning emitted during normalisation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "warning", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Warning {
    /// Input has many colours (likely not pixel art, so the sticker will be large).
    ManyColours {
        /// Number of distinct non-transparent colours.
        colours: usize,
    },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::ManyColours { colours } => {
                write!(f, "{colours} colours found (unusual for pixel art; sticker will be large)")
            }
        }
    }
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Snaps `at` (in nanoseconds) to a nearby frame start (or end when `ends` is
/// true) if within [`CUT_LEEWAY`] and within a quarter of either adjacent
/// frame's duration.
fn on_edge(at: u64, durations: &[u64], ends: bool) -> u64 {
    let leeway = nanos(CUT_LEEWAY);
    let mut edge = 0u64;
    for index in 0..=durations.len() {
        if edge > at.saturating_add(leeway) {
            break;
        }
        let before = index.checked_sub(1).map(|before| durations[before]);
        let after = durations.get(index).copied();
        // a start is where a frame starts, an end where one ends
        let side = if ends { before } else { after };
        let reach = before.into_iter().chain(after).min().unwrap_or(0) / 4;
        if side.is_some() && at.abs_diff(edge) <= leeway.min(reach) {
            return edge;
        }
        edge = edge.saturating_add(after.unwrap_or(0));
    }
    at
}

/// Packs each pixel of `area`, in RGBA `width` pixels wide, into a `u32`
/// with every fully transparent pixel as 0.
fn pack(rgba: &[u8], width: u32, area: Rect) -> Vec<u32> {
    let pixels = rgba.as_chunks::<4>().0;
    let mut out = Vec::with_capacity(area.width as usize * area.height as usize);
    for y in area.y..area.y + area.height {
        let start = y as usize * width as usize + area.x as usize;
        out.extend(
            pixels[start..start + area.width as usize]
                .iter()
                .map(|&pixel| if pixel[3] == 0 { 0 } else { u32::from_le_bytes(pixel) }),
        );
    }
    out
}

/// Quantised frame schedule on Telegram's 60 fps grid.
#[derive(Debug, Clone, PartialEq)]
struct Timing {
    /// Visible duration of each input frame in nanoseconds (`0` outside the
    /// active window).
    shown: Vec<u64>,
    /// 60 fps ticks assigned to each input frame (`1..=MAX_FRAMES` in total).
    ticks: Vec<u32>,
    /// Applied playback speed multiplier.
    tempo: f64,
    /// Whether the animation was trimmed to fit 3 seconds.
    trimmed: bool,
}

/// Maps input frame `durations` within `start..end` (nanoseconds) onto
/// Telegram's 60 fps grid at `tempo` (auto-speeding up when `None`).
///
/// Overflow is checked in 60 fps ticks rather than input nanoseconds (where a
/// half-tick leeway would equal 5 output ticks at tempo 0.1 and 1/20 of a tick
/// at tempo 10). Anything rounding to 180 ticks (such as 180 frames of 1/60 s
/// each rounded up to whole nanoseconds) is kept whole, and anything beyond is
/// trimmed at tick 180.
fn timing(durations: &[u64], start: u64, end: u64, tempo: Option<f64>) -> Result<Timing> {
    let length = end.saturating_sub(start);
    // 60 fps frames a nanosecond of input, as whole numbers over each other
    let rate = |tempo: f64| {
        let millionths = (tempo * 1e6).round() as u128;
        (u128::from(FPS) * 1_000_000, u128::from(NANOS) * millionths)
    };
    // the 60 fps frame nearest to a time, so each frame's rounding doesn't
    // add up over those before it
    let tick = |at: u64, (numer, denom): (u128, u128)| (u128::from(at) * numer + denom / 2) / denom;
    let most = u128::from(MAX_FRAMES);
    let (tempo, rate, trimmed) = match tempo {
        Some(tempo) if !TEMPOS.contains(&tempo) => return Err(Error::Tempo(tempo)),
        Some(tempo) => (tempo, rate(tempo), tick(length, rate(tempo)) > most),
        None if tick(length, rate(1.0)) > most => {
            (length as f64 / MAX_NANOS as f64, (most, u128::from(length)), false)
        }
        None => (1.0, rate(1.0), false),
    };
    // what still plays longer than 3 seconds is cut
    let end = if trimmed { start + (most * rate.1 / rate.0) as u64 } else { end };

    let (mut at, mut elapsed) = (0u64, 0u64);
    let mut shown = Vec::with_capacity(durations.len());
    let mut ticks = Vec::with_capacity(durations.len());
    for &duration in durations {
        let (from, to) = (at, at.saturating_add(duration));
        at = to;
        let within = to.min(end).saturating_sub(from.max(start));
        let before = tick(elapsed, rate);
        elapsed += within;
        shown.push(within);
        // no more than `most` in all, by how `rate` and `end` were chosen
        ticks.push((tick(elapsed, rate) - before) as u32);
    }
    // a part too short for 60 fps still shows, as its longest frame
    if ticks.iter().all(|&ticks| ticks == 0)
        && let Some(longest) = (0..shown.len()).max_by_key(|&index| shown[index])
    {
        ticks[longest] = 1;
    }
    Ok(Timing { shown, ticks, tempo, trimmed })
}

/// Normalises `animation` into a [`PixelAnim`] and [`Report`] according to
/// `options`: aligns frames to 60 fps, trims margins or crops, builds a
/// palette, and detects the cell grid. Consumes `animation` so each decoded
/// frame can be dropped as it is packed, keeping peak memory near input size.
pub fn normalise(animation: Animation, options: &Options) -> Result<(PixelAnim, Report)> {
    let durations: Vec<u64> =
        animation.frames().iter().map(|frame| nanos(frame.duration)).collect();
    let input_length =
        durations.iter().fold(0u64, |total, &duration| total.saturating_add(duration));
    let asked = nanos(options.start);
    let start = on_edge(asked, &durations, false);
    if start >= input_length {
        let length = Duration::from_nanos(input_length);
        return Err(Error::StartPastEnd { start: options.start, length });
    }
    let end = match options.length {
        Some(length) if length.is_zero() => return Err(Error::ZeroLength),
        Some(length) => {
            let end = asked.saturating_add(nanos(length)).min(input_length);
            Some(on_edge(end, &durations, true)).filter(|&edge| edge > start).unwrap_or(end)
        }
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
    let Timing { shown, ticks, tempo, trimmed } = timing(&durations, start, end, options.tempo)?;

    // frames within what is used, joined while identical
    let mut timed: Vec<(Vec<u32>, u32)> = Vec::new();
    let mut merged_frames = 0;
    let input_frames = animation.frames().len();
    // each frame goes once it is packed, so the input and its packed copy
    // are never held whole together
    let frames = animation.into_frames().into_iter();
    for (frame, (shown, ticks)) in frames.zip(shown.into_iter().zip(ticks)) {
        // outside the part that is used: an `Animation` has no frames that
        // show for no time
        if shown == 0 {
            continue;
        }
        let pixels = pack(&frame.rgba, input_width, area);
        match timed.last_mut() {
            Some((last, total)) if *last == pixels => {
                *total += ticks;
                merged_frames += 1;
            }
            _ => timed.push((pixels, ticks)),
        }
    }

    let mut frames: Vec<(Vec<u32>, u32)> = Vec::new();
    let mut dropped_frames = 0;
    for (pixels, ticks) in timed {
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

    // nothing to see is no sticker, on a kept canvas as on a cropped one,
    // where it would otherwise come out blank and pass every check
    let visible = visible_area(&frames, area.width).ok_or(Error::Invisible)?;
    let crop = if options.keep_canvas {
        Rect { x: 0, y: 0, width: area.width, height: area.height }
    } else {
        visible
    };
    let mut crop = crop;
    let ticks: Vec<u32> = frames.iter().map(|&(_, ticks)| ticks).collect();
    // frames the crop leaves whole are kept as they are, not copied
    let whole = crop == Rect { x: 0, y: 0, width: area.width, height: area.height };
    let mut pixels: Vec<Vec<u32>> = frames
        .into_iter()
        .map(|(pixels, _)| if whole { pixels } else { cut(&pixels, area.width, crop) })
        .collect();
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

    // cut square before the colours are gathered: those cut off don't count
    if options.square == Square::Crop && crop.width != crop.height {
        let square = middle_square(crop.width, crop.height);
        pixels = pixels.into_iter().map(|frame| cut(&frame, crop.width, square)).collect();
        crop = Rect { x: crop.x + square.x, y: crop.y + square.y, ..square };
        edges = Edges::count(&pixels, crop.width, crop.height);
    }
    // snapping and the square can each leave nothing to see of what did
    // show: as blank as an empty input, and no sticker either
    if pixels.iter().flatten().all(|&pixel| pixel == 0) {
        return Err(Error::Invisible);
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
        input_frames,
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
    let ticks = &ticks[..ticks.len().min(pixels.len())];
    assemble_with(ticks, edges, |frame, x, y| pixels[frame][(y * width + x) as usize])
}

/// [`assemble`] of frames whose packed pixels `pixel(frame, x, y)` gives,
/// one for each of `ticks`.
pub(crate) fn assemble_with(
    ticks: &[u32],
    edges: &Edges,
    pixel: impl Fn(usize, u32, u32) -> u32,
) -> Result<PixelAnim> {
    let grid = edges.grid();
    let (cells_wide, cells_high) = (grid.columns.len() as u32 - 1, grid.rows.len() as u32 - 1);
    let mut palette = vec![[0; 4]];
    let mut index = HashMap::from([(0u32, 0u16)]);
    let mut frames = Vec::with_capacity(ticks.len());
    for (frame, &ticks) in ticks.iter().enumerate() {
        let mut cells = Vec::with_capacity(cells_wide as usize * cells_high as usize);
        for &y in &grid.rows[..cells_high as usize] {
            for &x in &grid.columns[..cells_wide as usize] {
                let colour = pixel(frame, x, y);
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

/// Returns the largest centred square inside `width × height` (used by
/// [`Square::Crop`]). Outer art pixels may be clipped at its edges.
pub fn middle_square(width: u32, height: u32) -> Rect {
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
        let start = y as usize * width as usize + crop.x as usize;
        out.extend_from_slice(&pixels[start..start + crop.width as usize]);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::frames::Frame;

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
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
        assert_eq!(report.crop, Rect { x: 4, y: 5, width: 15, height: 12 });
        assert_eq!((anim.width(), anim.height(), report.scale), (5, 4, 3));
        assert_eq!(report.likely_scale, None);
        assert_eq!(report.colours, 4);
        assert_lossless(&input, &anim, &report);

        let (anim, report) =
            normalise(input.clone(), &Options { keep_canvas: true, ..Options::default() }).unwrap();
        assert_eq!(report.crop, Rect { x: 0, y: 0, width: 30, height: 20 });
        // the transparent margins become cells of their own
        assert_eq!((anim.width(), anim.height(), report.scale), (7, 6, 3));
        assert_lossless(&input, &anim, &report);

        // cropped square in the middle: the art pixels at its sides cut
        let square = Options { square: Square::Crop, ..Options::default() };
        let (anim, report) = normalise(input.clone(), &square).unwrap();
        assert_eq!(report.crop, Rect { x: 5, y: 5, width: 12, height: 12 });
        assert_eq!(anim.grid().columns, [0, 2, 5, 8, 11, 12]);
        assert_eq!((anim.width(), anim.height(), report.scale), (5, 4, 3));
        assert_lossless(&input, &anim, &report);
        // stretching and padding leave it as it is
        for square in [Square::Pad, Square::Stretch] {
            let (_, report) =
                normalise(input.clone(), &Options { square, ..Options::default() }).unwrap();
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
        let (anim, report) =
            normalise(input.clone(), &Options { crop, ..Options::default() }).unwrap();
        assert_eq!(report.crop, Rect { x: 4, y: 5, width: 6, height: 6 });
        assert_eq!((anim.width(), anim.height(), report.scale), (2, 2, 3));
        assert_lossless(&input, &anim, &report);

        let options = Options { crop, keep_canvas: true, ..Options::default() };
        let (_, report) = normalise(input.clone(), &options).unwrap();
        assert_eq!(Some(report.crop), crop);
        let outside = Some(Rect { x: 25, y: 0, width: 8, height: 8 });
        let options = Options { crop: outside, ..Options::default() };
        assert!(matches!(normalise(input.clone(), &options), Err(Error::CropOutside { .. })));
    }

    #[test]
    fn allows_art_pixels_cut_by_the_canvas() {
        // 5x4 art at 3x, starting this far left of and above the canvas
        let (left, above) = (2, 1);
        let input = animation(13, 11, &[ms(100)], |frame, x, y| {
            art(frame, (x + left) / 3, (y + above) / 3)
        });
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
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
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
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
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
        assert_eq!(report.scale, 1);
        let likely = report.likely_scale.unwrap();
        assert_eq!(likely.scale, 2);
        assert!(likely.fit > 0.98, "{likely:?}");
        assert_lossless(&input, &anim, &report);

        let forced = Options { pixel_scale: Some(2), ..Options::default() };
        let (anim, report) = normalise(input.clone(), &forced).unwrap();
        assert_eq!((anim.width(), anim.height(), report.scale), (12, 12, 2));
        assert_eq!(report.snapped_pixels, 1);
        assert_eq!(report.likely_scale, None);
        assert_eq!(
            normalise(input.clone(), &Options { pixel_scale: Some(0), ..forced }),
            Err(Error::ZeroScale)
        );
    }

    #[test]
    fn snaps_to_60_fps() {
        let frames: Vec<_> = (0..7).map(|i| (art(i, 0, 0), ms(70))).collect();
        let (anim, report) = normalise(flat(&frames), &Options::default()).unwrap();
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
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
        let frames: Vec<_> = anim
            .frames()
            .iter()
            .map(|frame| (anim.palette()[frame.pixels[0] as usize], frame.ticks))
            .collect();
        assert_eq!(frames, [(RED, 18), (BLUE, 6)]);
        assert_eq!((report.merged_frames, report.dropped_frames), (2, 1));

        // a frame too short for 60 fps alone still shows
        let (anim, _) = normalise(flat(&[(RED, ms(5))]), &Options::default()).unwrap();
        assert_eq!(anim.ticks(), 1);
    }

    #[test]
    fn speeds_up_or_trims_long_inputs() {
        let input = flat(&[(RED, ms(1500)), (GREEN, ms(1500)), (BLUE, ms(1500)), (RED, ms(1500))]);
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 45));
        assert_eq!((report.ticks, report.tempo, report.trimmed), (180, 2.0, false));

        let trim = Options { tempo: Some(1.0), ..Options::default() };
        let (anim, report) = normalise(input.clone(), &trim).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 90));
        assert_eq!((anim.frames().len(), report.trimmed, report.tempo), (2, true, 1.0));

        // faster than it needs, or slower and cut
        let fast = Options { tempo: Some(3.0), ..Options::default() };
        let (anim, report) = normalise(input.clone(), &fast).unwrap();
        assert!(anim.frames().iter().all(|frame| frame.ticks == 30));
        assert_eq!((report.ticks, report.trimmed), (120, false));
        let slow = Options { tempo: Some(0.5), ..Options::default() };
        let (anim, report) = normalise(input.clone(), &slow).unwrap();
        let ticks: Vec<u32> = anim.frames().iter().map(|frame| frame.ticks).collect();
        assert_eq!((ticks, report.trimmed), (vec![180], true));
        for tempo in [0.0, 1e-7, 11.0, 1e300, f64::NAN] {
            let wrong = Options { tempo: Some(tempo), ..Options::default() };
            assert!(matches!(normalise(input.clone(), &wrong), Err(Error::Tempo(_))), "{tempo}");
        }
    }

    #[test]
    fn shows_frames_that_say_no_time() {
        // an animation shows them for the default 100 ms, so none goes
        // missing without being counted as dropped
        let input = flat(&[(RED, ms(0)), (GREEN, ms(50)), (BLUE, ms(0))]);
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
        let ticks: Vec<u32> = anim.frames().iter().map(|frame| frame.ticks).collect();
        assert_eq!((ticks, report.dropped_frames, report.input_length), (vec![6, 3, 6], 0, 0.25));
        // nor is one where every frame does past its end at its start
        let input = flat(&[(RED, ms(0)), (BLUE, ms(0))]);
        let (anim, report) = normalise(input.clone(), &Options::default()).unwrap();
        assert_eq!((anim.frames().len(), report.ticks), (2, 12));
    }

    #[test]
    fn keeps_any_tempo_to_180_frames() {
        const TICK: f64 = NANOS as f64 / FPS as f64;
        let most = f64::from(MAX_FRAMES);
        // lengths this many 60 fps frames from 3 seconds of them, never
        // halfway between two
        let around = [-90.0, -1.0, -0.6, -0.4, -0.1, 0.0, 0.1, 0.3, 0.4, 0.6, 1.0, 4.8, 500.0];
        for tempo in [0.1, 0.25, 0.5, 1.0, 2.0, 10.0] {
            for off in around {
                let length = ((most + off) * TICK * tempo).round() as u64;
                // as 60 fps frames, were they not whole
                let ideal = length as f64 / (TICK * tempo);
                // as one frame, as seven uneven ones, and from a start
                let sevenths: Vec<u64> = (1..=7).map(|part| length * part / 28).collect();
                let uneven: u64 = sevenths.iter().sum();
                let split = [sevenths, vec![length - uneven]].concat();
                let late = [vec![12_345_678], split.clone()].concat();
                for (durations, start) in [(vec![length], 0), (split, 0), (late, 12_345_678)] {
                    let what = format!("tempo {tempo}, {off} from 3 s, {} frames", durations.len());
                    let timing = timing(&durations, start, start + length, Some(tempo)).unwrap();
                    let total: u32 = timing.ticks.iter().sum();
                    assert!((1..=MAX_FRAMES).contains(&total), "{what}: {total} ticks");
                    assert_eq!(timing.tempo, tempo);
                    // cut only when it rounds to more than 180 frames, and
                    // then at 180; otherwise every frame it rounds to
                    assert_eq!(timing.trimmed, ideal > most + 0.5, "{what}");
                    let expected = if timing.trimmed { most } else { ideal.round() };
                    assert_eq!(f64::from(total), expected, "{what}");
                    let used: u64 = timing.shown.iter().sum();
                    assert_eq!(used < length, timing.trimmed, "{what}");
                    assert_eq!(timing.shown[0] == 0, start > 0, "{what}");
                }
            }
        }
        // what the half frame of leeway let through, counted in input time
        let ticks = |ms: u64, tempo: f64| {
            let timing = timing(&[ms * 1_000_000], 0, ms * 1_000_000, Some(tempo)).unwrap();
            (timing.ticks[0], timing.trimmed)
        };
        assert_eq!(ticks(1505, 0.5), (180, true));
        assert_eq!(ticks(308, 0.1), (180, true));
        // and the 0.3 of a frame it called a cut
        assert_eq!(ticks(30_050, 10.0), (180, false));
        let slow = Options { tempo: Some(0.5), ..Options::default() };
        let (anim, report) = normalise(flat(&[(RED, ms(1505))]), &slow).unwrap();
        assert_eq!((anim.ticks(), report.ticks, report.trimmed), (180, 180, true));
    }

    #[test]
    fn fits_long_inputs_into_180_frames_when_no_tempo_is_asked() {
        let most = f64::from(MAX_FRAMES);
        for off in [-90.0, -0.6, -0.4, 0.0, 0.4, 0.6, 1.0, 180.0, 1e6] {
            let length = ((most + off) * NANOS as f64 / f64::from(FPS)).round() as u64;
            let thirds = [length / 2, length / 3, length - length / 2 - length / 3];
            let timing = timing(&thirds, 0, length, None).unwrap();
            let total: u32 = timing.ticks.iter().sum();
            // what rounds to 180 frames plays as it is, anything longer
            // as fast as makes it 180, and nothing is cut
            let fitted = off > 0.5;
            let expected = if fitted { most } else { (most + off).round() };
            assert_eq!((f64::from(total), timing.trimmed), (expected, false), "{off}");
            let tempo = if fitted { length as f64 / MAX_NANOS as f64 } else { 1.0 };
            assert_eq!(timing.tempo, tempo, "{off}");
            assert_eq!(timing.shown, thirds);
        }
    }

    #[test]
    fn times_frames_of_any_length() {
        // too short for a 60 fps frame together: the longest shows
        let short = timing(&[1_000_000, 3_000_000, 2_000_000], 0, 6_000_000, None).unwrap();
        assert_eq!(short.ticks, [0, 1, 0]);
        // no time at all gets no frames, wherever it is
        let none = timing(&[0, 50_000_000, 0, 50_000_000, 0], 0, 100_000_000, None).unwrap();
        assert_eq!(
            (none.shown, none.ticks),
            (vec![0, 50_000_000, 0, 50_000_000, 0], vec![0, 3, 0, 3, 0])
        );
        // and frames longer than the numbers hold neither overflow nor
        // count for more than 180 frames
        for tempo in [None, Some(0.1), Some(10.0)] {
            let long = timing(&[u64::MAX, u64::MAX], 0, u64::MAX, tempo).unwrap();
            assert_eq!((long.ticks, long.trimmed), (vec![180, 0], tempo.is_some()), "{tempo:?}");
        }
        let endless = flat(&[(RED, Duration::MAX), (BLUE, Duration::MAX)]);
        let (anim, report) = normalise(endless.clone(), &Options::default()).unwrap();
        assert_eq!((anim.ticks(), report.frames, report.trimmed), (180, 1, false));
        let late = Options { start: Duration::MAX, ..Options::default() };
        assert!(matches!(normalise(endless.clone(), &late), Err(Error::StartPastEnd { .. })));
    }

    #[test]
    fn takes_3_s_of_60_fps_frames_whole() {
        // 1/60 s in whole nanoseconds is a little longer
        let tick = Duration::from_secs_f64(1.0 / 60.0);
        let frames: Vec<_> =
            (0..180).map(|i| (if i % 2 == 0 { RED } else { BLUE }, tick)).collect();
        for tempo in [None, Some(1.0)] {
            let options = Options { tempo, ..Options::default() };
            let (anim, report) = normalise(flat(&frames), &options).unwrap();
            assert_eq!((report.ticks, report.tempo, report.trimmed), (180, 1.0, false));
            assert_eq!(anim.frames().len(), 180);
        }
    }

    #[test]
    fn uses_a_window() {
        let input = flat(&[(RED, ms(1500)), (GREEN, ms(1500)), (BLUE, ms(1500))]);
        let window = Options { start: ms(1200), length: Some(ms(600)), ..Options::default() };
        let (anim, _) = normalise(input.clone(), &window).unwrap();
        let ticks: Vec<_> = anim.frames().iter().map(|frame| frame.ticks).collect();
        assert_eq!(ticks, [18, 18]);

        let late = Options { start: ms(4500), ..Options::default() };
        assert!(matches!(normalise(input.clone(), &late), Err(Error::StartPastEnd { .. })));
        let empty = Options { length: Some(Duration::ZERO), ..Options::default() };
        assert_eq!(normalise(input.clone(), &empty), Err(Error::ZeroLength));
    }

    #[test]
    fn cuts_between_frames_a_time_near_them_is_meant_for() {
        // a third of a second each, as a frame folder at 3 fps times them
        let third = Duration::from_secs_f64(1.0 / 3.0);
        let input = flat(&[(RED, third), (GREEN, third), (BLUE, third)]);
        let used = |start: f64, length: f64| {
            let window = Options {
                start: Duration::from_secs_f64(start),
                length: Some(Duration::from_secs_f64(length)),
                ..Options::default()
            };
            let (anim, _) = normalise(input.clone(), &window).unwrap();
            let colours =
                anim.frames().iter().map(|frame| anim.palette()[frame.pixels[0] as usize]);
            (colours.collect::<Vec<_>>(), anim.ticks())
        };
        // the second frame alone, whole, however its times were written
        for (start, length) in [
            (1.0 / 3.0, 1.0 / 3.0),
            (0.333333, 0.333333),
            (0.33333, 0.33334),
            (0.3334, 0.3332),
            // as the GUI once wrote them, a hair inside
            (1.0 / 3.0 + 1e-6, 1.0 / 3.0 - 2e-6),
        ] {
            assert_eq!(used(start, length), (vec![GREEN], 20), "{start} {length}");
        }
        // and the first two, from the start
        assert_eq!(used(0.00004, 0.66663), (vec![RED, GREEN], 40));
        // further off is a cut into a frame, as asked
        assert_eq!(used(0.3, 0.4).0, [RED, GREEN, BLUE]);

        // no more than a quarter of the frames beside a cut
        let short = flat(&[(RED, ms(100)), (GREEN, Duration::from_micros(200)), (BLUE, ms(100))]);
        let near = |at: u64, ends| {
            let durations: Vec<u64> = short.frames().iter().map(|f| nanos(f.duration)).collect();
            on_edge(at, &durations, ends)
        };
        assert_eq!(near(100_040_000, false), 100_000_000);
        assert_eq!(near(99_960_000, true), 100_000_000);
        assert_eq!(near(100_060_000, false), 100_060_000);
        assert_eq!(near(99_920_000, false), 99_920_000);
        // the end is no frame's start, nor the start any frame's end
        assert_eq!(near(200_200_000 - 50_000, false), 200_150_000);
        assert_eq!(near(30_000, true), 30_000);
        assert_eq!(near(30_000, false), 0);
    }

    #[test]
    fn keeps_alpha_levels_apart() {
        let half = [255, 0, 0, 128];
        let input =
            animation(4, 1, &[ms(100)], |_, x, _| [RED, half, CLEAR, [9, 9, 9, 0]][x as usize]);
        let (anim, report) =
            normalise(input.clone(), &Options { keep_canvas: true, ..Options::default() }).unwrap();
        assert_eq!(report.colours, 2);
        assert_eq!(anim.palette(), [CLEAR, RED, half]);
        // both transparent pixels are one cell
        assert_eq!(anim.width(), 3);

        let invisible = animation(2, 2, &[ms(100)], |_, x, _| [x as u8, 0, 0, 0]);
        assert_eq!(normalise(invisible.clone(), &Options::default()), Err(Error::Invisible));
        // whether or not its canvas is kept
        let canvas = Options { keep_canvas: true, ..Options::default() };
        assert_eq!(normalise(invisible.clone(), &canvas), Err(Error::Invisible));
        // and so is a part of the input with nothing in it
        let corner = animation(4, 4, &[ms(100)], |_, x, y| if x + y == 0 { RED } else { CLEAR });
        let crop = Some(Rect { x: 2, y: 2, width: 2, height: 2 });
        for keep_canvas in [false, true] {
            let empty = Options { crop, keep_canvas, ..Options::default() };
            assert_eq!(normalise(corner.clone(), &empty), Err(Error::Invisible), "{keep_canvas}");
        }
        assert_eq!(normalise(corner.clone(), &canvas).unwrap().1.crop.width, 4);

        // a line of single pixels, which a forced scale of 4 snaps to cells
        // of three-quarters transparent
        let diagonal = animation(16, 16, &[ms(100)], |_, x, y| if x == y { RED } else { CLEAR });
        let forced = Options { pixel_scale: Some(4), ..Options::default() };
        assert_eq!(normalise(diagonal.clone(), &forced), Err(Error::Invisible));
        assert_eq!(normalise(diagonal.clone(), &Options::default()).unwrap().1.colours, 1);
        // dots at the far ends of a strip, which the square in its middle
        // leaves out
        let ends = animation(10, 2, &[ms(100)], |_, x, _| if x % 9 == 0 { RED } else { CLEAR });
        let square = Options { square: Square::Crop, ..Options::default() };
        assert_eq!(normalise(ends.clone(), &square), Err(Error::Invisible));
        assert_eq!(normalise(ends.clone(), &Options::default()).unwrap().1.width, 3);
    }

    #[test]
    fn warns_about_many_colours() {
        let input = animation(300, 1, &[ms(100)], |_, x, _| [x as u8, (x >> 8) as u8, 0, 255]);
        let (_, report) = normalise(input.clone(), &Options::default()).unwrap();
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
        let (anim, report) = normalise(input.clone(), &canvas).unwrap();
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
        assert_eq!(normalise(dot.clone(), &huge).unwrap().1.width, 1);

        // snapping one cell of 65536 different pixels stays quick
        let started = std::time::Instant::now();
        let whole = Options { pixel_scale: Some(256), ..canvas };
        let (anim, report) = normalise(input.clone(), &whole).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!((anim.width(), report.snapped_pixels), (1, 65535));
    }
}

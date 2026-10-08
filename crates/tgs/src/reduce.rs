//! Lossy reductions for animations that don't fit a sticker losslessly
//! (see "Fit" in `docs/tgs.md`), and how much each one changes.
//!
//! Every reduction keeps the crop and the length, so results compare with
//! the original pixel for pixel and frame for frame.

use std::collections::HashMap;

use serde::Serialize;

use crate::normalise::{Edges, Grid, PixelAnim, PixelFrame, assemble, snap_to};
use crate::{Error, Result};

/// One reduction at one strength.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "reduction", rename_all = "snake_case")]
pub enum Reduction {
    /// Snaps the art onto a pixel grid most of it follows.
    SnapToGrid { scale: u32 },
    /// Joins colours closer than `distance` in OKLab into the more common.
    MergeColours { distance: f64 },
    /// Joins neighbouring frames that differ in at most this share of the
    /// area.
    MergeFrames { changed: f64 },
    /// Drops this share of frames, the least different first, giving
    /// their time to the frame before.
    DropFrames { share: f64 },
    /// Replaces cells that match none of their 8 neighbours with the most
    /// common neighbouring colour.
    Despeckle,
    /// Scales the art down by `factor`.
    Downscale { factor: f64 },
}

/// The kinds of reductions, roughly from least to most visible.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    SnapToGrid,
    MergeColours,
    MergeFrames,
    DropFrames,
    Despeckle,
    Downscale,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::SnapToGrid,
        Kind::MergeColours,
        Kind::MergeFrames,
        Kind::DropFrames,
        Kind::Despeckle,
        Kind::Downscale,
    ];

    /// The strengths tried, weakest first. `likely_scale` is the scale
    /// snapping would use, if the art has one.
    pub fn ladder(self, likely_scale: Option<u32>) -> Vec<Reduction> {
        match self {
            Kind::SnapToGrid => {
                likely_scale.map(|scale| Reduction::SnapToGrid { scale }).into_iter().collect()
            }
            Kind::MergeColours => [0.02, 0.04, 0.06, 0.09, 0.13, 0.18, 0.25]
                .map(|distance| Reduction::MergeColours { distance })
                .to_vec(),
            Kind::MergeFrames => [0.002, 0.005, 0.01, 0.02, 0.04, 0.08, 0.16]
                .map(|changed| Reduction::MergeFrames { changed })
                .to_vec(),
            Kind::DropFrames => {
                [0.1, 0.2, 0.33, 0.5, 0.67].map(|share| Reduction::DropFrames { share }).to_vec()
            }
            Kind::Despeckle => vec![Reduction::Despeckle],
            Kind::Downscale => {
                [0.85, 0.7, 0.5, 0.35, 0.25].map(|factor| Reduction::Downscale { factor }).to_vec()
            }
        }
    }
}

impl Reduction {
    pub fn kind(&self) -> Kind {
        match self {
            Reduction::SnapToGrid { .. } => Kind::SnapToGrid,
            Reduction::MergeColours { .. } => Kind::MergeColours,
            Reduction::MergeFrames { .. } => Kind::MergeFrames,
            Reduction::DropFrames { .. } => Kind::DropFrames,
            Reduction::Despeckle => Kind::Despeckle,
            Reduction::Downscale { .. } => Kind::Downscale,
        }
    }

    /// The same reduction at a strength between this one's and `weaker`'s
    /// (`share` 0 is `weaker`, 1 this one), for kinds that have one.
    pub fn between(&self, weaker: Option<&Reduction>, share: f64) -> Option<Reduction> {
        let mix = |from: f64, to: f64| from + (to - from) * share;
        Some(match (*self, weaker.copied()) {
            (Reduction::MergeColours { distance }, weaker) => Reduction::MergeColours {
                distance: mix(
                    match weaker {
                        Some(Reduction::MergeColours { distance }) => distance,
                        _ => 0.0,
                    },
                    distance,
                ),
            },
            (Reduction::MergeFrames { changed }, weaker) => Reduction::MergeFrames {
                changed: mix(
                    match weaker {
                        Some(Reduction::MergeFrames { changed }) => changed,
                        _ => 0.0,
                    },
                    changed,
                ),
            },
            (Reduction::DropFrames { share: strength }, weaker) => Reduction::DropFrames {
                share: mix(
                    match weaker {
                        Some(Reduction::DropFrames { share }) => share,
                        _ => 0.0,
                    },
                    strength,
                ),
            },
            (Reduction::Downscale { factor }, weaker) => Reduction::Downscale {
                factor: mix(
                    match weaker {
                        Some(Reduction::Downscale { factor }) => factor,
                        _ => 1.0,
                    },
                    factor,
                ),
            },
            _ => return None,
        })
    }

    pub fn apply(&self, anim: &PixelAnim) -> Result<PixelAnim> {
        match *self {
            Reduction::SnapToGrid { scale } => snap_to_grid(anim, scale),
            Reduction::MergeColours { distance } => Ok(merge_colours(anim, distance)),
            Reduction::MergeFrames { changed } => Ok(merge_frames(anim, changed)),
            Reduction::DropFrames { share } => Ok(drop_frames(anim, share)),
            Reduction::Despeckle => Ok(despeckle(anim)),
            Reduction::Downscale { factor } => downscale(anim, factor),
        }
    }
}

/// A colour in OKLab, scaled by its alpha.
fn oklab([r, g, b, _]: [u8; 4]) -> [f64; 3] {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    ]
}

/// How different two colours look: OKLab distance where both show, plus
/// the difference in opacity (1 between transparent and opaque).
pub fn distance(a: [u8; 4], b: [u8; 4]) -> f64 {
    if a[3] == 0 && b[3] == 0 {
        return 0.0;
    }
    let alpha = f64::from(a[3].abs_diff(b[3])) / 255.0;
    let shown = f64::from(a[3].min(b[3])) / 255.0;
    let (a, b) = (oklab(a), oklab(b));
    let colour = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    alpha + colour * shown
}

/// Where each frame starts, and the total.
fn starts(anim: &PixelAnim) -> Vec<u32> {
    let mut starts = vec![0];
    for frame in &anim.frames {
        starts.push(starts.last().unwrap() + frame.ticks);
    }
    starts
}

/// How much `reduced` differs from `original`: the [`distance`] of every
/// input pixel at every 60 fps frame, averaged.
pub fn error(original: &PixelAnim, reduced: &PixelAnim) -> f64 {
    // the edges of both grids together; each piece is in one cell of each
    let overlay = |a: &[u32], b: &[u32]| {
        let mut edges: Vec<u32> = a.iter().chain(b).copied().collect();
        edges.sort_unstable();
        edges.dedup();
        // per piece: its size and the cell it falls in, in either grid
        edges
            .windows(2)
            .map(|piece| {
                let cell = |grid: &[u32]| grid.partition_point(|&edge| edge <= piece[0]) - 1;
                (piece[1] - piece[0], cell(a), cell(b))
            })
            .collect::<Vec<_>>()
    };
    let columns = overlay(&original.grid.columns, &reduced.grid.columns);
    let rows = overlay(&original.grid.rows, &reduced.grid.rows);
    let (before, after) = (starts(original), starts(reduced));
    let mut ticks: Vec<u32> = before.iter().chain(&after).copied().collect();
    ticks.sort_unstable();
    ticks.dedup();
    let frame_at = |starts: &[u32], tick: u32| starts.partition_point(|&start| start <= tick) - 1;

    let mut distances: HashMap<(u16, u16), f64> = HashMap::new();
    let mut total = 0.0;
    for span in ticks.windows(2) {
        let a = &original.frames[frame_at(&before, span[0])].pixels;
        let b = &reduced.frames[frame_at(&after, span[0])].pixels;
        let mut sum = 0.0;
        for &(height, row_a, row_b) in &rows {
            for &(width, column_a, column_b) in &columns {
                let ca = a[row_a * original.width as usize + column_a];
                let cb = b[row_b * reduced.width as usize + column_b];
                if original.palette[ca as usize] == reduced.palette[cb as usize] {
                    continue;
                }
                let d = *distances.entry((ca, cb)).or_insert_with(|| {
                    distance(original.palette[ca as usize], reduced.palette[cb as usize])
                });
                sum += d * f64::from(width) * f64::from(height);
            }
        }
        total += sum * f64::from(span[1] - span[0]);
    }
    let area = f64::from(*original.grid.columns.last().unwrap())
        * f64::from(*original.grid.rows.last().unwrap());
    total / (area * f64::from(*before.last().unwrap()))
}

/// Joins identical neighbouring frames, drops unused colours and joins
/// cell columns and rows that became equal.
fn compact(anim: PixelAnim) -> PixelAnim {
    let mut frames: Vec<PixelFrame> = Vec::with_capacity(anim.frames.len());
    for frame in anim.frames {
        match frames.last_mut() {
            Some(last) if last.pixels == frame.pixels => last.ticks += frame.ticks,
            _ => frames.push(frame),
        }
    }
    // colours in use, transparent staying first
    let mut used = vec![false; anim.palette.len()];
    used[0] = true;
    for frame in &frames {
        for &colour in &frame.pixels {
            used[colour as usize] = true;
        }
    }
    let mut renumber = vec![0u16; anim.palette.len()];
    let mut palette = Vec::new();
    for (colour, &kept) in used.iter().enumerate() {
        if kept {
            renumber[colour] = palette.len() as u16;
            palette.push(anim.palette[colour]);
        }
    }
    // cells as pixels, to find the columns and rows that can join
    let cells: Vec<Vec<u32>> = frames
        .iter()
        .map(|frame| frame.pixels.iter().map(|&c| u32::from(renumber[c as usize])).collect())
        .collect();
    let ticks: Vec<u32> = frames.iter().map(|frame| frame.ticks).collect();
    let edges = Edges::count(&cells, anim.width, anim.height);
    let joined = assemble(&cells, &ticks, anim.width, &edges)
        .expect("fewer colours than before can't overflow the palette");
    // `joined` has its colours in order of appearance; map back to ours
    let pick = |edges: &[u32], within: &[u32]| edges.iter().map(|&e| within[e as usize]).collect();
    PixelAnim {
        width: joined.width,
        height: joined.height,
        grid: Grid {
            columns: pick(&joined.grid.columns, &anim.grid.columns),
            rows: pick(&joined.grid.rows, &anim.grid.rows),
        },
        palette: joined
            .palette
            .iter()
            .map(|&packed| palette[u32::from_le_bytes(packed) as usize])
            .collect(),
        frames: joined.frames,
    }
}

fn merge_colours(anim: &PixelAnim, threshold: f64) -> PixelAnim {
    let mut counts = vec![0usize; anim.palette.len()];
    for frame in &anim.frames {
        for &colour in &frame.pixels {
            counts[colour as usize] += frame.ticks as usize;
        }
    }
    // the most used colours first; each joins the first kept one near it
    let mut order: Vec<usize> = (1..anim.palette.len()).collect();
    order.sort_by_key(|&colour| std::cmp::Reverse(counts[colour]));
    // palette indices fit u16, the palette's length may not
    let mut target: Vec<u16> = (0..anim.palette.len()).map(|colour| colour as u16).collect();
    let mut kept: Vec<usize> = Vec::new();
    for colour in order {
        let near = kept
            .iter()
            .copied()
            .find(|&other| distance(anim.palette[colour], anim.palette[other]) < threshold);
        match near {
            Some(other) => target[colour] = other as u16,
            None => kept.push(colour),
        }
    }
    let frames = anim
        .frames
        .iter()
        .map(|frame| PixelFrame {
            pixels: frame.pixels.iter().map(|&c| target[c as usize]).collect(),
            ticks: frame.ticks,
        })
        .collect();
    compact(PixelAnim { frames, ..anim.clone() })
}

/// The share of the area that differs between two frames.
fn changed(anim: &PixelAnim, a: &[u16], b: &[u16]) -> f64 {
    let (columns, rows) = (&anim.grid.columns, &anim.grid.rows);
    let mut area = 0u64;
    for (y, row) in rows.windows(2).enumerate() {
        for (x, column) in columns.windows(2).enumerate() {
            let index = y * anim.width as usize + x;
            if a[index] != b[index] {
                area += u64::from(column[1] - column[0]) * u64::from(row[1] - row[0]);
            }
        }
    }
    area as f64 / (f64::from(*columns.last().unwrap()) * f64::from(*rows.last().unwrap()))
}

fn merge_frames(anim: &PixelAnim, threshold: f64) -> PixelAnim {
    let mut frames: Vec<PixelFrame> = Vec::with_capacity(anim.frames.len());
    for frame in &anim.frames {
        match frames.last_mut() {
            Some(last) if changed(anim, &last.pixels, &frame.pixels) <= threshold => {
                last.ticks += frame.ticks;
            }
            _ => frames.push(frame.clone()),
        }
    }
    compact(PixelAnim { frames, ..anim.clone() })
}

fn drop_frames(anim: &PixelAnim, share: f64) -> PixelAnim {
    let count = anim.frames.len();
    if count < 2 {
        return anim.clone();
    }
    let drop = ((count as f64 * share).round() as usize).clamp(1, count - 1);
    // the first frame stays: it is the sticker's preview
    let mut cost: Vec<(f64, usize)> = (1..count)
        .map(|i| {
            let difference = changed(anim, &anim.frames[i - 1].pixels, &anim.frames[i].pixels);
            (difference * f64::from(anim.frames[i].ticks), i)
        })
        .collect();
    cost.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut dropped = vec![false; count];
    for &(_, i) in &cost[..drop] {
        dropped[i] = true;
    }
    let mut frames: Vec<PixelFrame> = Vec::with_capacity(count - drop);
    for (frame, gone) in anim.frames.iter().zip(dropped) {
        match frames.last_mut() {
            Some(last) if gone => last.ticks += frame.ticks,
            _ => frames.push(frame.clone()),
        }
    }
    compact(PixelAnim { frames, ..anim.clone() })
}

fn despeckle(anim: &PixelAnim) -> PixelAnim {
    let (w, h) = (anim.width as usize, anim.height as usize);
    let frames = anim
        .frames
        .iter()
        .map(|frame| {
            let mut pixels = frame.pixels.clone();
            for y in 0..h {
                for x in 0..w {
                    let colour = frame.pixels[y * w + x];
                    if colour == 0 {
                        continue;
                    }
                    let mut neighbours: Vec<u16> = Vec::with_capacity(8);
                    for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                            if (nx, ny) != (x, y) {
                                neighbours.push(frame.pixels[ny * w + nx]);
                            }
                        }
                    }
                    if neighbours.contains(&colour) {
                        continue;
                    }
                    // the most common neighbour, the smallest index on ties
                    neighbours.sort_unstable();
                    let common = neighbours
                        .chunk_by(|a, b| a == b)
                        .map(|run| (run.len(), std::cmp::Reverse(run[0])))
                        .max()
                        .map(|(_, std::cmp::Reverse(colour))| colour);
                    if let Some(common) = common.filter(|&c| c != 0) {
                        pixels[y * w + x] = common;
                    }
                }
            }
            PixelFrame { pixels, ticks: frame.ticks }
        })
        .collect();
    compact(PixelAnim { frames, ..anim.clone() })
}

/// Frames as packed pixels over the whole crop.
fn expand(anim: &PixelAnim) -> Vec<Vec<u32>> {
    (0..anim.frames.len())
        .map(|frame| {
            anim.rgba(frame).as_chunks::<4>().0.iter().map(|&p| u32::from_le_bytes(p)).collect()
        })
        .collect()
}

/// Snaps the pixels to cells with these edges and turns them into cells.
fn snapped(anim: &PixelAnim, columns: &[u32], rows: &[u32]) -> Result<PixelAnim> {
    let width = *anim.grid.columns.last().unwrap();
    let height = *anim.grid.rows.last().unwrap();
    let mut pixels = expand(anim);
    for frame in &mut pixels {
        snap_to(frame, width, columns, rows);
    }
    let ticks: Vec<u32> = anim.frames.iter().map(|frame| frame.ticks).collect();
    let edges = Edges::count(&pixels, width, height);
    Ok(compact(assemble(&pixels, &ticks, width, &edges)?))
}

fn snap_to_grid(anim: &PixelAnim, scale: u32) -> Result<PixelAnim> {
    let width = *anim.grid.columns.last().unwrap();
    let height = *anim.grid.rows.last().unwrap();
    let pixels = expand(anim);
    let edges = Edges::count(&pixels, width, height);
    let offset = edges.fit(scale).offset;
    let starts = |offset: u32, size: u32| {
        let mut starts = vec![0];
        starts.extend((offset % scale..size).step_by(scale as usize).filter(|&start| start > 0));
        starts.push(size);
        starts
    };
    snapped(anim, &starts(offset.0, width), &starts(offset.1, height))
}

fn downscale(anim: &PixelAnim, factor: f64) -> Result<PixelAnim> {
    if !(factor > 0.0 && factor < 1.0) {
        return Err(Error::ZeroScale);
    }
    let width = *anim.grid.columns.last().unwrap();
    let height = *anim.grid.rows.last().unwrap();
    // the art's own pixels, grown by 1 / factor
    let step = f64::from(anim.grid.scale().max(1)) / factor;
    let edges = |size: u32| {
        let mut edges: Vec<u32> =
            (0..).map(|i| (f64::from(i) * step).round() as u32).take_while(|&e| e < size).collect();
        edges.dedup();
        edges.push(size);
        edges
    };
    snapped(anim, &edges(width), &edges(height))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tgradish_frames::{Animation, Frame};

    use super::*;
    use crate::normalise::{Options, normalise};

    fn anim(width: u32, height: u32, frames: &[(&[[u8; 4]], u64)]) -> PixelAnim {
        let frames = frames
            .iter()
            .map(|(pixels, ms)| Frame {
                rgba: pixels.iter().flatten().copied().collect(),
                duration: Duration::from_millis(*ms),
            })
            .collect();
        let input = Animation::new(width, height, frames).unwrap();
        normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap().0
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const NEAR_RED: [u8; 4] = [250, 4, 2, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0; 4];

    #[test]
    fn measures_differences() {
        assert_eq!(distance(CLEAR, [9, 9, 9, 0]), 0.0);
        assert_eq!(distance(CLEAR, RED), 1.0);
        assert!(distance(RED, NEAR_RED) < 0.02);
        assert!(distance(RED, BLUE) > 0.3);
        let a = anim(2, 1, &[(&[RED, BLUE], 100), (&[RED, BLUE], 100)]);
        assert_eq!(error(&a, &a), 0.0);
        // half the area changes for half the time: a quarter of the
        // distance, on average
        let b = anim(2, 1, &[(&[RED, BLUE], 100), (&[RED, RED], 100)]);
        assert!((error(&a, &b) - distance(BLUE, RED) / 4.0).abs() < 1e-9);
    }

    #[test]
    fn reduces() {
        let a = anim(3, 1, &[(&[RED, NEAR_RED, BLUE], 100), (&[RED, NEAR_RED, CLEAR], 100)]);
        // close colours join into the more common one
        let merged = Reduction::MergeColours { distance: 0.05 }.apply(&a).unwrap();
        assert_eq!(merged.palette().len(), 3);
        assert_eq!(merged.rgba(0), [RED, RED, BLUE].concat());
        // and then the two red columns are one cell
        assert_eq!(merged.width(), 2);
        assert!(error(&a, &merged) > 0.0);

        // a third of the area changed: merging at a third keeps the first
        let frames = Reduction::MergeFrames { changed: 0.34 }.apply(&a).unwrap();
        assert_eq!(frames.frames().len(), 1);
        assert_eq!(frames.ticks(), a.ticks());
        assert_eq!(Reduction::MergeFrames { changed: 0.3 }.apply(&a).unwrap().frames().len(), 2);

        let dropped = Reduction::DropFrames { share: 0.5 }.apply(&a).unwrap();
        assert_eq!((dropped.frames().len(), dropped.ticks()), (1, 12));
        // a still image has nothing to drop
        let still = anim(1, 1, &[(&[RED], 100)]);
        assert_eq!(Reduction::DropFrames { share: 0.5 }.apply(&still).unwrap(), still);

        // a lone blue pixel among red becomes red
        let speck = anim(3, 3, &[(&[RED, RED, RED, RED, BLUE, RED, RED, RED, RED], 100)]);
        let clean = Reduction::Despeckle.apply(&speck).unwrap();
        assert_eq!(clean.rgba(0), [RED; 9].concat());

        // 4x4 in 2x2 quadrants (red, blue / blue, red), each with one pixel
        // of the other colour; halved, the odd pixels go
        let quadrants: Vec<[u8; 4]> = (0..16)
            .map(|i| {
                let (x, y) = (i % 4, i / 4);
                let red = (x / 2 + y / 2) % 2 == 0;
                if (x % 2, y % 2) == (0, 0) {
                    if red { BLUE } else { RED }
                } else if red {
                    RED
                } else {
                    BLUE
                }
            })
            .collect();
        let board = anim(4, 4, &[(&quadrants, 100)]);
        let half = Reduction::Downscale { factor: 0.5 }.apply(&board).unwrap();
        assert_eq!(half.grid().columns, [0, 2, 4]);
        assert_eq!(half.rgba(0)[..4], RED);
        assert!(error(&board, &half) > 0.0);
        assert_eq!(half.ticks(), board.ticks());
    }

    #[test]
    fn snaps_to_a_likely_grid() {
        // 2x art with one pixel off the grid
        let mut pixels = vec![RED; 16];
        for i in [2, 3, 6, 7] {
            pixels[i] = BLUE;
        }
        pixels[5] = BLUE;
        let off = anim(4, 4, &[(&pixels, 100)]);
        let snapped = Reduction::SnapToGrid { scale: 2 }.apply(&off).unwrap();
        assert_eq!(snapped.grid().scale(), 2);
        assert!(error(&off, &snapped) > 0.0);
    }
}

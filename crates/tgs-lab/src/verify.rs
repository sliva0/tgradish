//! Renders Lottie made from a [`PixelAnim`] with tlottie and compares it
//! with the art: colours at cell centres, and seams (see `docs/tgs.md`).

use anyhow::{Result, anyhow};
use tgradish_tgs::layout::Placement;
use tgradish_tgs::limits::telegram::CANVAS;
use tgradish_tgs::normalise::PixelAnim;
use tlottie::{CPURenderer, Composition, Limits, RenderOptions};

pub enum Renderer {
    Tlottie(Box<CPURenderer>),
    #[cfg(feature = "rlottie")]
    Rlottie(rlottie::Animation),
}

impl Renderer {
    pub fn new(json: &[u8]) -> Result<Renderer> {
        let composition =
            Composition::parse(json, &Limits::default()).map_err(|err| anyhow!("{err}"))?;
        Ok(Renderer::Tlottie(Box::new(CPURenderer::new(composition))))
    }

    /// rlottie, built from Telegram's fork, which older clients use.
    #[cfg(feature = "rlottie")]
    pub fn rlottie(json: &[u8]) -> Result<Renderer> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // rlottie caches animations by key, so every load gets its own
        static LOADS: AtomicUsize = AtomicUsize::new(0);
        let key = format!("tgs-lab-{}", LOADS.fetch_add(1, Ordering::Relaxed));
        rlottie::Animation::from_data(json, key, "")
            .map(Renderer::Rlottie)
            .ok_or_else(|| anyhow!("rlottie can't parse the animation"))
    }

    /// Every renderer this build has, by name.
    pub fn all(json: &[u8]) -> Result<Vec<(&'static str, Renderer)>> {
        #[cfg_attr(not(feature = "rlottie"), allow(unused_mut))]
        let mut out = vec![("tlottie", Renderer::new(json)?)];
        #[cfg(feature = "rlottie")]
        out.push(("rlottie", Renderer::rlottie(json)?));
        Ok(out)
    }

    /// Premultiplied RGBA, row by row.
    pub fn render(&mut self, tick: u32, size: u32) -> Result<Vec<[u8; 4]>> {
        match self {
            Renderer::Tlottie(renderer) => {
                let mut pixels = vec![0u32; size as usize * size as usize];
                renderer
                    .render(tick as f32, &mut pixels, size, size, RenderOptions::default())
                    .map_err(|err| anyhow!("{err}"))?;
                Ok(pixels.into_iter().map(u32::to_le_bytes).collect())
            }
            #[cfg(feature = "rlottie")]
            Renderer::Rlottie(animation) => {
                let mut surface =
                    rlottie::Surface::new(rlottie::Size::new(size as usize, size as usize));
                animation.render(tick as usize, &mut surface);
                Ok(surface
                    .data()
                    .iter()
                    .map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
                    .collect())
            }
        }
    }
}

pub fn straight([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let channel = |c: u8| match a {
        0 => 0,
        255 => c,
        _ => ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
    };
    [channel(r), channel(g), channel(b), a]
}

/// When each frame of `anim` starts, in 60 fps frames.
fn starts(anim: &PixelAnim) -> Vec<u32> {
    let mut start = 0;
    anim.frames()
        .iter()
        .map(|frame| {
            start += frame.ticks;
            start - frame.ticks
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Miss {
    pub tick: u32,
    pub cell: (u32, u32),
    pub expected: [u8; 4],
    pub got: [u8; 4],
}

#[derive(Debug, Default)]
pub struct Centres {
    pub checked: usize,
    /// Cells smaller than a canvas pixel, which can't be sampled cleanly.
    pub skipped: usize,
    pub misses: usize,
    /// The first few misses.
    pub examples: Vec<Miss>,
}

/// Anti-aliasing noise allowed at cell centres, in 8-bit levels.
const NOISE: u8 = 6;

/// Renders every frame at 512x512 and reads the pixel at the centre of each
/// cell, which must match within [`NOISE`].
pub fn centres(renderer: &mut Renderer, anim: &PixelAnim) -> Result<Centres> {
    let placement = Placement::new(anim.grid());
    let mut out = Centres::default();
    for (frame, tick) in anim.frames().iter().zip(starts(anim)) {
        let pixels = renderer.render(tick, CANVAS)?;
        for row in 0..anim.height() {
            for column in 0..anim.width() {
                let [x0, y0] = placement.canvas(column, row);
                let [x1, y1] = placement.canvas(column + 1, row + 1);
                let (x, y) = (((x0 + x1) / 2.0).floor(), ((y0 + y1) / 2.0).floor());
                if x < x0 || x + 1.0 > x1 || y < y0 || y + 1.0 > y1 {
                    out.skipped += 1;
                    continue;
                }
                out.checked += 1;
                let index = (row * anim.width() + column) as usize;
                let expected = anim.palette()[frame.pixels[index] as usize];
                let got = straight(pixels[y as usize * CANVAS as usize + x as usize]);
                let close = |a: u8, b: u8, by: u8| a.abs_diff(b) <= by;
                // rlottie's anti-aliasing leaks up to 5 levels into
                // neighbours of cells smaller than about two canvas pixels
                let same = match expected[3] {
                    0 => got[3] <= NOISE,
                    alpha => {
                        close(got[3], alpha, NOISE)
                            && (0..3).all(|i| close(got[i], expected[i], NOISE))
                    }
                };
                if !same {
                    out.misses += 1;
                    if out.examples.len() < 10 {
                        out.examples.push(Miss { tick, cell: (column, row), expected, got });
                    }
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Default)]
pub struct Seams {
    /// Pixels fully inside the art that came out less than 90% opaque.
    pub leaks: usize,
    /// Pixels fully inside the art with a channel off by more than 40.
    pub fringes: usize,
    /// Mean of the largest channel error over all pixels.
    pub mean_error: f64,
}

/// For each output pixel, the cells it overlaps along one axis and by how
/// much (fractions of the pixel).
fn overlaps(edges: &[f64], size: u32) -> Vec<Vec<(usize, f64)>> {
    let pixel = f64::from(CANVAS) / f64::from(size);
    (0..size)
        .map(|i| {
            let (from, to) = (f64::from(i) * pixel, f64::from(i + 1) * pixel);
            edges
                .windows(2)
                .enumerate()
                .filter_map(|(cell, edge)| {
                    let overlap = to.min(edge[1]) - from.max(edge[0]);
                    (overlap > 0.0).then_some((cell, overlap / pixel))
                })
                .collect()
        })
        .collect()
}

const MAGENTA: [f64; 3] = [255.0, 0.0, 255.0];

/// An exact area-averaged render of a frame: premultiplied channels 0-255
/// and alpha 0-1, row by row.
fn ideal(anim: &PixelAnim, frame: usize, size: u32) -> Vec<[f64; 4]> {
    let placement = Placement::new(anim.grid());
    let columns: Vec<f64> = (0..=anim.width()).map(|c| placement.canvas(c, 0)[0]).collect();
    let rows: Vec<f64> = (0..=anim.height()).map(|r| placement.canvas(0, r)[1]).collect();
    let palette: Vec<[f64; 4]> = anim
        .palette()
        .iter()
        .map(|&[r, g, b, a]| {
            let a = f64::from(a) / 255.0;
            [f64::from(r) * a, f64::from(g) * a, f64::from(b) * a, a]
        })
        .collect();
    let pixels = &anim.frames()[frame].pixels;
    let (along_x, along_y) = (overlaps(&columns, size), overlaps(&rows, size));
    let mut out = Vec::with_capacity(size as usize * size as usize);
    for row_cells in &along_y {
        for column_cells in &along_x {
            let mut sum = [0.0; 4];
            for &(row, wy) in row_cells {
                for &(column, wx) in column_cells {
                    let paint = palette[pixels[row * anim.width() as usize + column] as usize];
                    for i in 0..4 {
                        sum[i] += paint[i] * wx * wy;
                    }
                }
            }
            out.push(sum);
        }
    }
    out
}

/// Composites premultiplied RGBA (0-255 channels, 0-1 alpha) over magenta.
fn over_magenta(pixel: [f64; 4]) -> [f64; 3] {
    [0, 1, 2].map(|i| pixel[i] + MAGENTA[i] * (1.0 - pixel[3]))
}

fn as_float(pixel: [u8; 4]) -> [f64; 4] {
    [f64::from(pixel[0]), f64::from(pixel[1]), f64::from(pixel[2]), f64::from(pixel[3]) / 255.0]
}

fn largest_error(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max)
}

/// Renders each frame (or the `only` ones) at every size, over magenta,
/// and compares it with an exact area-averaged render of the art, like the
/// planning prototype's `seams.py`.
pub fn seams(
    renderer: &mut Renderer,
    anim: &PixelAnim,
    sizes: &[u32],
    only: Option<&[usize]>,
) -> Result<Seams> {
    let mut out = Seams::default();
    let mut pixels_seen = 0usize;
    for size in sizes.iter().copied() {
        for (index, tick) in starts(anim).into_iter().enumerate() {
            if only.is_some_and(|only| !only.contains(&index)) {
                continue;
            }
            let rendered = renderer.render(tick, size)?;
            for (want, &got) in ideal(anim, index, size).into_iter().zip(&rendered) {
                let got = as_float(got);
                let error = largest_error(over_magenta(want), over_magenta(got));
                out.mean_error += error;
                pixels_seen += 1;
                if want[3] > 0.999 {
                    out.leaks += usize::from(got[3] < 0.9);
                    out.fringes += usize::from(error > 40.0);
                }
            }
        }
    }
    out.mean_error /= pixels_seen.max(1) as f64;
    Ok(out)
}

/// The first `count` fringe pixels of a frame: where they are, the ideal
/// and rendered colour over magenta, and the cells they overlap.
pub fn explain(
    renderer: &mut Renderer,
    anim: &PixelAnim,
    frame: usize,
    size: u32,
    count: usize,
) -> Result<Vec<String>> {
    let rendered = renderer.render(starts(anim)[frame], size)?;
    let placement = Placement::new(anim.grid());
    let columns: Vec<f64> = (0..=anim.width()).map(|c| placement.canvas(c, 0)[0]).collect();
    let rows: Vec<f64> = (0..=anim.height()).map(|r| placement.canvas(0, r)[1]).collect();
    let (along_x, along_y) = (overlaps(&columns, size), overlaps(&rows, size));
    let pixels = &anim.frames()[frame].pixels;
    let mut out = Vec::new();
    for (index, want) in ideal(anim, frame, size).into_iter().enumerate() {
        let got = as_float(rendered[index]);
        let (want_rgb, got_rgb) = (over_magenta(want), over_magenta(got));
        if want[3] <= 0.999 || largest_error(want_rgb, got_rgb) <= 40.0 {
            continue;
        }
        let (x, y) = (index % size as usize, index / size as usize);
        let mut cells = Vec::new();
        for &(row, wy) in &along_y[y] {
            for &(column, wx) in &along_x[x] {
                let colour = pixels[row * anim.width() as usize + column];
                cells.push(format!("({column},{row}) colour {colour} {:.2}", wx * wy));
            }
        }
        let rgb = |c: [f64; 3]| format!("{:.0},{:.0},{:.0}", c[0], c[1], c[2]);
        out.push(format!(
            "pixel {x},{y}: ideal {} got {}; {}",
            rgb(want_rgb),
            rgb(got_rgb),
            cells.join(", ")
        ));
        if out.len() == count {
            break;
        }
    }
    Ok(out)
}

/// Three renders of a frame side by side, over magenta: the renderer's,
/// the ideal one, and the renderer's with fringes in green and leaks in
/// cyan. Straight RGBA, `3 * size` wide.
pub fn picture(
    renderer: &mut Renderer,
    anim: &PixelAnim,
    frame: usize,
    size: u32,
) -> Result<Vec<[u8; 4]>> {
    let rendered = renderer.render(starts(anim)[frame], size)?;
    let ideal = ideal(anim, frame, size);
    let byte = |c: [f64; 3]| [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, 255];
    let mut out = vec![[0; 4]; 3 * size as usize * size as usize];
    for y in 0..size as usize {
        for x in 0..size as usize {
            let (want, got) =
                (ideal[y * size as usize + x], as_float(rendered[y * size as usize + x]));
            let row = y * 3 * size as usize;
            out[row + x] = byte(over_magenta(got));
            out[row + size as usize + x] = byte(over_magenta(want));
            let interior = want[3] > 0.999;
            out[row + 2 * size as usize + x] = if interior && got[3] < 0.9 {
                [0, 255, 255, 255]
            } else if interior && largest_error(over_magenta(want), over_magenta(got)) > 40.0 {
                [0, 255, 0, 255]
            } else {
                byte(over_magenta(got))
            };
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tgradish_tgs::frames::{Animation, Frame};
    use tgradish_tgs::layout::lay_out;
    use tgradish_tgs::lottie::Style;
    use tgradish_tgs::normalise::{Options, normalise};
    use tgradish_tgs::scene::{FillRule, Group, Layer, Scene, Shape};

    use super::*;

    /// A red 6x6 ring around a 2x2 hole, with a translucent blue 1x2 bar in
    /// the hole's left half and a green notch on the outside right.
    fn ring() -> PixelAnim {
        let rgba = (0..36u32)
            .flat_map(|i| match (i % 6, i / 6) {
                (2, 2..=3) => [0, 0, 255, 100],
                (3, 2..=3) => [0; 4],
                (5, 1) => [0, 255, 0, 255],
                _ => [255, 0, 0, 255],
            })
            .collect();
        let frame = Frame { rgba, duration: Duration::from_millis(100) };
        let input = Animation::new(6, 6, vec![frame]).unwrap();
        normalise(&input, &Options { keep_canvas: true, ..Options::default() }).unwrap().0
    }

    fn render_and_compare(anim: &PixelAnim, groups: Vec<Group>) -> usize {
        let scene = Scene {
            width: anim.width(),
            height: anim.height(),
            ticks: anim.ticks(),
            layers: vec![Layer { from: 0, to: anim.ticks(), groups }],
        };
        assert_eq!(scene.compare(anim), None, "the scene itself is wrong");
        let json = lay_out(&scene, anim, None).to_json(Style::default());
        let mut misses = 0;
        for (name, mut renderer) in Renderer::all(json.as_bytes()).unwrap() {
            let result = centres(&mut renderer, anim).unwrap();
            assert_eq!(result.skipped, 0);
            if result.misses > 0 {
                eprintln!("{name}: {:?}", result.examples);
            }
            misses += result.misses;
        }
        misses
    }

    #[test]
    fn renders_paths_like_the_scene() {
        let anim = ring();
        // cells: columns and rows 0, 1, 2, 3, 4-5 are joined only where equal
        let (w, h) = (anim.width(), anim.height());
        let colour = |rgba: [u8; 4]| anim.palette().iter().position(|&c| c == rgba).unwrap() as u16;
        let (red, green, blue) =
            (colour([255, 0, 0, 255]), colour([0, 255, 0, 255]), colour([0, 0, 255, 100]));
        let column =
            |x: u32| anim.grid().columns.iter().position(|&edge| edge == x).unwrap() as u32;
        let row = |y: u32| anim.grid().rows.iter().position(|&edge| edge == y).unwrap() as u32;
        let (left, right, top, bottom) = (column(2), column(4), row(2), row(4));
        let outline = |points: &[(u32, u32)]| Shape::Path(points.to_vec());
        let outer = outline(&[(0, 0), (w, 0), (w, h), (0, h)]);
        let hole_ccw = outline(&[(left, top), (left, bottom), (right, bottom), (right, top)]);
        let hole_cw = outline(&[(left, top), (right, top), (right, bottom), (left, bottom)]);
        let extras = |groups: &mut Vec<Group>| {
            groups.push(Group {
                colour: blue,
                rule: FillRule::NonZero,
                shapes: vec![Shape::Rect { x: left, y: top, width: 1, height: bottom - top }],
            });
            groups.push(Group {
                colour: green,
                rule: FillRule::NonZero,
                shapes: vec![Shape::Rect { x: column(5), y: row(1), width: 1, height: 1 }],
            });
        };

        // non-zero: a counter-clockwise outline cuts a hole
        let mut groups = vec![Group {
            colour: red,
            rule: FillRule::NonZero,
            shapes: vec![outer.clone(), hole_ccw],
        }];
        extras(&mut groups);
        assert_eq!(render_and_compare(&anim, groups), 0);

        // even-odd: any second outline cuts a hole
        let mut groups =
            vec![Group { colour: red, rule: FillRule::EvenOdd, shapes: vec![outer, hole_cw] }];
        extras(&mut groups);
        assert_eq!(render_and_compare(&anim, groups), 0);

        // rectangles and a clockwise path add up in one fill
        let top_band = Shape::Rect { x: 0, y: 0, width: w, height: top };
        let bottom_band = Shape::Rect { x: 0, y: bottom, width: w, height: h - bottom };
        let sides = outline(&[(0, top), (left, top), (left, bottom), (0, bottom)]);
        let right_side = outline(&[(right, top), (w, top), (w, bottom), (right, bottom)]);
        let mut groups = vec![Group {
            colour: red,
            rule: FillRule::NonZero,
            shapes: vec![top_band, bottom_band, sides, right_side],
        }];
        extras(&mut groups);
        assert_eq!(render_and_compare(&anim, groups), 0);
    }

    /// A red 12x12 square with a green 6x6 square inside it.
    fn nested() -> PixelAnim {
        let rgba = (0..144u32)
            .flat_map(|i| match (i % 12, i / 12) {
                (3..=8, 3..=8) => [0, 255, 0, 255],
                _ => [255, 0, 0, 255],
            })
            .collect();
        let frame = Frame { rgba, duration: Duration::from_millis(100) };
        let input = Animation::new(12, 12, vec![frame]).unwrap();
        normalise(&input, &Options::default()).unwrap().0
    }

    fn seams_of(anim: &PixelAnim, scene: &Scene) -> Vec<(&'static str, Seams)> {
        assert_eq!(scene.compare(anim), None);
        let json = lay_out(scene, anim, None).to_json(Style::default());
        Renderer::all(json.as_bytes())
            .unwrap()
            .into_iter()
            .map(|(name, mut renderer)| {
                (name, seams(&mut renderer, anim, &[100, 237, 512], None).unwrap())
            })
            .collect()
    }

    #[test]
    fn the_seam_invariant_stops_leaks() {
        let anim = nested();
        let colour = |rgba: [u8; 4]| anim.palette().iter().position(|&c| c == rgba).unwrap() as u16;
        let (w, h) = (anim.width(), anim.height());
        let (x, y, size) = (1, 1, 1);
        // red under the whole square, green on top
        let painted = Scene {
            width: w,
            height: h,
            ticks: anim.ticks(),
            layers: vec![Layer {
                from: 0,
                to: anim.ticks(),
                groups: vec![
                    Group {
                        colour: colour([255, 0, 0, 255]),
                        rule: FillRule::NonZero,
                        shapes: vec![Shape::Rect { x: 0, y: 0, width: w, height: h }],
                    },
                    Group {
                        colour: colour([0, 255, 0, 255]),
                        rule: FillRule::NonZero,
                        shapes: vec![Shape::Rect { x, y, width: size, height: size }],
                    },
                ],
            }],
        };
        assert!(painted.seams(anim.palette()).is_empty());
        for (name, result) in seams_of(&anim, &painted) {
            assert_eq!((result.leaks, result.fringes), (0, 0), "{name}");
        }
        let runs = tgradish_tgs::encode::runs(&anim);
        assert!(!runs.seams(anim.palette()).is_empty());
        for (name, result) in seams_of(&anim, &runs) {
            assert!(result.leaks > 0, "{name}");
        }
    }

    #[test]
    fn baseline_matches_the_corpus() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references/pixelart");
        for name in ["Ralsei_battle_start.gif", "ralsei_fat_blunt.webp", "animation_hammer.gif"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                eprintln!("skipped: needs references/pixelart/{name}");
                return;
            };
            let input = tgradish_tgs::frames::decode(&bytes, &Default::default()).unwrap();
            let (anim, _) = normalise(&input, &Options::default()).unwrap();
            let scene = tgradish_tgs::encode::runs(&anim);
            let json = lay_out(&scene, &anim, None).to_json(Style::default());
            for (renderer, mut instance) in Renderer::all(json.as_bytes()).unwrap() {
                let result = centres(&mut instance, &anim).unwrap();
                assert_eq!(result.misses, 0, "{name} in {renderer}: {:?}", result.examples);
            }
        }
    }
}

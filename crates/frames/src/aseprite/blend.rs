//! Aseprite's layer blend modes on straight 8-bit RGBA.
//!
//! Ported from asefile 0.3.8 (`src/blend.rs`, MIT, Copyright (c) 2020
//! Alpine Alpaca Games), itself a port of Aseprite's
//! `src/doc/blend_funcs.cpp`, keeping its integer rounding and the
//! saturation quirk so results match Aseprite's exports.

pub type Colour = [u8; 4];

/// Blend modes by their number in the file.
pub fn by_number(mode: u16) -> Option<fn(Colour, Colour, u8) -> Colour> {
    Some(match mode {
        0 => normal,
        1 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, multiply)),
        2 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, screen)),
        3 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, |b, s| hard_light(s, b))),
        4 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, |b, s| b.min(s) as u8)),
        5 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, |b, s| b.max(s) as u8)),
        6 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, color_dodge)),
        7 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, color_burn)),
        8 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, hard_light)),
        9 => |b, s, o| blender(b, s, o, soft_light),
        10 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, |b, s| b.abs_diff(s) as u8)),
        11 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, exclusion)),
        12 => |b, s, o| blender(b, s, o, hue),
        13 => |b, s, o| blender(b, s, o, saturation),
        14 => |b, s, o| blender(b, s, o, color),
        15 => |b, s, o| blender(b, s, o, luminosity),
        16 => {
            |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, |b, s| (b + s).min(255) as u8))
        }
        17 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, |b, s| (b - s).max(0) as u8)),
        18 => |b, s, o| blender(b, s, o, |b, s, o| channels(b, s, o, divide)),
        _ => return None,
    })
}

/// `MUL_UN8`: `a * b / 255`, rounded.
pub fn mul_un8(a: i32, b: i32) -> i32 {
    let t = a * b + 0x80;
    ((t >> 8) + t) >> 8
}

/// `DIV_UN8`: `a * 255 / b`, rounded.
fn div_un8(a: i32, b: i32) -> u8 {
    ((a * 0xff + b / 2) / b).clamp(0, 255) as u8
}

fn blend8(back: u8, src: u8, opacity: u8) -> u8 {
    let t = (i32::from(src) - i32::from(back)) * i32::from(opacity) + 0x80;
    (i32::from(back) + (((t >> 8) + t) >> 8)).clamp(0, 255) as u8
}

fn ints([r, g, b, a]: Colour) -> [i32; 4] {
    [r, g, b, a].map(i32::from)
}

fn clamped(r: i32, g: i32, b: i32, a: i32) -> Colour {
    [r, g, b, a].map(|c| c.clamp(0, 255) as u8)
}

/// `rgba_blender_merge`.
fn merge(back: Colour, src: Colour, opacity: u8) -> Colour {
    let [r, g, b] = if back[3] == 0 {
        [src[0], src[1], src[2]]
    } else if src[3] == 0 {
        [back[0], back[1], back[2]]
    } else {
        [0, 1, 2].map(|i| blend8(back[i], src[i], opacity))
    };
    let a = blend8(back[3], src[3], opacity);
    if a == 0 { [0; 4] } else { [r, g, b, a] }
}

/// `rgba_blender_normal`.
pub fn normal(back: Colour, src: Colour, opacity: u8) -> Colour {
    let [br, bg, bb, ba] = ints(back);
    let [sr, sg, sb, sa] = ints(src);
    if ba == 0 {
        return clamped(sr, sg, sb, mul_un8(sa, i32::from(opacity)));
    }
    if sa == 0 {
        return back;
    }
    let sa = mul_un8(sa, i32::from(opacity));
    let ra = sa + ba - mul_un8(ba, sa);
    if ra == 0 {
        return back;
    }
    clamped(br + (sr - br) * sa / ra, bg + (sg - bg) * sa / ra, bb + (sb - bb) * sa / ra, ra)
}

/// The wrapper every mode but normal uses: blends by the mode where the
/// backdrop is opaque, normally where it is transparent.
fn blender(
    back: Colour,
    src: Colour,
    opacity: u8,
    mode: fn(Colour, Colour, u8) -> Colour,
) -> Colour {
    if back[3] == 0 {
        return normal(back, src, opacity);
    }
    let blended = mode(back, src, opacity);
    let normal_to_blend = merge(normal(back, src, opacity), blended, back[3]);
    let src_alpha = mul_un8(i32::from(src[3]), i32::from(opacity));
    let composite_alpha = mul_un8(i32::from(back[3]), src_alpha);
    merge(normal_to_blend, blended, composite_alpha.clamp(0, 255) as u8)
}

/// Applies a per-channel function, then blends normally.
fn channels(back: Colour, src: Colour, opacity: u8, f: fn(i32, i32) -> u8) -> Colour {
    let (b, s) = (ints(back), ints(src));
    normal(back, [f(b[0], s[0]), f(b[1], s[1]), f(b[2], s[2]), src[3]], opacity)
}

fn multiply(b: i32, s: i32) -> u8 {
    mul_un8(b, s).clamp(0, 255) as u8
}

fn screen(b: i32, s: i32) -> u8 {
    (b + s - mul_un8(b, s)).clamp(0, 255) as u8
}

fn hard_light(b: i32, s: i32) -> u8 {
    if s < 128 { multiply(b, s << 1) } else { screen(b, (s << 1) - 255) }
}

fn color_dodge(b: i32, s: i32) -> u8 {
    if b == 0 {
        return 0;
    }
    let s = 255 - s;
    if b >= s { 255 } else { div_un8(b, s) }
}

fn color_burn(b: i32, s: i32) -> u8 {
    if b == 255 {
        return 255;
    }
    let b = 255 - b;
    if b >= s { 0 } else { 255 - div_un8(b, s) }
}

fn divide(b: i32, s: i32) -> u8 {
    if b == 0 {
        0
    } else if b >= s {
        255
    } else {
        div_un8(b, s)
    }
}

fn exclusion(b: i32, s: i32) -> u8 {
    (b + s - 2 * mul_un8(b, s)).clamp(0, 255) as u8
}

fn soft_light(back: Colour, src: Colour, opacity: u8) -> Colour {
    let channel = |b: u8, s: u8| {
        let (b, s) = (f64::from(b) / 255.0, f64::from(s) / 255.0);
        let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
        let r = if s <= 0.5 {
            b - (1.0 - 2.0 * s) * b * (1.0 - b)
        } else {
            b + (2.0 * s - 1.0) * (d - b)
        };
        (r * 255.0 + 0.5).clamp(0.0, 255.0) as u8
    };
    let blended = [0, 1, 2].map(|i| channel(back[i], src[i]));
    normal(back, [blended[0], blended[1], blended[2], src[3]], opacity)
}

// The non-separable modes, from the PDF blend mode addendum through pixman.

type Rgb = [f64; 3];

fn rgb(colour: Colour) -> Rgb {
    [0, 1, 2].map(|i| f64::from(colour[i]) / 255.0)
}

fn from_rgb(c: Rgb, alpha: u8) -> Colour {
    let byte = |v: f64| (v * 255.0).clamp(0.0, 255.0) as u8;
    [byte(c[0]), byte(c[1]), byte(c[2]), alpha]
}

/// Chroma, which Aseprite calls saturation.
fn sat(c: Rgb) -> f64 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn lum(c: Rgb) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip(c: Rgb) -> Rgb {
    let l = lum(c);
    let (min, max) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    let mut c = c;
    if min < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - min));
    }
    if max > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (max - l));
    }
    c
}

fn set_lum(c: Rgb, l: f64) -> Rgb {
    let delta = l - lum(c);
    clip(c.map(|v| v + delta))
}

/// Aseprite's way of finding the smallest, middle and largest channel,
/// which mixes them up when two are equal; kept for identical results.
fn sort3([r, g, b]: Rgb) -> (usize, usize, usize) {
    let min = if r < g.min(b) {
        0
    } else if g < b {
        1
    } else {
        2
    };
    let max = if r > g.max(b) {
        0
    } else if g > b {
        1
    } else {
        2
    };
    let mid = if r > g {
        if g > b {
            1
        } else if r > b {
            2
        } else {
            0
        }
    } else if g > b {
        if b > r { 2 } else { 0 }
    } else {
        1
    };
    (min, mid, max)
}

fn set_sat(c: Rgb, s: f64) -> Rgb {
    let mut c = c;
    let (min, mid, max) = sort3(c);
    if c[max] > c[min] {
        c[mid] = (c[mid] - c[min]) * s / (c[max] - c[min]);
        c[max] = s;
    } else {
        c[mid] = 0.0;
        c[max] = 0.0;
    }
    c[min] = 0.0;
    c
}

fn hue(back: Colour, src: Colour, opacity: u8) -> Colour {
    let b = rgb(back);
    let c = set_lum(set_sat(rgb(src), sat(b)), lum(b));
    normal(back, from_rgb(c, src[3]), opacity)
}

fn saturation(back: Colour, src: Colour, opacity: u8) -> Colour {
    let b = rgb(back);
    let c = set_lum(set_sat(b, sat(rgb(src))), lum(b));
    normal(back, from_rgb(c, src[3]), opacity)
}

fn color(back: Colour, src: Colour, opacity: u8) -> Colour {
    let c = set_lum(rgb(src), lum(rgb(back)));
    normal(back, from_rgb(c, src[3]), opacity)
}

fn luminosity(back: Colour, src: Colour, opacity: u8) -> Colour {
    let c = set_lum(rgb(back), lum(rgb(src)));
    normal(back, from_rgb(c, src[3]), opacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_like_aseprite() {
        let red = [255, 0, 0, 255];
        let half_blue = [0, 0, 255, 128];
        // onto nothing: the source with opacity applied
        assert_eq!(normal([0; 4], half_blue, 255), half_blue);
        assert_eq!(normal([0; 4], red, 128), [255, 0, 0, 128]);
        assert_eq!(normal(red, half_blue, 255), [127, 0, 128, 255]);
        assert_eq!(normal(red, [0; 4], 255), red);
        let multiply = by_number(1).unwrap();
        assert_eq!(multiply([200, 100, 50, 255], [128, 255, 0, 255], 255), [100, 100, 0, 255]);
        let difference = by_number(10).unwrap();
        assert_eq!(difference([200, 100, 50, 255], [50, 150, 50, 255], 255), [150, 50, 0, 255]);
        assert!(by_number(19).is_none());
        // every mode handles every corner without panicking
        for mode in 0..=18 {
            let f = by_number(mode).unwrap();
            for back in [[0; 4], [255; 4], [0, 0, 0, 255], [10, 200, 30, 1]] {
                for src in [[0; 4], [255; 4], [255, 255, 255, 0], [128, 64, 32, 200]] {
                    for opacity in [0, 1, 128, 255] {
                        f(back, src, opacity);
                    }
                }
            }
        }
    }
}

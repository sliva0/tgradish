//! Image framing (scaling, cropping and padding) for WebM and WebP outputs,
//! plus pixel-art detection so `auto` scaling keeps pixel art crisp.

use crate::CancelToken;
use crate::backend::{Backend, FramesRequest};
use crate::error::{Error, Result};
use crate::options::{Crop, IntegerScale, Resize, Scaling};
use crate::presets::Format;
use crate::telegram::{self, Target};
use crate::webm::Options;
use crate::webm::Plan;

/// Rounds `value` to the nearest valid dimension for `format` (even and >= 2
/// for WebM; >= 1 for WebP).
fn side(format: Format, value: f64) -> u32 {
    match format.even_sides() {
        true => ((value / 2.0).round() as u32 * 2).max(2),
        false => (value.round() as u32).max(1),
    }
}

/// Computes the centring pad offset along an axis of length `out` for content
/// of length `scaled`. When `format.even_sides()` is true, rounds down to an
/// even pixel so WebM 4:2:0 chroma subsampling does not bleed the margin into
/// the image edge. Shared by encoding and GUI previews.
pub fn pad_offset(format: Format, out: u32, scaled: u32) -> u32 {
    let middle = out.saturating_sub(scaled) / 2;
    if format.even_sides() { middle / 2 * 2 } else { middle }
}

/// Resolved scaling and output dimensions for a WebM or WebP conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sizes {
    /// Dimensions after scaling and before cropping or padding.
    pub scaled: (u32, u32),
    /// Final output width in pixels, to which the scaled image is
    /// centre-cropped or centre-padded.
    pub width: u32,
    /// Final output height in pixels, as the width.
    pub height: u32,
    /// Effective scaling filter (`Auto` resolves to `Smooth` until pixel-art
    /// detection runs; `Integer` falls back to `Sharp` when downscaling).
    pub scaling: Scaling,
}

impl Sizes {
    /// Constructs a [`Sizes`] value (useful for front-ends computing preview
    /// geometry).
    pub fn new(scaled: (u32, u32), width: u32, height: u32, scaling: Scaling) -> Sizes {
        Sizes { scaled, width, height, scaling }
    }

    /// Returns true if transparent padding will be added around the image.
    pub fn pads(&self) -> bool {
        self.scaled.0 < self.width || self.scaled.1 < self.height
    }

    /// Minimum upscale factor along either axis relative to `used`.
    pub fn enlarges(&self, used: (u32, u32)) -> f64 {
        let along = |scaled: u32, used: u32| f64::from(scaled) / f64::from(used.max(1));
        along(self.scaled.0, used.0).min(along(self.scaled.1, used.1))
    }
}

/// Computes output dimensions for `used` source dimensions under `format`,
/// `target`, `resize` and `scaling`.
pub fn sizes(
    format: Format,
    target: Target,
    resize: Resize,
    scaling: Scaling,
    used: (u32, u32),
) -> Sizes {
    let even = |value: f64| side(format, value);
    let (box_w, box_h) = target.box_size();
    let (src_w, src_h) = (f64::from(used.0.max(1)), f64::from(used.1.max(1)));
    let (box_wf, box_hf) = (f64::from(box_w), f64::from(box_h));
    let contain = (box_wf / src_w).min(box_hf / src_h);
    let fill = (box_wf / src_w).max(box_hf / src_h);
    let contained = (even(src_w * contain).min(box_w), even(src_h * contain).min(box_h));
    let boxed = (box_w, box_h);
    // whole multiples of each pixel, where the picture can grow
    let whole = match resize {
        Resize::Contain | Resize::Pad => Some((contain.floor(), contain.floor())),
        Resize::Crop => (fill >= 1.0).then(|| (fill.ceil(), fill.ceil())),
        Resize::Stretch => Some(((box_wf / src_w).floor(), (box_hf / src_h).floor())),
    }
    .filter(|&(x, y)| scaling == Scaling::Integer && x >= 1.0 && y >= 1.0);
    let (scaled, (width, height)) = match (whole, resize) {
        (Some((x, y)), _) => {
            let scaled = ((src_w * x) as u32, (src_h * y) as u32);
            (scaled, if resize == Resize::Contain { contained } else { boxed })
        }
        (None, Resize::Contain) => (contained, contained),
        (None, Resize::Pad) => (contained, boxed),
        (None, Resize::Crop) => {
            ((even(src_w * fill).max(box_w), even(src_h * fill).max(box_h)), boxed)
        }
        (None, Resize::Stretch) => (boxed, boxed),
    };
    let scaling = match scaling {
        Scaling::Integer if whole.is_none() => Scaling::Sharp,
        Scaling::Auto => Scaling::Smooth,
        other => other,
    };
    Sizes { scaled, width, height, scaling }
}

/// Computes output dimensions for an explicit [`IntegerScale`], which must
/// keep the image within the target box. Pads the remaining area with
/// transparency to satisfy `target` and `resize` (the full target box for
/// emoji or `Resize::Pad`, or 512 px on the longer side for `Resize::Contain`,
/// with even sides for WebM ([`Format::even_sides`])), using
/// [`Scaling::Integer`] when enlarging and [`Scaling::Sharp`] for `1` or
/// `1/N`.
pub fn exact_sizes(
    format: Format,
    target: Target,
    resize: Resize,
    used: (u32, u32),
    scale: IntegerScale,
) -> Result<Sizes> {
    let (box_w, box_h) = target.box_size();
    let invalid = |reason: String| Error::invalid("integer-scale", reason);
    if !scale.is_valid() {
        // Format {up}/{down} directly because IntegerScale::NONE formats as "".
        let IntegerScale { up, down } = scale;
        return Err(invalid(format!("{up}/{down} has a side of 0: use N or 1/N")));
    }
    let (Some(width), Some(height)) = (scale.apply(used.0), scale.apply(used.1)) else {
        let down = scale.down;
        return Err(invalid(format!(
            "{scale} requires crop dimensions to be multiples of {down} pixels"
        )));
    };
    if width > box_w || height > box_h {
        let most = scale.most_input(box_w.max(box_h));
        return Err(invalid(match most {
            0 => format!("{scale} makes a single pixel larger than the target {}", target.name()),
            most => format!("{scale} requires a crop of at most {most} pixels on the longer side"),
        }));
    }
    let even = |side: u32| if format.even_sides() { side.next_multiple_of(2) } else { side };
    let result = match resize {
        Resize::Crop | Resize::Stretch => {
            let reason = "cannot be used with integer-scale; use contain or pad";
            let name = if resize == Resize::Crop { "crop" } else { "stretch" };
            return Err(Error::invalid("resize", format!("{name} {reason}")));
        }
        Resize::Pad => (box_w, box_h),
        _ if target.requires_exact_size() => (box_w, box_h),
        Resize::Contain if width >= height => (box_w, even(height)),
        Resize::Contain => (even(width), box_h),
    };
    // growing makes whole blocks; shrinking averages whole blocks
    let scaling = if scale.up > 1 { Scaling::Integer } else { Scaling::Sharp };
    Ok(Sizes { scaled: (width, height), width: result.0, height: result.1, scaling })
}

/// Returns true if `frames` (with width `width`) look like pixel art inside
/// `crop`, defined as having at most [`PIXEL_ART_COLOURS`] distinct
/// non-transparent colours.
pub fn is_pixel_art(frames: &[Vec<u8>], width: u32, crop: Option<Crop>) -> bool {
    has_few_colours(frames, width, crop, PIXEL_ART_COLOURS)
}

/// Returns true if `frames` have at most `most` distinct non-transparent
/// colours inside `crop`. Any part of `crop` outside the frame bounds is
/// ignored because front-ends call this on preview frames downscaled from the
/// full input on which `crop` was drawn.
pub fn has_few_colours(frames: &[Vec<u8>], width: u32, crop: Option<Crop>, most: usize) -> bool {
    let mut colours = std::collections::HashSet::new();
    let width = width as usize;
    for rgba in frames {
        let height = rgba.len() / 4 / width.max(1);
        let (columns, rows) = match crop {
            Some(crop) => {
                let within = |start: u32, length: u32, side: usize| {
                    let start = (start as usize).min(side);
                    start..start.saturating_add(length as usize).min(side)
                };
                (within(crop.x, crop.width, width), within(crop.y, crop.height, height))
            }
            None => (0..width, 0..height),
        };
        for row in rows {
            let row = &rgba[row * width * 4..][..width * 4];
            for pixel in row[columns.start * 4..columns.end * 4].as_chunks::<4>().0 {
                if pixel[3] > 0 && colours.insert(*pixel) && colours.len() > most {
                    return false;
                }
            }
        }
    }
    true
}

/// Maximum distinct non-transparent colours considered pixel art.
pub const PIXEL_ART_COLOURS: usize = 256;

/// Default resize mode for `target`: [`Resize::Pad`] for emoji (which must be
/// exactly 100x100) and [`Resize::Contain`] for stickers.
pub fn default_resize(target: Target) -> Resize {
    if target.requires_exact_size() { Resize::Pad } else { Resize::Contain }
}

/// Computes WebM or WebP output sizes for `used` dimensions, `target` and
/// `options`. Shared between conversion planning and GUI previews.
pub fn picture_sizes(
    format: Format,
    target: Target,
    options: &Options,
    used: (u32, u32),
) -> Result<Sizes> {
    let resize = options.resize.unwrap_or(default_resize(target));
    match options.integer_scale() {
        Some(scale) => exact_sizes(format, target, resize, used, scale),
        None => Ok(sizes(format, target, resize, options.scaling_or_default(), used)),
    }
}

/// Minimum upscale factor at which `auto` scaling switches to sharp scaling for
/// pixel art. Below 2x, nearest-neighbour produces uneven 1-pixel and 2-pixel
/// blocks that look worse than smooth scaling.
pub const AUTO_SHARP_FROM: f64 = 2.0;

/// Updates `plan.scaling` to [`Scaling::Sharp`] when `options` use automatic
/// scaling, the image is upscaled by at least [`AUTO_SHARP_FROM`], and sampled
/// frames look like pixel art.
pub(crate) fn choose_auto_scaling(
    backend: &Backend,
    plan: &mut Plan,
    options: &Options,
    cancel: &CancelToken,
) -> Result<()> {
    let mut looked = Ok(false);
    if turns_sharp(options, plan.enlarges(), || {
        looked = looks_like_pixel_art(backend, plan, cancel);
        *looked.as_ref().unwrap_or(&false)
    }) {
        plan.scaling = Scaling::Sharp;
    }
    looked.map(|_| ())
}

/// Returns true if `auto` scaling should switch to sharp scaling for an image
/// upscaled by `enlarges`. Only invokes `looks_like_art` when needed. Shared
/// with GUI previews.
pub fn turns_sharp(
    options: &Options,
    enlarges: f64,
    looks_like_art: impl FnOnce() -> bool,
) -> bool {
    options.auto_scaling() && enlarges >= AUTO_SHARP_FROM && looks_like_art()
}

/// Number of frames sampled for pixel-art detection: enough to look past a
/// title card while staying fast to decode.
pub const PIXEL_ART_SAMPLES: usize = 3;

/// Decodes up to [`PIXEL_ART_SAMPLES`] full-resolution frames and checks
/// whether they look like pixel art.
fn looks_like_pixel_art(backend: &Backend, plan: &Plan, cancel: &CancelToken) -> Result<bool> {
    let request = FramesRequest {
        start: plan.start,
        length: Some((plan.length * plan.tempo).min(telegram::MAX_SECONDS)),
        fps: plan.fps,
        max_side: plan.source.width.max(plan.source.height),
        max_frames: PIXEL_ART_SAMPLES,
        threads: plan.threads,
    };
    let frames = backend.frames(&plan.input, &plan.source, &request, cancel)?;
    Ok(is_pixel_art(&frames.frames, frames.width, plan.crop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_odd_sides_where_the_format_takes_them() {
        let contained = |format, used| {
            let sizes = sizes(format, Target::Sticker, Resize::Contain, Scaling::Auto, used);
            (sizes.scaled, (sizes.width, sizes.height))
        };
        // a WebP keeps the crop's size where it fits, and its shape
        assert_eq!(contained(Format::Webp, (325, 512)), ((325, 512), (325, 512)));
        assert_eq!(contained(Format::Webp, (101, 51)), ((512, 259), (512, 259)));
        assert_eq!(contained(Format::Webp, (1, 1000)), ((1, 512), (1, 512)));
        // a WebM's colour planes need even sides
        assert_eq!(contained(Format::Webm, (325, 512)), ((326, 512), (326, 512)));
        assert_eq!(contained(Format::Webm, (101, 51)), ((512, 258), (512, 258)));
        assert_eq!(contained(Format::Webm, (1, 1000)), ((2, 512), (2, 512)));
        let filled =
            sizes(Format::Webp, Target::Emoji, Resize::Crop, Scaling::Smooth, (101, 51)).scaled;
        assert_eq!(filled, (198, 100));
        let thrice = IntegerScale { up: 3, down: 1 };
        let exact = |format| {
            let sizes =
                exact_sizes(format, Target::Sticker, Resize::Contain, (170, 101), thrice).unwrap();
            (sizes.width, sizes.height)
        };
        assert_eq!((exact(Format::Webp), exact(Format::Webm)), ((512, 303), (512, 304)));
    }

    #[test]
    fn tells_pixel_art_by_its_colours() {
        let flat = vec![vec![10, 20, 30, 255, 200, 100, 0, 255, 10, 20, 30, 255, 0, 0, 0, 0]];
        assert!(is_pixel_art(&flat, 2, None));
        let photo: Vec<u8> =
            (0..1000u32).flat_map(|i| [i as u8, (i / 256) as u8, 0, 255]).collect();
        assert!(!is_pixel_art(std::slice::from_ref(&photo), 1000, None));
        // only the crop counts
        let crop = Some(Crop { x: 0, y: 0, width: 100, height: 1 });
        assert!(is_pixel_art(std::slice::from_ref(&photo), 1000, crop));
    }

    #[test]
    fn counts_colours_of_a_crop_only_where_it_meets_the_frame() {
        // 4x2: a row of four colours over a row of one
        let row = |colours: [u8; 4]| colours.into_iter().flat_map(|c| [c, 0, 0, 255]);
        let frames = [row([1, 2, 3, 4]).chain(row([9, 9, 9, 9])).collect::<Vec<u8>>()];
        let few = |x, y, width, height, most| {
            has_few_colours(&frames, 4, Some(Crop { x, y, width, height }), most)
        };
        assert!(!few(0, 0, 4, 2, 4) && few(0, 0, 4, 2, 5));
        // partly outside: the last two columns, and the last row
        assert!(!few(2, 0, 100, 100, 2) && few(2, 0, 100, 100, 3));
        assert!(!few(0, 1, 4, u32::MAX, 0) && few(0, 1, 4, u32::MAX, 1));
        // wholly outside, to the right, below, and as far as numbers go
        assert!(few(5, 0, 2, 2, 0) && few(0, 2, 4, 1, 0));
        assert!(few(u32::MAX, u32::MAX, u32::MAX, u32::MAX, 0));
        // nor a picture with no width, or frames cut short
        assert!(has_few_colours(&frames, 0, Some(Crop::whole((4, 2))), 0));
        assert!(has_few_colours(&[vec![1, 0, 0, 255, 2, 0]], 4, None, 0));
    }
}

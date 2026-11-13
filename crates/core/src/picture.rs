//! Where a picture lands in a WebM or image sticker, as the options scale,
//! crop and pad it, and telling pixel art, which auto scaling keeps sharp,
//! from filmed pictures.

use crate::CancelToken;
use crate::backend::{Backend, FramesRequest};
use crate::error::{Error, Result};
use crate::options::{Crop, ExactScale, Resize, Scaling};
use crate::telegram::{self, Target};
use crate::webm::Options;
use crate::webm::Plan;

fn even(value: f64) -> u32 {
    ((value / 2.0).round() as u32 * 2).max(2)
}

/// How a picture lands in the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sizes {
    /// What the picture is scaled to.
    pub scaled: (u32, u32),
    /// The result, which the scaled picture is cropped or padded to, around
    /// its middle.
    pub width: u32,
    pub height: u32,
    /// The scaling used: [`Scaling::Auto`] is smooth until the picture
    /// turns out to be pixel art, and pixel-perfect needs room to grow.
    pub scaling: Scaling,
}

impl Sizes {
    /// Whether transparent margins fill part of the result.
    pub fn pads(&self) -> bool {
        self.scaled.0 < self.width || self.scaled.1 < self.height
    }

    /// How many times larger the picture gets, at least, from `used`.
    pub fn enlarges(&self, used: (u32, u32)) -> f64 {
        let along = |scaled: u32, used: u32| f64::from(scaled) / f64::from(used.max(1));
        along(self.scaled.0, used.0).min(along(self.scaled.1, used.1))
    }
}

/// Where a `used` picture (the crop, or all of the input) lands in a result
/// for `target`, as `resize` and `scaling` place it.
pub fn sizes(target: Target, resize: Resize, scaling: Scaling, used: (u32, u32)) -> Sizes {
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
    .filter(|&(x, y)| scaling == Scaling::PixelPerfect && x >= 1.0 && y >= 1.0);
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
        Scaling::PixelPerfect if whole.is_none() => Scaling::Sharp,
        Scaling::Auto => Scaling::Smooth,
        other => other,
    };
    Sizes { scaled, width, height, scaling }
}

/// Where a `used` picture lands at an exact `scale`, which must keep it
/// within the box. Transparent margins fill the rest of what `target` and
/// `resize` need: the box for emoji and padded stickers, otherwise 512 on
/// the longer side and even sides, as the video's colour planes need.
pub fn exact_sizes(
    target: Target,
    resize: Resize,
    used: (u32, u32),
    scale: ExactScale,
) -> Result<Sizes> {
    let (box_w, box_h) = target.box_size();
    let invalid = |reason: String| Error::invalid("exact-scale", reason);
    if !scale.is_valid() {
        // written out, as none is written as nothing
        let ExactScale { up, down } = scale;
        return Err(invalid(format!("{up}/{down} has a side of 0: use N or 1/N")));
    }
    let (Some(width), Some(height)) = (scale.apply(used.0), scale.apply(used.1)) else {
        let down = scale.down;
        return Err(invalid(format!(
            "{scale} needs the crop's sides in multiples of {down} pixels"
        )));
    };
    if width > box_w || height > box_h {
        let most = scale.most_input(box_w.max(box_h));
        return Err(invalid(match most {
            0 => format!("{scale} makes a pixel larger than a whole {}", target.name()),
            most => format!("{scale} takes a crop of at most {most} pixels on its longer side"),
        }));
    }
    let even = |side: u32| side.next_multiple_of(2);
    let result = match resize {
        Resize::Crop | Resize::Stretch => {
            let reason = "keeps the picture whole with an exact-scale: use contain or pad";
            let name = if resize == Resize::Crop { "crop" } else { "stretch" };
            return Err(Error::invalid("resize", format!("{name} {reason}")));
        }
        Resize::Pad => (box_w, box_h),
        _ if target.requires_exact_size() => (box_w, box_h),
        Resize::Contain if width >= height => (box_w, even(height)),
        Resize::Contain => (even(width), box_h),
    };
    // growing makes whole blocks; shrinking averages whole blocks
    let scaling = if scale.up > 1 { Scaling::PixelPerfect } else { Scaling::Sharp };
    Ok(Sizes { scaled: (width, height), width: result.0, height: result.1, scaling })
}

/// Whether `frames` of a `width` wide picture look like pixel art within
/// `crop`, as auto scaling decides it: few colours, so it is drawn rather
/// than filmed.
pub fn is_pixel_art(frames: &[Vec<u8>], width: u32, crop: Option<Crop>) -> bool {
    has_few_colours(frames, width, crop, PIXEL_ART_COLOURS)
}

/// Whether `frames` of a `width` wide picture have at most `most` colours
/// within `crop`, not counting transparent pixels. What of the crop lies
/// outside a frame holds no colours: front-ends ask this of frames made
/// smaller than the input the crop was drawn on.
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

/// More colours than this is a photo or a video, not pixel art.
pub const PIXEL_ART_COLOURS: usize = 256;

/// How a result fills its box when not told: emoji must be exactly
/// their size, so are padded; stickers keep the picture's shape.
pub fn default_resize(target: Target) -> Resize {
    if target.requires_exact_size() { Resize::Pad } else { Resize::Contain }
}

/// The sizes a WebM or image result of a `used` part of the input has, as
/// `options` ask. Planning a conversion uses this, and so can front-ends
/// showing what it will make, so the two can't disagree.
pub fn picture_sizes(options: &Options, used: (u32, u32)) -> Result<Sizes> {
    let target = options.target.unwrap_or_default();
    let resize = options.resize.unwrap_or(default_resize(target));
    match options.exact_scale() {
        Some(scale) => exact_sizes(target, resize, used, scale),
        None => Ok(sizes(target, resize, options.scaling_or_default(), used)),
    }
}

/// Auto scaling keeps pixels sharp for pixel art made at least this many
/// times as large. Less, and blocks of one and two pixels would alternate,
/// which shows more than blending them does.
pub const AUTO_SHARP_FROM: f64 = 2.0;

/// What auto scaling, as `options` leave it, makes of the input: sharp
/// pixels for pixel art made at least [`AUTO_SHARP_FROM`] times as large,
/// smooth as [`plan`] has it otherwise.
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

/// Whether auto scaling, as `options` leave it, gives a picture made
/// `enlarges` times as large sharp pixels: when it is that much larger
/// and `looks_like_art`, which is asked only then, as it reads frames.
/// Front-ends showing what a conversion makes ask this too.
pub fn turns_sharp(
    options: &Options,
    enlarges: f64,
    looks_like_art: impl FnOnce() -> bool,
) -> bool {
    options.auto_scaling() && enlarges >= AUTO_SHARP_FROM && looks_like_art()
}

/// How many frames auto scaling looks at to tell pixel art from filmed
/// pictures, from where the used part starts: enough to see more than a
/// title card, few enough to read quickly.
pub const PIXEL_ART_SAMPLES: usize = 3;

/// A few frames of the planned part at full size, to tell pixel art from
/// filmed pictures.
fn looks_like_pixel_art(backend: &Backend, plan: &Plan, cancel: &CancelToken) -> Result<bool> {
    let request = FramesRequest {
        start: plan.start,
        length: Some((plan.length * plan.tempo).min(telegram::MAX_SECONDS)),
        fps: plan.fps,
        max_side: plan.source.width.max(plan.source.height),
        max_frames: PIXEL_ART_SAMPLES,
    };
    let frames = backend.frames(&plan.input, &plan.source, &request, cancel)?;
    Ok(is_pixel_art(&frames.frames, frames.width, plan.crop))
}

#[cfg(test)]
mod tests {
    use super::*;

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

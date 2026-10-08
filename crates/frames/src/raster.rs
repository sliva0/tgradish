//! GIF, PNG/APNG and WebP through the `image` crate, which composites
//! frames (disposal, blending, sub-rectangles) into full-size RGBA.

use std::io::Cursor;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, Frames, ImageDecoder, ImageError};

use crate::{Animation, DEFAULT_FRAME_DURATION, Error, Format, Frame, Limits, Result};

pub(crate) fn decode(bytes: &[u8], format: Format, limits: &Limits) -> Result<Animation> {
    let error = |err: ImageError| match err {
        ImageError::Limits(err) => Error::TooLarge(err.to_string()),
        err => Error::Decode { format, message: err.to_string() },
    };
    // the codecs check sizes against these before they allocate
    let mut codec_limits = image::Limits::default();
    codec_limits.max_image_width = Some(limits.max_dimension);
    codec_limits.max_image_height = Some(limits.max_dimension);
    codec_limits.max_alloc = Some(limits.max_bytes as u64);
    let reader = Cursor::new(bytes);
    let animated = match format {
        Format::Gif => {
            let mut decoder = GifDecoder::new(reader).map_err(error)?;
            decoder.set_limits(codec_limits).map_err(error)?;
            collect(decoder.into_frames(), format, limits, gif_delay)?
        }
        Format::Png => {
            let decoder = PngDecoder::with_limits(reader, codec_limits).map_err(error)?;
            if decoder.is_apng().map_err(error)? {
                let frames = decoder.apng().map_err(error)?.into_frames();
                collect(frames, format, limits, |delay| delay)?
            } else {
                return still(decoder, format, limits);
            }
        }
        Format::WebP => {
            let mut decoder = WebPDecoder::new(reader).map_err(error)?;
            decoder.set_limits(codec_limits).map_err(error)?;
            if decoder.has_animation() {
                collect(decoder.into_frames(), format, limits, |delay| delay)?
            } else {
                return still(decoder, format, limits);
            }
        }
        Format::Aseprite => unreachable!("Aseprite files are decoded by aseprite/mod.rs"),
    };
    animated.ok_or(Error::Empty)
}

/// Browsers show GIF frames with a delay of 0 or 1 centiseconds for 100 ms,
/// and GIFs are made to look right in browsers.
fn gif_delay(delay: Duration) -> Duration {
    if delay <= Duration::from_millis(10) { DEFAULT_FRAME_DURATION } else { delay }
}

fn collect(
    frames: Frames<'_>,
    format: Format,
    limits: &Limits,
    delay: impl Fn(Duration) -> Duration,
) -> Result<Option<Animation>> {
    let mut size = None;
    let mut out = Vec::new();
    for frame in frames {
        let frame = frame.map_err(|err| match err {
            ImageError::Limits(err) => Error::TooLarge(err.to_string()),
            err => Error::Decode { format, message: err.to_string() },
        })?;
        let duration = delay(Duration::from(frame.delay()));
        let buffer = frame.into_buffer();
        let (width, height) = *size.get_or_insert(buffer.dimensions());
        // every frame is kept, so together they must fit
        limits.check(width, height, out.len() + 1)?;
        out.push(Frame { rgba: buffer.into_raw(), duration });
    }
    let Some((width, height)) = size else { return Ok(None) };
    // A single frame is a still image, whatever delay it has.
    if out.len() == 1 {
        out[0].duration = DEFAULT_FRAME_DURATION;
    }
    // Files whose frames all say 0 rely on the viewer's default speed.
    if out.iter().all(|frame| frame.duration.is_zero()) {
        out.iter_mut().for_each(|frame| frame.duration = DEFAULT_FRAME_DURATION);
    }
    Animation::new(width, height, out).map(Some)
}

fn still(decoder: impl ImageDecoder, format: Format, limits: &Limits) -> Result<Animation> {
    // the decoded image and its RGBA copy are both alive for a moment
    let (width, height) = decoder.dimensions();
    limits.check(width, height, 1)?;
    let rgba = (u64::from(width) * u64::from(height)).saturating_mul(4);
    if decoder.total_bytes().saturating_add(rgba) > limits.max_bytes as u64 {
        return Err(Error::TooLarge(format!("a {width}x{height} image")));
    }
    let image = DynamicImage::from_decoder(decoder)
        .map_err(|err| Error::Decode { format, message: err.to_string() })?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Animation::new(
        width,
        height,
        vec![Frame { rgba: image.into_raw(), duration: DEFAULT_FRAME_DURATION }],
    )
}

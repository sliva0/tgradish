//! GIF, PNG/APNG, WebP, JPEG and BMP through the `image` crate, which
//! composites frames (disposal, blending, sub-rectangles) into full-size
//! RGBA.

use std::io::Cursor;
use std::time::Duration;

use image::codecs::bmp::BmpDecoder;
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, Frames, ImageDecoder, ImageError};

use crate::{Animation, DEFAULT_FRAME_DURATION, Error, Format, Frame, Limits, Result};

/// The `image` crate's errors as ours: a codec stopped by its limits says
/// the input is too large, anything else that it doesn't decode.
fn image_error(format: Format) -> impl Fn(ImageError) -> Error + Copy {
    move |err| match err {
        ImageError::Limits(err) => Error::TooLarge(err.to_string()),
        err => Error::Decode { format, message: err.to_string() },
    }
}

pub(crate) fn decode(bytes: &[u8], format: Format, limits: &Limits) -> Result<Animation> {
    let error = image_error(format);
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
        Format::Jpeg => {
            let mut decoder = JpegDecoder::new(reader).map_err(error)?;
            decoder.set_limits(codec_limits).map_err(error)?;
            return still(decoder, format, limits);
        }
        Format::Bmp => {
            let mut decoder = BmpDecoder::new(reader).map_err(error)?;
            decoder.set_limits(codec_limits).map_err(error)?;
            return still(decoder, format, limits);
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
        let frame = frame.map_err(image_error(format))?;
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
    // A frame that says 0 relies on the viewer's default speed, which
    // `Animation::new` gives it, whether all of them do or only some.
    Animation::new(width, height, out).map(Some)
}

fn still(mut decoder: impl ImageDecoder, format: Format, limits: &Limits) -> Result<Animation> {
    // the decoded image and its RGBA copy are both alive for a moment
    let (width, height) = decoder.dimensions();
    limits.check(width, height, 1)?;
    let rgba = (u64::from(width) * u64::from(height)).saturating_mul(4);
    if decoder.total_bytes().saturating_add(rgba) > limits.max_bytes as u64 {
        return Err(Error::TooLarge(format!("a {width}x{height} image")));
    }
    let error = image_error(format);
    // photos are often stored sideways, with the way up in their metadata
    let orientation = decoder.orientation().map_err(error)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(error)?;
    image.apply_orientation(orientation);
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    Animation::new(
        width,
        height,
        vec![Frame { rgba: image.into_raw(), duration: DEFAULT_FRAME_DURATION }],
    )
}

#[cfg(test)]
mod tests {
    use tgradish_corpus::{Canvas, Sprite, encode};

    use super::*;
    use crate::DecodeOptions;

    /// A sprite of 2x2 frames, each of another colour, shown for `delays`
    /// milliseconds.
    fn sprite(delays: &[u32]) -> Sprite {
        let frames = (0..delays.len() as u8).map(|index| {
            let mut canvas = Canvas::new(2, 2);
            canvas.rect(0, 0, 2, 2, [index * 60, 200, 40, 255]);
            canvas
        });
        Sprite {
            name: "test",
            about: "",
            frames: frames.collect(),
            delays: delays.to_vec(),
            scale: 1,
            likely_scale: None,
        }
    }

    /// How long each frame of a decoded file shows, in milliseconds.
    fn delays(bytes: &[u8]) -> Vec<u128> {
        let animation =
            crate::decode(bytes, &DecodeOptions::default(), &crate::Limits::default()).unwrap();
        animation.frames().iter().map(|frame| frame.duration.as_millis()).collect()
    }

    #[test]
    fn shows_gif_frames_as_browsers_do() {
        let ms = Duration::from_millis;
        assert_eq!(gif_delay(ms(0)), DEFAULT_FRAME_DURATION);
        assert_eq!(gif_delay(ms(10)), DEFAULT_FRAME_DURATION);
        assert_eq!(gif_delay(ms(20)), ms(20));
        // in a file, where delays are in centiseconds
        assert_eq!(delays(&encode::gif(&sprite(&[0, 10, 20, 0, 70]))), [100, 100, 20, 100, 70]);
        assert_eq!(delays(&encode::gif(&sprite(&[0, 0, 0]))), [100, 100, 100]);
    }

    #[test]
    fn gives_frames_that_say_0_the_default_duration() {
        // APNG and WebP keep 10 ms as it is, and 0 is the default whether
        // every frame says it or only some
        type Encode = fn(&Sprite) -> Vec<u8>;
        for (format, encode) in [("APNG", encode::apng as Encode), ("WebP", encode::webp)] {
            assert_eq!(delays(&encode(&sprite(&[0, 0, 0]))), [100, 100, 100], "{format}");
            assert_eq!(delays(&encode(&sprite(&[0, 10, 40, 0]))), [100, 10, 40, 100], "{format}");
            assert_eq!(delays(&encode(&sprite(&[30, 0]))), [30, 100], "{format}");
        }
    }

    #[test]
    fn shows_a_single_frame_as_a_still_image() {
        // whatever delay its one frame has
        for delay in [0, 20, 5000] {
            let one = sprite(&[delay]);
            assert_eq!(delays(&encode::gif(&one)), [100], "GIF of {delay} ms");
            assert_eq!(delays(&encode::apng(&one)), [100], "APNG of {delay} ms");
            assert_eq!(delays(&encode::webp(&one)), [100], "WebP of {delay} ms");
            assert_eq!(delays(&encode::png(&one.frames[0])), [100]);
        }
    }
}

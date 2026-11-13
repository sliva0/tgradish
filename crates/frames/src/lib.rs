//! RGBA animations with frame durations, and pure-Rust decoders for them:
//! GIF, PNG and APNG, WebP, JPEG, BMP, Aseprite, sprite sheets and image
//! sequences.
//!
//! Builds for `wasm32-unknown-unknown`, so everything works on bytes: no
//! filesystem, processes or threads. What decoding may allocate is bounded
//! by [`Limits`], so hostile files fail rather than exhaust memory.
//!
//! ```
//! use tgradish_frames::{DEFAULT_FRAME_DURATION, DecodeOptions, Limits, decode};
//!
//! // a GIF of one black pixel
//! let gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;";
//! let animation = decode(gif, &DecodeOptions::default(), &Limits::default())?;
//! assert_eq!((animation.width(), animation.height()), (1, 1));
//! // a frame that says nothing of its time shows for the default
//! assert_eq!(animation.duration(), DEFAULT_FRAME_DURATION);
//! assert_eq!(animation.pixel(0, 0, 0), Some([0, 0, 0, 255]));
//! # Ok::<(), tgradish_frames::Error>(())
//! ```
//!
//! Aseprite files can be read for their tags and layers first, with
//! [`aseprite::Sprite`]; [`DecodeOptions`] picks among them.

#![warn(missing_docs)]

use std::fmt;
use std::time::Duration;

use thiserror::Error;

pub mod aseprite;
mod raster;
mod sheet;

use aseprite::{Layers, Sprite};
pub use sheet::{Sheet, sequence, split_sheet, sprite_sheet};

/// Duration of still images in an animation when nothing else says how long
/// to show them, and of every frame that says 0, in whatever format or
/// however it is put together: viewers show such frames for a while too,
/// and an [`Animation`] holds none.
pub const DEFAULT_FRAME_DURATION: Duration = Duration::from_millis(100);

/// Why an animation couldn't be decoded or put together.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum Error {
    /// An animation of no frames, which [`Animation::new`] refuses.
    #[error("the animation has no frames")]
    Empty,
    /// A width or height of 0, or a frame too large to address.
    #[error("{width}x{height} is not a usable size")]
    BadSize {
        /// The width given.
        width: u32,
        /// The height given.
        height: u32,
    },
    /// A frame of another size than its animation's.
    #[error("frame {index} has {len} bytes, a {width}x{height} RGBA frame has {expected}")]
    FrameSize {
        /// Which frame, from 0.
        index: usize,
        /// How many bytes of RGBA it has.
        len: usize,
        /// The animation's width.
        width: u32,
        /// The animation's height.
        height: u32,
        /// How many bytes it should have.
        expected: usize,
    },
    /// Bytes that start like no format decoded here.
    #[error("not a GIF, PNG, WebP, JPEG, BMP or Aseprite file")]
    UnknownFormat,
    /// A file that starts like `format` but doesn't decode as one.
    #[error("couldn't decode the {format} file: {message}")]
    Decode {
        /// What the file starts like.
        format: Format,
        /// What is wrong with it, in English.
        message: String,
    },
    /// An Aseprite tag asked for that the file doesn't have.
    #[error("no tag named {name:?} (tags: {})", if available.is_empty() { "none".into() } else { available.join(", ") })]
    NoSuchTag {
        /// The tag asked for.
        name: String,
        /// The tags the file has.
        available: Vec<String>,
    },
    /// A tag asked for of a file that isn't an Aseprite file.
    #[error("tags only exist in Aseprite files")]
    TagWithoutAseprite,
    /// An Aseprite layer named that the file doesn't have.
    #[error("no layer named {name:?} (layers: {})", if available.is_empty() { "none".into() } else { available.join(", ") })]
    NoSuchLayer {
        /// The layer named.
        name: String,
        /// The layers the file has.
        available: Vec<String>,
    },
    /// Layers named for a file that isn't an Aseprite file.
    #[error("layers only exist in Aseprite files")]
    LayersWithoutAseprite,
    /// An image of a [`sequence`] of another size than the first.
    #[error("image {index} is {width}x{height}, the first one is {first_width}x{first_height}")]
    SequenceSize {
        /// Which image, from 0.
        index: usize,
        /// Its width.
        width: u32,
        /// Its height.
        height: u32,
        /// The first image's width.
        first_width: u32,
        /// The first image's height.
        first_height: u32,
    },
    /// A sprite sheet that doesn't divide into the [`Sheet`]'s cells.
    #[error("{width}x{height} doesn't split into {columns} columns and {rows} rows")]
    SheetGrid {
        /// The sheet's width.
        width: u32,
        /// The sheet's height.
        height: u32,
        /// The columns asked for.
        columns: u32,
        /// The rows asked for.
        rows: u32,
    },
    /// More frames asked of a sprite sheet than it has cells, or none.
    #[error("the sprite sheet has {cells} cells, {frames} frames were asked for")]
    SheetFrames {
        /// How many cells it has.
        cells: u32,
        /// How many frames were asked for.
        frames: u32,
    },
    /// An animation given as a sprite sheet, which is one picture.
    #[error("a sprite sheet must be a single picture, this has {frames} frames")]
    SheetAnimated {
        /// How many frames it has.
        frames: usize,
    },
    /// Something past the [`Limits`] decoding was given.
    #[error("the input is too large: {}", oversize_text(what, limit))]
    TooLarge {
        /// What was too large, with its size.
        what: Oversize,
        /// The limit it went past.
        limit: Limit,
    },
}

/// What decoding found too large for its [`Limits`], with its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Oversize {
    /// Frames of one size: what all the frames decoded come to together.
    Frames {
        /// How many.
        frames: usize,
        /// Their width.
        width: u32,
        /// Their height.
        height: u32,
    },
    /// A still image, decoded and copied as RGBA.
    Image {
        /// Its width.
        width: u32,
        /// Its height.
        height: u32,
    },
    /// An Aseprite file's canvas.
    Canvas {
        /// Its width.
        width: u32,
        /// Its height.
        height: u32,
    },
    /// What decoding needs on the way, of a size not known before it is
    /// refused.
    Decoding,
}

/// The one of [`Limits`] that something went past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limit {
    /// [`Limits::max_dimension`], in pixels.
    Dimension(u32),
    /// [`Limits::max_bytes`].
    Bytes(usize),
}

fn oversize_text(what: &Oversize, limit: &Limit) -> String {
    match (what, limit) {
        (Oversize::Frames { frames, width, height }, _) => {
            format!("{frames} frames of {width}x{height}")
        }
        (Oversize::Image { width, height }, _) => format!("a {width}x{height} image"),
        (Oversize::Canvas { width, height }, _) => format!("the canvas is {width}x{height}"),
        (Oversize::Decoding, Limit::Bytes(bytes)) => {
            format!("decoding needs more than {bytes} bytes")
        }
        (Oversize::Decoding, Limit::Dimension(pixels)) => {
            format!("a side is longer than {pixels} pixels")
        }
    }
}

/// What this crate's functions return.
pub type Result<T> = std::result::Result<T, Error>;

/// The kinds of file [`decode`] reads, which [`Format::detect`] tells
/// apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// GIF, still or animated.
    Gif,
    /// PNG and APNG.
    Png,
    /// WebP, still or animated.
    WebP,
    /// Aseprite's `.ase` and `.aseprite`.
    Aseprite,
    /// Still images, which pixel art is rarely kept as, but screenshots
    /// and photos of it are.
    Jpeg,
    /// Still images, as Windows' Paint saves them.
    Bmp,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Gif => "GIF",
            Format::Png => "PNG",
            Format::WebP => "WebP",
            Format::Aseprite => "Aseprite",
            Format::Jpeg => "JPEG",
            Format::Bmp => "BMP",
        })
    }
}

impl Format {
    /// Recognises a file by its first bytes.
    pub fn detect(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Format::Gif)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Format::Png)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Format::WebP)
        } else if bytes.get(4..6) == Some(&[0xe0, 0xa5]) {
            Some(Format::Aseprite)
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Format::Jpeg)
        } else if bytes.len() >= 18 && bytes.starts_with(b"BM") {
            Some(Format::Bmp)
        } else {
            None
        }
    }
}

/// How much decoding may take, so hostile files fail instead of
/// exhausting memory. Built as `Limits { max_bytes, ..Limits::default() }`,
/// so that what is added later keeps its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest width or height.
    pub max_dimension: u32,
    /// The most bytes of pixels: all decoded frames together, and what the
    /// decoders need on the way, as what an Aseprite file says of its
    /// layers, cels and tags.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_dimension: 16384, max_bytes: 1 << 30 }
    }
}

impl Limits {
    /// Fails when `frames` frames of `width`x`height` don't fit.
    pub(crate) fn check(&self, width: u32, height: u32, frames: usize) -> Result<()> {
        let bytes = frame_bytes(width, height)?;
        let what = Oversize::Frames { frames, width, height };
        if width.max(height) > self.max_dimension {
            return Err(self.dimension(what));
        }
        if bytes.checked_mul(frames).is_none_or(|total| total > self.max_bytes) {
            return Err(self.bytes(what));
        }
        Ok(())
    }

    /// `what` is wider or higher than these limits allow.
    pub(crate) fn dimension(&self, what: Oversize) -> Error {
        Error::TooLarge { what, limit: Limit::Dimension(self.max_dimension) }
    }

    /// `what` takes more bytes than these limits allow.
    pub(crate) fn bytes(&self, what: Oversize) -> Error {
        Error::TooLarge { what, limit: Limit::Bytes(self.max_bytes) }
    }
}

/// What [`decode`] reads of a file. Built as `DecodeOptions { tag,
/// ..DecodeOptions::default() }`, so that what is added later keeps its
/// default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecodeOptions {
    /// Aseprite: only the frames of this tag, in its direction.
    pub tag: Option<String>,
    /// Aseprite: layers to show or hide, over what the file shows.
    pub layers: Layers,
}

/// Decodes an animated or still image, within `limits`. Still images get
/// one frame of [`DEFAULT_FRAME_DURATION`].
pub fn decode(bytes: &[u8], options: &DecodeOptions, limits: &Limits) -> Result<Animation> {
    let format = Format::detect(bytes).ok_or(Error::UnknownFormat)?;
    if format != Format::Aseprite && options.tag.is_some() {
        return Err(Error::TagWithoutAseprite);
    }
    if format != Format::Aseprite && !options.layers.is_empty() {
        return Err(Error::LayersWithoutAseprite);
    }
    match format {
        Format::Aseprite => {
            Sprite::read(bytes, limits)?.animation(options.tag.as_deref(), &options.layers)
        }
        _ => raster::decode(bytes, format, limits),
    }
}

/// One frame: straight (not premultiplied) RGBA, row by row from the top.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    /// 4 bytes a pixel, `width * height * 4` in all.
    pub rgba: Vec<u8>,
    /// How long it shows. In an [`Animation`] never zero.
    pub duration: Duration,
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("bytes", &self.rgba.len())
            .field("duration", &self.duration)
            .finish()
    }
}

/// Frames of one size, each shown for its own duration, looping. Every
/// frame shows for some time: see [`Animation::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    width: u32,
    height: u32,
    frames: Vec<Frame>,
}

impl Animation {
    /// Frames of `width`x`height`, which must be at least one, each with
    /// as many bytes as RGBA of that size takes. Frames that say they show
    /// for no time get [`DEFAULT_FRAME_DURATION`], so what works on an
    /// animation never has to decide what such a frame means.
    pub fn new(width: u32, height: u32, mut frames: Vec<Frame>) -> Result<Animation> {
        let expected = frame_bytes(width, height)?;
        if frames.is_empty() {
            return Err(Error::Empty);
        }
        if let Some((index, frame)) =
            frames.iter().enumerate().find(|(_, frame)| frame.rgba.len() != expected)
        {
            return Err(Error::FrameSize { index, len: frame.rgba.len(), width, height, expected });
        }
        for frame in &mut frames {
            if frame.duration.is_zero() {
                frame.duration = DEFAULT_FRAME_DURATION;
            }
        }
        Ok(Animation { width, height, frames })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The frames in playing order.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// The frames in playing order, kept rather than copied.
    pub fn into_frames(self) -> Vec<Frame> {
        self.frames
    }

    /// Length of one loop, or the longest a [`Duration`] holds when its
    /// frames come to more.
    pub fn duration(&self) -> Duration {
        self.frames.iter().fold(Duration::ZERO, |total, frame| total.saturating_add(frame.duration))
    }

    /// RGBA of a pixel, or `None` outside the animation.
    pub fn pixel(&self, frame: usize, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = ((y as usize * self.width as usize) + x as usize) * 4;
        let rgba = self.frames.get(frame)?.rgba.get(start..start + 4)?;
        Some([rgba[0], rgba[1], rgba[2], rgba[3]])
    }
}

fn frame_bytes(width: u32, height: u32) -> Result<usize> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|&bytes| bytes > 0)
        .ok_or(Error::BadSize { width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bytes: usize, ms: u64) -> Frame {
        Frame { rgba: vec![0; bytes], duration: Duration::from_millis(ms) }
    }

    #[test]
    fn checks_sizes() {
        assert_eq!(Animation::new(2, 2, vec![]), Err(Error::Empty));
        assert!(matches!(Animation::new(0, 2, vec![frame(0, 10)]), Err(Error::BadSize { .. })));
        assert!(matches!(
            Animation::new(u32::MAX, u32::MAX, vec![frame(0, 10)]),
            Err(Error::BadSize { .. })
        ));
        let wrong = Animation::new(2, 2, vec![frame(16, 10), frame(12, 10)]);
        assert!(matches!(wrong, Err(Error::FrameSize { index: 1, expected: 16, .. })));
    }

    #[test]
    fn reads_pixels_and_duration() {
        let mut first = frame(2 * 3 * 4, 100);
        // x = 1, y = 2
        first.rgba[(2 * 2 + 1) * 4..][..4].copy_from_slice(&[1, 2, 3, 4]);
        let animation = Animation::new(2, 3, vec![first, frame(24, 50)]).unwrap();
        assert_eq!(animation.pixel(0, 1, 2), Some([1, 2, 3, 4]));
        assert_eq!(animation.pixel(0, 2, 0), None);
        assert_eq!(animation.pixel(2, 0, 0), None);
        assert_eq!(animation.duration(), Duration::from_millis(150));
    }

    #[test]
    fn gives_frames_of_no_time_the_default() {
        let frames = vec![frame(4, 0), frame(4, 30), frame(4, 0)];
        let animation = Animation::new(1, 1, frames).unwrap();
        let durations: Vec<Duration> = animation.frames().iter().map(|f| f.duration).collect();
        let ms = Duration::from_millis;
        assert_eq!(durations, [DEFAULT_FRAME_DURATION, ms(30), DEFAULT_FRAME_DURATION]);
        assert_eq!(animation.duration(), ms(30) + DEFAULT_FRAME_DURATION * 2);

        // and frames of more time than there is don't overflow its length
        let endless = Frame { rgba: vec![0; 4], duration: Duration::MAX };
        let animation = Animation::new(1, 1, vec![endless.clone(), endless]).unwrap();
        assert_eq!(animation.duration(), Duration::MAX);
    }

    #[test]
    fn detects_formats() {
        assert_eq!(Format::detect(b"GIF89a..."), Some(Format::Gif));
        assert_eq!(Format::detect(b"\x89PNG\r\n\x1a\n...."), Some(Format::Png));
        assert_eq!(Format::detect(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::WebP));
        assert_eq!(Format::detect(b"\0\0\0\0\xe0\xa5"), Some(Format::Aseprite));
        assert_eq!(Format::detect(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(
            decode(b"nope", &DecodeOptions::default(), &Limits::default()),
            Err(Error::UnknownFormat)
        );
    }
}

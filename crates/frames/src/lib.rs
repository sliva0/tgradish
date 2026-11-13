//! RGBA animations with frame durations, and pure-Rust decoders for them:
//! GIF, PNG and APNG, WebP, JPEG, BMP, Aseprite, sprite sheets and image
//! sequences.
//!
//! Builds for `wasm32-unknown-unknown`, so everything works on bytes: no
//! filesystem, processes or threads.

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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("the animation has no frames")]
    Empty,
    #[error("{width}x{height} is not a usable size")]
    BadSize { width: u32, height: u32 },
    #[error("frame {index} has {len} bytes, a {width}x{height} RGBA frame has {expected}")]
    FrameSize { index: usize, len: usize, width: u32, height: u32, expected: usize },
    #[error("not a GIF, PNG, WebP, JPEG, BMP or Aseprite file")]
    UnknownFormat,
    #[error("couldn't decode the {format} file: {message}")]
    Decode { format: Format, message: String },
    #[error("no tag named {name:?} (tags: {})", if available.is_empty() { "none".into() } else { available.join(", ") })]
    NoSuchTag { name: String, available: Vec<String> },
    #[error("tags only exist in Aseprite files")]
    TagWithoutAseprite,
    #[error("no layer named {name:?} (layers: {})", if available.is_empty() { "none".into() } else { available.join(", ") })]
    NoSuchLayer { name: String, available: Vec<String> },
    #[error("layers only exist in Aseprite files")]
    LayersWithoutAseprite,
    #[error("image {index} is {width}x{height}, the first one is {first_width}x{first_height}")]
    SequenceSize { index: usize, width: u32, height: u32, first_width: u32, first_height: u32 },
    #[error("{width}x{height} doesn't split into {columns} columns and {rows} rows")]
    SheetGrid { width: u32, height: u32, columns: u32, rows: u32 },
    #[error("the sprite sheet has {cells} cells, {frames} frames were asked for")]
    SheetFrames { cells: u32, frames: u32 },
    #[error("a sprite sheet is one picture, this has {frames} frames")]
    SheetAnimated { frames: usize },
    #[error("the input is too large: {0}")]
    TooLarge(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Gif,
    /// PNG and APNG.
    Png,
    WebP,
    Aseprite,
    /// Still images, which pixel art is rarely kept as, but screenshots
    /// and photos of it are.
    Jpeg,
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
/// exhausting memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest width or height.
    pub max_dimension: u32,
    /// The most bytes of pixels: all decoded frames together, and what the
    /// decoders need on the way.
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
        if width.max(height) > self.max_dimension
            || bytes.checked_mul(frames).is_none_or(|total| total > self.max_bytes)
        {
            return Err(Error::TooLarge(format!("{frames} frames of {width}x{height}")));
        }
        Ok(())
    }
}

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
    /// Frames that say they show for no time get
    /// [`DEFAULT_FRAME_DURATION`], so what works on an animation never has
    /// to decide what such a frame means.
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

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

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

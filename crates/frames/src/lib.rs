//! RGBA animations with frame durations: what `.tgs` conversion starts from.
//! Decoders for GIF, APNG, animated WebP, Aseprite, sprite sheets and image
//! sequences come next (see `docs/tgs.md`, T2).
//!
//! Builds for `wasm32-unknown-unknown`, so everything works on bytes: no
//! filesystem, processes or threads.

use std::fmt;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("the animation has no frames")]
    Empty,
    #[error("{width}x{height} is not a usable size")]
    BadSize { width: u32, height: u32 },
    #[error("frame {index} has {len} bytes, a {width}x{height} RGBA frame has {expected}")]
    FrameSize { index: usize, len: usize, width: u32, height: u32, expected: usize },
}

pub type Result<T> = std::result::Result<T, Error>;

/// One frame: straight (not premultiplied) RGBA, row by row from the top.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub rgba: Vec<u8>,
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

/// Frames of one size, each shown for its own duration, looping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    width: u32,
    height: u32,
    frames: Vec<Frame>,
}

impl Animation {
    pub fn new(width: u32, height: u32, frames: Vec<Frame>) -> Result<Animation> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|&bytes| bytes > 0)
            .ok_or(Error::BadSize { width, height })?;
        if frames.is_empty() {
            return Err(Error::Empty);
        }
        if let Some((index, frame)) =
            frames.iter().enumerate().find(|(_, frame)| frame.rgba.len() != expected)
        {
            return Err(Error::FrameSize { index, len: frame.rgba.len(), width, height, expected });
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

    /// Length of one loop.
    pub fn duration(&self) -> Duration {
        self.frames.iter().map(|frame| frame.duration).sum()
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
}

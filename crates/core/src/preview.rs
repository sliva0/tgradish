//! Decoded sticker frames for previews (in the GUI and terminal) across all
//! output formats.

/// Tick rate (60 ticks per second) used to time preview frames. Matches `.tgs`
/// frame rate and is high enough for any WebM sticker.
pub const TICKS: f64 = 60.0;

/// Straight-RGBA preview frames, each paired with its display duration in
/// [`TICKS`] (1/60 s units). `.tgs` previews use one pixel per art pixel
/// ([`crate::tgs`]), WebM previews scale to the requested size
/// ([`crate::backend::Backend::preview`]), and WebP previews have one frame
/// ([`crate::webp::preview`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Preview {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// `(rgba_bytes, ticks)` per frame
    /// (`rgba_bytes.len() == width * height * 4`).
    pub frames: Vec<(Vec<u8>, u32)>,
}

impl Preview {
    /// Creates a [`Preview`] from explicit RGBA frames and tick durations.
    pub fn new(width: u32, height: u32, frames: Vec<(Vec<u8>, u32)>) -> Preview {
        Preview { width, height, frames }
    }
}

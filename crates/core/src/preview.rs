//! Finished stickers decoded for showing, whatever they were made as.

/// How many ticks a second preview frames are timed in: `.tgs` stickers'
/// frame rate, which times their frames, and enough for any WebM's.
pub const TICKS: f64 = 60.0;

/// Frames of a sticker as straight RGBA, each with how many 60 fps frames
/// ([`TICKS`]) it shows for. `.tgs` previews have a pixel per art pixel
/// ([`crate::tgs`]), WebM previews fit the size asked for
/// ([`crate::backend::Backend::preview`]), and static stickers are one
/// frame ([`crate::webp::preview`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Preview {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Each frame, `width * height * 4` bytes, and its [`TICKS`].
    pub frames: Vec<(Vec<u8>, u32)>,
}

impl Preview {
    /// Frames of `width` by `height` straight RGBA, each with how many
    /// [`TICKS`] it shows for: for front-ends to show one made up, as in
    /// tests.
    pub fn new(width: u32, height: u32, frames: Vec<(Vec<u8>, u32)>) -> Preview {
        Preview { width, height, frames }
    }
}

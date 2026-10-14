//! Finished stickers decoded for showing, whatever they were made as.

/// How many ticks a second preview frames are timed in: `.tgs` stickers'
/// frame rate, which times their frames, and enough for any WebM's.
pub const TICKS: f64 = 60.0;

/// Frames of a sticker as straight RGBA, each with how many 60 fps frames
/// ([`TICKS`]) it shows for. `.tgs` previews have a pixel per art pixel
/// ([`crate::tgs`]), WebM previews fit the size asked for
/// ([`crate::backend::Backend::preview`]), and image stickers are one
/// frame ([`crate::still::preview`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<(Vec<u8>, u32)>,
}

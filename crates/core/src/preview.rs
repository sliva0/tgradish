//! Finished stickers decoded for showing, whatever they were made as.

/// Frames of a sticker as straight RGBA, each with how many 60 fps frames
/// it shows for. `.tgs` previews have a pixel per art pixel
/// ([`crate::tgs`]), WebM previews fit the size asked for
/// ([`crate::backend::Backend::preview`]), and image stickers are one
/// frame ([`crate::still::preview`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<(Vec<u8>, u32)>,
}

//! Which files tgradish takes as input, by their extension, so the window's
//! dialogues, folders of frames and watched folders agree.

use std::path::Path;

/// Videos, and TIFF pictures, which only ffmpeg reads: for video and image
/// stickers.
pub const VIDEOS: &[&str] =
    &["mp4", "m4v", "mov", "mkv", "webm", "avi", "flv", "ts", "mts", "ogv", "tif", "tiff"];

/// Pictures and animations both ffmpeg and the pixel art reader read: for
/// any sticker.
pub const IMAGES: &[&str] = &["png", "apng", "gif", "webp", "jpg", "jpeg", "bmp"];

/// Aseprite files, which only the pixel art reader reads: for `.tgs`
/// stickers.
pub const ASEPRITE: &[&str] = &["ase", "aseprite"];

/// The extension of `path`, in lower case; empty without one.
pub fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Whether `path` can be a frame of an animation made of several files: a
/// picture the pixel art reader reads.
pub fn is_frame(path: &Path) -> bool {
    let extension = extension(path);
    IMAGES.contains(&extension.as_str()) || ASEPRITE.contains(&extension.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_frames_by_extension() {
        assert!(is_frame(Path::new("walk/1.PNG")) && is_frame(Path::new("walk.aseprite")));
        assert!(!is_frame(Path::new("clip.mp4")) && !is_frame(Path::new("notes")));
    }
}

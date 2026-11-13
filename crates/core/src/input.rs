//! What a conversion converts, and where its result goes unless it is
//! told: [`Input`], one file or the pictures of one animation, named for
//! its result by [`Input::default_output`]. Which files tgradish takes as
//! input, by their extension, is here too, so the window's dialogues,
//! folders of frames and watched folders agree.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::presets::Format;
use crate::telegram::Target;

/// What a conversion converts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A video, a picture, an animation or an Aseprite file. A folder is
    /// refused: its pictures are converted as a [`Input::Sequence`].
    File(PathBuf),
    /// The frames of one animation in several files: pictures, or folders
    /// of them, played in name order with numbers counted as numbers.
    Sequence(Vec<PathBuf>),
}

impl Input {
    /// The file, or the first of the sequence's files and folders, as it
    /// was given: what names the result. Empty for a sequence of nothing.
    pub fn first(&self) -> &Path {
        match self {
            Input::File(path) => path,
            Input::Sequence(inputs) => inputs.first().map_or(Path::new(""), PathBuf::as_path),
        }
    }

    /// Where the result goes unless a conversion is told: beside the input,
    /// named after it, with `.sticker` or `.emoji` and the format's
    /// extension, as `pig.mp4` makes `pig.sticker.webm`. A file is named
    /// without its extension, a folder by its whole name, and a sequence
    /// after its first file or folder, so a folder of frames and a
    /// sequence of the frames in it are named alike only when given as the
    /// folder: `walk/` makes `walk.sticker.tgs` beside it, `walk/1.png
    /// walk/2.png` makes `walk/1.sticker.tgs`. Front-ends writing results
    /// into a folder of their own keep the name.
    pub fn default_output(&self, format: Format, target: Target) -> PathBuf {
        crate::fsutil::beside(self.first(), &result_suffix(format, target))
    }

    /// Fails with [`Error::IsFolder`] for a folder given as a file.
    pub(crate) fn refuse_folder(&self) -> Result<()> {
        match self {
            Input::File(path) if path.is_dir() => Err(Error::IsFolder(path.clone())),
            _ => Ok(()),
        }
    }

    /// The files read: the file, or the sequence's pictures in the order
    /// they play, its folders' found in them.
    pub(crate) fn files(&self) -> Result<Vec<PathBuf>> {
        self.refuse_folder()?;
        match self {
            Input::File(path) => Ok(vec![path.clone()]),
            Input::Sequence(inputs) => crate::art::sequence_files(inputs),
        }
    }

    /// Fails before anything is read when writing `output` would lose a
    /// file: when it is one of the files read, even through `..` or a
    /// link, or, unless `overwrite`, when it exists. Every conversion asks
    /// this first.
    pub(crate) fn check_output(&self, output: &Path, overwrite: bool) -> Result<()> {
        if self.files()?.iter().any(|input| same_file(input, output)) {
            return Err(Error::OutputIsInput(output.to_path_buf()));
        }
        if !overwrite && output.exists() {
            return Err(Error::OutputExists(output.to_path_buf()));
        }
        Ok(())
    }
}

/// Whether `output` is the same file as `input`, following `..` and links;
/// where either doesn't exist, whether they are the same path.
fn same_file(input: &Path, output: &Path) -> bool {
    match (std::fs::canonicalize(input), std::fs::canonicalize(output)) {
        (Ok(input), Ok(output)) => input == output,
        _ => input == output,
    }
}

/// What [`Input::default_output`] puts after a result's name, as
/// `sticker.webm`.
fn result_suffix(format: Format, target: Target) -> String {
    format!("{}.{}", target.name(), format.extension())
}

/// Whether `path` is named as a result is by default, as
/// `pig.sticker.webm` or `walk.emoji.tgs`, of any format and target: what
/// a watched folder fills with, which isn't converted again.
pub fn is_result_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return false };
    let name = name.to_lowercase();
    let formats = [Format::Webm, Format::Webp, Format::Tgs];
    formats.into_iter().any(|format| {
        [Target::Sticker, Target::Emoji]
            .into_iter()
            .any(|target| name.ends_with(&format!(".{}", result_suffix(format, target))))
    })
}

/// What a file is, as tgradish tells kinds of file apart: what results
/// are for inspecting, and what reads an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileKind {
    /// WebM, or other Matroska: a video sticker, or a video.
    Webm,
    /// A WebP picture, still or animated.
    Webp { animated: bool },
    /// A `.tgs` sticker: gzip, or Lottie JSON named `.tgs`.
    Tgs,
    /// An Aseprite file.
    Aseprite,
    /// A picture or animation both ffmpeg and the pixel art reader read:
    /// PNG and APNG, GIF, JPEG or BMP.
    Picture,
    /// Anything else, which only ffmpeg might read: other videos, TIFF.
    Other,
}

impl FileKind {
    /// Whether tgradish's own pixel art reader reads it, as `.tgs`
    /// stickers are made of it; the rest are read through ffmpeg.
    pub fn reads_as_art(self) -> bool {
        matches!(self, FileKind::Webp { .. } | FileKind::Aseprite | FileKind::Picture)
    }

    /// The kind of a file starting with `start`, if they tell.
    fn of_start(start: &[u8]) -> Option<FileKind> {
        use tgradish_tgs::frames::Format;
        Some(match Format::detect(start) {
            Some(Format::WebP) => {
                // the extended header's animation flag
                let animated = start.get(12..16) == Some(b"VP8X")
                    && start.get(20).is_some_and(|flags| flags & 0x02 != 0);
                FileKind::Webp { animated }
            }
            Some(Format::Aseprite) => FileKind::Aseprite,
            Some(Format::Gif | Format::Png | Format::Jpeg | Format::Bmp) => FileKind::Picture,
            None if start.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) => FileKind::Webm,
            None if start.starts_with(&[0x1f, 0x8b]) => FileKind::Tgs,
            _ => return None,
        })
    }

    /// The kind a file named `path` is by its extension.
    fn of_name(path: &Path) -> FileKind {
        match extension(path).as_str() {
            "webm" | "mkv" => FileKind::Webm,
            "webp" => FileKind::Webp { animated: false },
            "tgs" => FileKind::Tgs,
            extension if ASEPRITE.contains(&extension) => FileKind::Aseprite,
            extension if IMAGES.contains(&extension) => FileKind::Picture,
            _ => FileKind::Other,
        }
    }
}

/// What the file at `path` is: by its first bytes, or where they tell
/// nothing, as of plain Lottie JSON, or the file can't be read, by its
/// extension.
pub fn sniff(path: &Path) -> FileKind {
    sniff_readable(path).unwrap_or_else(|_| FileKind::of_name(path))
}

/// [`sniff`] of a file that must be readable: one that can't be opened is
/// [`Error::Unreadable`].
pub(crate) fn sniff_readable(path: &Path) -> Result<FileKind> {
    let mut start = Vec::with_capacity(START_BYTES as usize);
    std::fs::File::open(path)
        .and_then(|file| {
            std::io::Read::read_to_end(&mut std::io::Read::take(file, START_BYTES), &mut start)
        })
        .map_err(|err| crate::art::unreadable(path, err))?;
    Ok(FileKind::of_start(&start).unwrap_or_else(|| FileKind::of_name(path)))
}

/// How much of a file [`sniff`] reads: enough for a WebP's extended
/// header.
const START_BYTES: u64 = 32;

/// Videos, and TIFF pictures, which only ffmpeg reads: for any sticker,
/// animated ones from the frames ffmpeg reads.
pub const VIDEOS: &[&str] =
    &["mp4", "m4v", "mov", "mkv", "webm", "avi", "flv", "ts", "mts", "ogv", "tif", "tiff"];

/// Pictures and animations both ffmpeg and the pixel art reader read: for
/// any sticker.
pub const IMAGES: &[&str] = &["png", "apng", "gif", "webp", "jpg", "jpeg", "bmp"];

/// Aseprite files, which only the pixel art reader reads: for any sticker,
/// drawn for ffmpeg for video and image ones.
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
    fn names_results_after_the_first_input() {
        let file = Input::File("clips/pig.mp4".into());
        assert_eq!(
            file.default_output(Format::Webm, Target::Sticker),
            Path::new("clips/pig.sticker.webm")
        );
        let frames = Input::Sequence(vec!["walk/2.png".into(), "walk/1.png".into()]);
        assert_eq!(
            frames.default_output(Format::Tgs, Target::Emoji),
            Path::new("walk/2.emoji.tgs")
        );
        assert_eq!(Input::Sequence(Vec::new()).first(), Path::new(""));
        for name in ["pig.sticker.webm", "PIG.EMOJI.WEBP", "walk.sticker.tgs"] {
            assert!(is_result_name(Path::new(name)), "{name}");
        }
        for name in ["pig.webm", "sticker.webm", "walk.tgs", "pig.sticker.mp4"] {
            assert!(!is_result_name(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn refuses_to_overwrite_an_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        std::fs::write(&input, b"").unwrap();
        let name = dir.path().file_name().unwrap();
        let sneaky = dir.path().join("..").join(name).join("input.mp4");
        let file = Input::File(input.clone());
        assert!(matches!(file.check_output(&sneaky, true), Err(Error::OutputIsInput(_))));
        assert!(matches!(file.check_output(&input, false), Err(Error::OutputIsInput(_))));
        // nor any frame of a sequence, found in its folder too
        let (first, second) = (dir.path().join("1.png"), dir.path().join("2.webp"));
        std::fs::write(&first, b"").unwrap();
        std::fs::write(&second, b"").unwrap();
        let frames = Input::Sequence(vec![first.clone(), second.clone()]);
        assert!(matches!(frames.check_output(&second, true), Err(Error::OutputIsInput(_))));
        let folder = Input::Sequence(vec![dir.path().to_path_buf()]);
        let other = dir.path().join(".").join("1.png");
        assert!(matches!(folder.check_output(&other, true), Err(Error::OutputIsInput(_))));
        // what isn't an input is refused only when it is there
        let output = dir.path().join("out.webm");
        assert!(file.check_output(&output, false).is_ok());
        std::fs::write(&output, b"").unwrap();
        assert!(matches!(file.check_output(&output, false), Err(Error::OutputExists(_))));
        assert!(file.check_output(&output, true).is_ok());
        // a folder given as a file is a sequence's
        let folder = Input::File(dir.path().to_path_buf());
        assert!(matches!(folder.check_output(&output, true), Err(Error::IsFolder(_))));
    }

    #[test]
    fn sniffs_by_content_then_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        let mut animated = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\x02".to_vec();
        animated.extend([0; 11]);
        // a WebP named otherwise, as a WebM and a .tgs are
        assert_eq!(sniff(&file("a.webm", &animated)), FileKind::Webp { animated: true });
        assert_eq!(sniff(&file("b", b"RIFF\0\0\0\0WEBPVP8 ")), FileKind::Webp { animated: false });
        assert_eq!(sniff(&file("c.webp", &[0x1a, 0x45, 0xdf, 0xa3, 0x9f])), FileKind::Webm);
        assert_eq!(sniff(&file("d.png", &[0x1f, 0x8b, 8])), FileKind::Tgs);
        assert_eq!(sniff(&file("e", b"\x89PNG\r\n\x1a\n")), FileKind::Picture);
        // what the start doesn't tell, by name: Lottie JSON, and videos
        assert_eq!(sniff(&file("f.tgs", b"{\"v\":\"5.5.2\"}")), FileKind::Tgs);
        assert_eq!(sniff(&file("g.mp4", b"\0\0\0\x18ftypmp42")), FileKind::Other);
        // and what can't be read
        assert_eq!(sniff(&dir.path().join("missing.ase")), FileKind::Aseprite);
        assert!(sniff_readable(&dir.path().join("missing.ase")).is_err());
        assert!(FileKind::Picture.reads_as_art() && !FileKind::Webm.reads_as_art());
    }

    #[test]
    fn tells_frames_by_extension() {
        assert!(is_frame(Path::new("walk/1.PNG")) && is_frame(Path::new("walk.aseprite")));
        assert!(!is_frame(Path::new("clip.mp4")) && !is_frame(Path::new("notes")));
    }
}

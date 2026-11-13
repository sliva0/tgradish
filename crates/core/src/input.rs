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
    fn tells_frames_by_extension() {
        assert!(is_frame(Path::new("walk/1.PNG")) && is_frame(Path::new("walk.aseprite")));
        assert!(!is_frame(Path::new("clip.mp4")) && !is_frame(Path::new("notes")));
    }
}

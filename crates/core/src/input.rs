//! Conversion inputs, default output naming, file-kind sniffing, and shared
//! supported-extension lists used by the GUI, image sequences and watch mode.

use std::path::{Path, PathBuf};

use crate::error::{Error, FileAction, Result};
use crate::presets::Format;
use crate::telegram::Target;

/// Input to a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A single video, image, animation or Aseprite file. Directories are
    /// rejected (pass them via [`Input::Sequence`]).
    File(PathBuf),
    /// Image files, or directories of them, as the frames of one animation,
    /// ordered naturally by name (`2.png` before `10.png`).
    Sequence(Vec<PathBuf>),
}

impl Input {
    /// Returns the input file, or the first entry of a sequence (empty path if
    /// the sequence is empty). Used to derive default output filenames.
    pub fn first(&self) -> &Path {
        match self {
            Input::File(path) => path,
            Input::Sequence(inputs) => inputs.first().map_or(Path::new(""), PathBuf::as_path),
        }
    }

    /// Computes the default output path beside the input:
    /// `<stem>.<target>.<ext>`
    /// (for example `pig.mp4` -> `pig.sticker.webm`).
    ///
    /// - Files drop their extension; directories keep their full name.
    /// - Sequences take the name of their first entry (`walk/` ->
    ///   `walk.sticker.tgs`, `walk/1.png walk/2.png` -> `walk/1.sticker.tgs`).
    /// - Converting an existing output (matching [`is_result_name`]) to a
    ///   different format keeps one suffix (`pig.sticker.webm` ->
    ///   `pig.sticker.webp`), while converting to the same format adds another
    ///   (`pig.sticker.sticker.webm`) to avoid overwriting itself.
    /// - Non-result inputs ending in `.sticker` or `.emoji` (such as
    ///   `pig.sticker.mp4`) still get the suffix (`pig.sticker.sticker.webm`)
    ///   so their output stays distinct from `pig.mp4`'s.
    ///
    /// Front-ends that write into their own output folder keep this file name.
    pub fn default_output(&self, format: Format, target: Target) -> PathBuf {
        let first = self.first();
        let stem = first.file_stem().and_then(|stem| stem.to_str()).map(str::to_lowercase);
        let named = !first.is_dir()
            && is_result_name(first)
            && stem.is_some_and(|stem| stem.ends_with(&format!(".{}", target.name())));
        // Case-insensitive extension check avoids colliding with the input on
        // case-insensitive filesystems.
        let itself = extension(first) == format.extension();
        let suffix = match named && !itself {
            true => format.extension().to_owned(),
            false => result_suffix(format, target),
        };
        crate::fsutil::beside(first, &suffix)
    }

    /// Returns [`Error::IsFolder`] if this is a single-file input pointing to
    /// a directory.
    pub(crate) fn refuse_folder(&self) -> Result<()> {
        match self {
            Input::File(path) if path.is_dir() => Err(Error::IsFolder(path.clone())),
            _ => Ok(()),
        }
    }

    /// Resolves all input files in playback order (expanding sequence
    /// directories).
    pub(crate) fn files(&self) -> Result<Vec<PathBuf>> {
        self.refuse_folder()?;
        match self {
            Input::File(path) => Ok(vec![path.clone()]),
            Input::Sequence(inputs) => crate::art::sequence_files(inputs),
        }
    }

    /// Runs pre-flight checks before reading: ensures all inputs exist,
    /// rejects overwriting any input (resolving symlinks and `..`), rejects
    /// existing outputs when `overwrite` is false, and checks that the output
    /// directory is writable.
    pub(crate) fn check_output(&self, output: &Path, overwrite: bool) -> Result<()> {
        let files = self.files()?;
        // Report missing files consistently rather than using ffmpeg's wording.
        if let Some(missing) = files.iter().find(|file| !file.exists()) {
            return Err(Error::NoSuchFile(missing.clone()));
        }
        if files.iter().any(|input| same_file(input, output)) {
            return Err(Error::OutputIsInput(output.to_path_buf()));
        }
        if !overwrite && output.exists() {
            return Err(Error::OutputExists(output.to_path_buf()));
        }
        crate::fsutil::check_writable(output).map_err(Error::io(FileAction::Write, output))
    }
}

/// Returns true if `input` and `output` refer to the same file (following
/// symlinks and `..`, falling back to path equality when either does not
/// exist).
fn same_file(input: &Path, output: &Path) -> bool {
    match (std::fs::canonicalize(input), std::fs::canonicalize(output)) {
        (Ok(input), Ok(output)) => input == output,
        _ => input == output,
    }
}

/// Builds the default output suffix, e.g. `"sticker.webm"`.
fn result_suffix(format: Format, target: Target) -> String {
    format!("{}.{}", target.name(), format.extension())
}

/// Returns true if `path` matches tgradish's default output naming
/// (`*.sticker.{webm,webp,tgs}` or `*.emoji.{webm,webp,tgs}`). Used by watch
/// mode to avoid re-converting outputs.
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

/// Detected file category used to pick a reader or inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileKind {
    /// WebM or Matroska container.
    Webm,
    /// Static or animated WebP image.
    Webp {
        /// True if the `VP8X` extended header sets the animation flag.
        animated: bool,
    },
    /// `.tgs` sticker (gzipped or plain Lottie JSON named `.tgs`).
    Tgs,
    /// Aseprite sprite file.
    Aseprite,
    /// Image or animation supported by both ffmpeg and the pixel-art reader
    /// (PNG/APNG, GIF, JPEG, BMP).
    Picture,
    /// Any other format that only ffmpeg might read (such as MP4, MOV, TIFF).
    Other,
}

impl FileKind {
    /// Returns true if the built-in pixel-art reader can decode this kind
    /// without ffmpeg.
    pub fn reads_as_art(self) -> bool {
        matches!(self, FileKind::Webp { .. } | FileKind::Aseprite | FileKind::Picture)
    }

    /// Detects the file kind from leading magic bytes.
    fn of_start(start: &[u8]) -> Option<FileKind> {
        use tgradish_tgs::frames::Format;
        Some(match Format::detect(start) {
            Some(Format::WebP) => {
                // Check the animation bit in the VP8X extended header.
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

    /// Guesses the file kind from its extension alone (used as a fallback when
    /// content is unreadable or lacks magic bytes; also what the GUI assumes
    /// until the file can be read).
    pub fn of_name(path: &Path) -> FileKind {
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

/// Detects the kind of file at `path` from its magic bytes, falling back to
/// its extension if magic bytes are inconclusive or the file cannot be read.
pub fn sniff(path: &Path) -> FileKind {
    sniff_readable(path).unwrap_or_else(|_| FileKind::of_name(path))
}

/// Like [`sniff`], but returns [`Error::NoSuchFile`] or [`Error::Unreadable`]
/// if the file cannot be opened.
pub(crate) fn sniff_readable(path: &Path) -> Result<FileKind> {
    let mut start = Vec::with_capacity(START_BYTES as usize);
    std::fs::File::open(path)
        .and_then(|file| {
            std::io::Read::read_to_end(&mut std::io::Read::take(file, START_BYTES), &mut start)
        })
        .map_err(|err| crate::art::read_failed(path, err))?;
    Ok(FileKind::of_start(&start).unwrap_or_else(|| FileKind::of_name(path)))
}

/// Number of leading bytes read by [`sniff`] (enough to cover a WebP `VP8X`
/// header).
const START_BYTES: u64 = 32;

/// Video and TIFF extensions decoded only by ffmpeg.
pub const VIDEOS: &[&str] =
    &["mp4", "m4v", "mov", "mkv", "webm", "avi", "flv", "ts", "mts", "ogv", "tif", "tiff"];

/// Image and animation extensions decoded by both ffmpeg and the pixel-art
/// reader.
pub const IMAGES: &[&str] = &["png", "apng", "gif", "webp", "jpg", "jpeg", "bmp"];

/// Aseprite extensions decoded by the pixel-art reader (and rendered to PNG
/// for ffmpeg).
pub const ASEPRITE: &[&str] = &["ase", "aseprite"];

/// Returns the lowercase extension of `path`, or an empty string if none.
pub fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Returns true if `path` has an image or Aseprite extension usable as a
/// sequence frame.
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
    fn names_a_result_of_a_result_with_one_suffix() {
        let named =
            |input: &str, format, target| Input::File(input.into()).default_output(format, target);
        // a video sticker made into a static one, or an animated one
        let sticker = "clips/cat.sticker.webm";
        assert_eq!(
            named(sticker, Format::Webp, Target::Sticker),
            Path::new("clips/cat.sticker.webp")
        );
        assert_eq!(
            named(sticker, Format::Tgs, Target::Sticker),
            Path::new("clips/cat.sticker.tgs")
        );
        assert_eq!(
            named("cat.emoji.webp", Format::Webm, Target::Emoji),
            Path::new("cat.emoji.webm")
        );
        // but not into itself, nor into what it isn't
        assert_eq!(
            named(sticker, Format::Webm, Target::Sticker),
            Path::new("clips/cat.sticker.sticker.webm")
        );
        assert_eq!(
            named("cat.sticker.WEBM", Format::Webm, Target::Sticker),
            Path::new("cat.sticker.sticker.webm")
        );
        assert_eq!(
            named(sticker, Format::Webp, Target::Emoji),
            Path::new("clips/cat.sticker.emoji.webp")
        );
        assert_eq!(
            named("sticker.webm", Format::Webp, Target::Sticker),
            Path::new("sticker.sticker.webp")
        );
        // whatever the case of the name
        assert_eq!(
            named("Cat.STICKER.WebM", Format::Webp, Target::Sticker),
            Path::new("Cat.STICKER.webp")
        );
        // only a result's name, as a watched folder takes it: an input that
        // merely ends so is named apart from the one without, as cat.mp4
        // makes cat.sticker.webm
        assert_eq!(
            named("cat.sticker.mp4", Format::Webm, Target::Sticker),
            Path::new("cat.sticker.sticker.webm")
        );
        assert_eq!(
            named("cat.emoji.png", Format::Webp, Target::Emoji),
            Path::new("cat.emoji.emoji.webp")
        );
        // nor a sequence's first frame
        let frames = Input::Sequence(vec!["walk/1.sticker.png".into(), "walk/2.png".into()]);
        assert_eq!(
            frames.default_output(Format::Tgs, Target::Sticker),
            Path::new("walk/1.sticker.sticker.tgs")
        );
        // nor a folder, named by its whole name
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("walk.sticker");
        std::fs::create_dir(&folder).unwrap();
        let output = Input::Sequence(vec![folder]).default_output(Format::Tgs, Target::Sticker);
        assert_eq!(output, dir.path().join("walk.sticker.sticker.tgs"));
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

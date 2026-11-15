//! The errors of the library, which front-ends print or show. Each says
//! what is wrong in neutral words, naming an option by its key, as
//! `fit-range`, where it has to: how to put it right, as by which flag or
//! setting, is the front-end's to say.

use std::path::PathBuf;

use thiserror::Error;

use crate::presets::PresetError;
use crate::webm::container::WebmError;

/// Why something the library was asked to do failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No ffmpeg could be used for work that needs one.
    #[error("ffmpeg not found: {0}")]
    FfmpegNotFound(FfmpegMissing),
    /// An ffmpeg or ffprobe process failed, or said nothing of what it
    /// was run for.
    #[error("{program} failed ({status}):\n{stderr}")]
    Ffmpeg {
        /// The program run: `ffmpeg` or `ffprobe`.
        program: &'static str,
        /// How it exited, or what its output lacked.
        status: String,
        /// The last of what it wrote to stderr.
        stderr: String,
    },
    /// A file read as WebM, as one spoofed or inspected, that isn't one.
    #[error("{} isn't a WebM file", .0.display())]
    NotWebm(PathBuf),
    /// A file inspected that is no kind of sticker.
    #[error("{} isn't a WebM, WebP or .tgs file", .0.display())]
    NotSticker(PathBuf),
    /// An input that isn't there: said the same whatever was to read it.
    #[error("{}: no such file", .0.display())]
    NoSuchFile(PathBuf),
    /// An input that couldn't be read: of a kind nothing here reads,
    /// broken, or not to be opened.
    #[error("couldn't read {}: {message}", path.display())]
    Unreadable {
        /// The input.
        path: PathBuf,
        /// Why, in English.
        message: String,
    },
    /// A `.tgs` that can't be shown as tgradish draws its own: one with
    /// more than tgradish's stickers have, as paths, gradients or masks.
    #[error("{} has {what}, which tgradish doesn't draw", path.display())]
    Undrawable {
        /// The sticker.
        path: PathBuf,
        /// What it has, in English.
        what: String,
    },
    /// The input has no video stream.
    #[error("{} has no video stream", .0.display())]
    NoVideo(PathBuf),
    /// Nothing of the input was decoded from where it was to start, as
    /// when the start is past its last frame.
    #[error("{} has no frame at {at:.3} s", path.display())]
    NoFrame {
        /// The input.
        path: PathBuf,
        /// Where reading started, in seconds.
        at: f64,
    },
    /// An option's value can't be used: `option` is its key, as presets and
    /// `describe` name it, and `reason` follows it in the message, as in
    /// "crf must be at most 63".
    #[error("{option} {reason}")]
    InvalidOption {
        /// The option's key.
        option: &'static str,
        /// What is wrong with its value.
        reason: String,
    },
    /// `ffmpeg-args` were given with the built-in ffmpeg, which has no
    /// command line to add them to.
    #[error(
        "ffmpeg-args are arguments for ffmpeg's command line, which the built-in ffmpeg \
         doesn't have; encoder-options work with either"
    )]
    FfmpegArgsNeedProgram,
    /// The config file couldn't be read as one, or written.
    #[error("config {}: {message}", path.display())]
    Config {
        /// The config file.
        path: PathBuf,
        /// What is wrong with it, in English.
        message: String,
    },
    /// A preset couldn't be found, read, saved, renamed or removed.
    #[error(transparent)]
    Preset(#[from] PresetError),
    /// The output is one of the inputs, even through another path or a
    /// link, and writing it would destroy that input.
    #[error("the output {} is an input", .0.display())]
    OutputIsInput(PathBuf),
    /// A folder was given as one file to convert; the pictures in it are
    /// converted as a sequence.
    #[error("{} is a folder, whose pictures are converted as a sequence", .0.display())]
    IsFolder(PathBuf),
    /// No inputs were given, or a sequence's folders hold no images.
    #[error("there are no images to convert")]
    NothingToConvert,
    /// The output, or a preset's file, is there already, and wasn't to be
    /// replaced.
    #[error("{} already exists", .0.display())]
    OutputExists(PathBuf),
    /// Fitting a WebM found no encoding within the size limit.
    #[error(
        "nothing fits the {limit} byte limit, the smallest attempt was {smallest} bytes; \
         a shorter length, a lower frame rate or a wider fit-range can help"
    )]
    NothingFits {
        /// The smallest encoding tried, in bytes; 0 when none was made.
        smallest: u64,
        /// The limit, in bytes.
        limit: u64,
    },
    /// The [`CancelToken`](crate::CancelToken) said to stop.
    #[error("cancelled")]
    Cancelled,
    /// The built-in ffmpeg's libraries failed: what was being done, and
    /// their message.
    #[error("ffmpeg: {0}")]
    Libav(String),
    /// Nothing on the clipboard could be read as an input, or saved as one.
    #[error("couldn't paste from the clipboard: {0}")]
    Clipboard(ClipboardError),
    /// A WebM's container couldn't be inspected or patched.
    #[error(transparent)]
    Webm(#[from] WebmError),
    /// A `.tgs` couldn't be made.
    #[error(transparent)]
    Tgs(#[from] tgradish_tgs::Error),
    /// libwebp couldn't encode an image sticker: what failed.
    #[error("couldn't encode the WebP: {0}")]
    Webp(String),
    /// Something done to a file failed, as writing the result: `action`
    /// says what, and `error` why, said as [`io_reason`] says it.
    #[error("couldn't {action} {}: {}", path.display(), io_reason(error))]
    Io {
        /// What was being done.
        action: FileAction,
        /// To which file or folder, or the program run.
        path: PathBuf,
        /// What the system said.
        error: std::io::Error,
    },
}

/// Why `error` happened, as words to follow a colon: the system's, in
/// lower case, without the "(os error 13)" Rust adds, nor what a library
/// adds after it, as tempfile does the temporary file's path.
pub fn io_reason(error: &std::io::Error) -> String {
    let mut text = error.to_string();
    if let Some(end) = text.find(" (os error ") {
        text.truncate(end);
    }
    lower_first(&text)
}

/// `text` with its first letter in lower case, unless it starts a word in
/// capitals, as "UTF-8".
pub(crate) fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(first), second) if !second.is_some_and(char::is_uppercase) => {
            first.to_lowercase().chain(text[first.len_utf8()..].chars()).collect()
        }
        _ => text.to_owned(),
    }
}

impl Error {
    /// A function making an [`Error::Io`] of an `io::Error` met in doing
    /// `action` to `path`, for `map_err`.
    pub(crate) fn io(
        action: FileAction,
        path: impl Into<PathBuf>,
    ) -> impl FnOnce(std::io::Error) -> Error {
        let path = path.into();
        move |error| Error::Io { action, path, error }
    }

    /// An [`Error::InvalidOption`] of the option `option`.
    pub(crate) fn invalid(option: &'static str, reason: impl Into<String>) -> Error {
        Error::InvalidOption { option, reason: reason.into() }
    }
}

/// What was being done to a file when it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileAction {
    /// Reading a file.
    Read,
    /// Writing a file.
    Write,
    /// Making a folder, or a temporary one.
    Create,
    /// Listing what a folder holds.
    List,
    /// Renaming a file.
    Rename,
    /// Deleting a file.
    Remove,
    /// Starting a program, as ffmpeg.
    Run,
}

impl std::fmt::Display for FileAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FileAction::Read => "read",
            FileAction::Write => "write",
            FileAction::Create => "create",
            FileAction::List => "list",
            FileAction::Rename => "rename",
            FileAction::Remove => "remove",
            FileAction::Run => "run",
        })
    }
}

/// Why no ffmpeg could be used.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum FfmpegMissing {
    /// There is no ffmpeg, or ffprobe, where a path said.
    #[error("{} doesn't exist", .0.display())]
    NoProgram(PathBuf),
    /// There is no ffprobe beside the ffmpeg a path gave.
    #[error("ffprobe is missing, expected it at {}", .0.display())]
    NoFfprobe(PathBuf),
    /// Neither ffmpeg and ffprobe is on PATH.
    #[error("ffmpeg and ffprobe are not on PATH")]
    NotOnPath,
    /// The built-in ffmpeg was asked for, which this build doesn't have.
    #[error("this tgradish build has no built-in ffmpeg")]
    NotBuiltIn,
    /// A video was to be read, but no ffmpeg was given to read it.
    #[error("{} is a video, which ffmpeg reads", .0.display())]
    ForVideo(PathBuf),
}

/// Why nothing could be pasted, said to follow "couldn't paste from the
/// clipboard: ", where "it" is the clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ClipboardError {
    /// It holds nothing that can be an input.
    #[error("it has no files, file paths or image")]
    Empty,
    /// It couldn't be read: what the system's clipboard said.
    #[error("{0}")]
    Unreadable(String),
    /// The image on it couldn't be saved as a file: what the encoder said.
    #[error("couldn't save the image: {0}")]
    ImageUnsaved(String),
}

/// What the library's functions return.
pub type Result<T> = std::result::Result<T, Error>;

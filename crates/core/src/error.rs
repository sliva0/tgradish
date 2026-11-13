//! The errors of the library, which front-ends print or show. Each says
//! what is wrong in neutral words, naming an option by its key, as
//! `fit-range`, where it has to: how to put it right, as by which flag or
//! setting, is the front-end's to say.

use std::path::PathBuf;

use thiserror::Error;

use crate::presets::PresetError;
use crate::webm::container::WebmError;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("ffmpeg not found: {0}")]
    FfmpegNotFound(FfmpegMissing),
    #[error("{program} failed ({status}):\n{stderr}")]
    Ffmpeg { program: &'static str, status: String, stderr: String },
    /// An input that couldn't be read: missing, of a kind nothing here
    /// reads, or broken.
    #[error("couldn't read {}: {message}", path.display())]
    Unreadable { path: PathBuf, message: String },
    #[error("{} has no video stream", .0.display())]
    NoVideo(PathBuf),
    #[error("{} has no frame at {at:.3} s", path.display())]
    NoFrame { path: PathBuf, at: f64 },
    /// An option's value can't be used: `option` is its key, as presets and
    /// `describe` name it, and `reason` follows it in the message, as in
    /// "crf must be at most 63".
    #[error("{option} {reason}")]
    InvalidOption { option: &'static str, reason: String },
    /// `ffmpeg-args` were given with the built-in ffmpeg, which has no
    /// command line to add them to.
    #[error(
        "ffmpeg-args are arguments for ffmpeg's command line, which the built-in ffmpeg \
         doesn't have; encoder-options work with either"
    )]
    FfmpegArgsNeedProgram,
    /// The config file couldn't be read as one, or written.
    #[error("config {}: {message}", path.display())]
    Config { path: PathBuf, message: String },
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
    #[error("{} already exists", .0.display())]
    OutputExists(PathBuf),
    #[error(
        "nothing fits the {limit} byte limit, the smallest attempt was {smallest} bytes; \
         a shorter length, a lower frame rate or a wider fit-range can help"
    )]
    NothingFits { smallest: u64, limit: u64 },
    #[error("cancelled")]
    Cancelled,
    #[error("ffmpeg: {0}")]
    Libav(String),
    #[error("couldn't paste from the clipboard: {0}")]
    Clipboard(String),
    #[error(transparent)]
    Webm(#[from] WebmError),
    #[error(transparent)]
    Tgs(#[from] tgradish_tgs::Error),
    #[error("couldn't encode the WebP: {0}")]
    Webp(String),
    /// Something done to a file failed, as writing the result: `action`
    /// says what, and `error` why.
    #[error("couldn't {action} {}: {error}", path.display())]
    Io { action: FileAction, path: PathBuf, error: std::io::Error },
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
    Read,
    Write,
    /// Making a folder, or a temporary one.
    Create,
    /// Listing what a folder holds.
    List,
    Rename,
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
    #[error("{} does not exist", .0.display())]
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

pub type Result<T> = std::result::Result<T, Error>;

//! Library error types reported to front-ends.
//!
//! Messages use neutral wording and refer to options by their key (such as
//! `fit-range`) rather than CLI flags or GUI controls.

use std::path::PathBuf;

use thiserror::Error;

use crate::presets::PresetError;
use crate::webm::container::WebmError;

/// Error returned by `tgradish-core` operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No usable ffmpeg installation was available.
    #[error("ffmpeg not found: {0}")]
    FfmpegNotFound(FfmpegMissing),
    /// An external `ffmpeg` or `ffprobe` process failed or produced unusable
    /// output.
    #[error("{program} failed ({status}):\n{stderr}")]
    Ffmpeg {
        /// Executable name (`ffmpeg` or `ffprobe`).
        program: &'static str,
        /// Exit status or summary of missing output.
        status: String,
        /// Tail of the process stderr.
        stderr: String,
    },
    /// A file expected to be WebM (for inspection or spoofing) was not WebM.
    #[error("{} is not a WebM file", .0.display())]
    NotWebm(PathBuf),
    /// A file passed to `inspect` is not a supported sticker format.
    #[error("{} is not a WebM, WebP or .tgs file", .0.display())]
    NotSticker(PathBuf),
    /// Input file does not exist (reported the same way whichever reader found
    /// it).
    #[error("{}: no such file", .0.display())]
    NoSuchFile(PathBuf),
    /// Input file could not be read or decoded.
    #[error("failed to read {}: {message}", path.display())]
    Unreadable {
        /// Path to the unreadable input.
        path: PathBuf,
        /// Human-readable reason.
        message: String,
    },
    /// `.tgs` file has Lottie features that tgradish's built-in preview
    /// renderer cannot draw (such as paths, gradients or masks).
    #[error("{} has {what}, which tgradish cannot draw", path.display())]
    Undrawable {
        /// Path to the `.tgs` file.
        path: PathBuf,
        /// Unsupported Lottie feature description.
        what: String,
    },
    /// Input file contains no video stream.
    #[error("{} has no video stream", .0.display())]
    NoVideo(PathBuf),
    /// No video frame could be decoded from the requested start timestamp (for
    /// example when `start` is past the last frame).
    #[error("{} has no frame at {at:.3} s", path.display())]
    NoFrame {
        /// Path to the input file.
        path: PathBuf,
        /// Requested start time in seconds.
        at: f64,
    },
    /// Option value failed validation. `option` is the canonical key (as used
    /// in presets and `describe`), and `reason` completes the message (e.g.
    /// `"crf must be at most 63"`).
    #[error("{option} {reason}")]
    InvalidOption {
        /// Canonical option key.
        option: &'static str,
        /// Validation failure description.
        reason: String,
    },
    /// `ffmpeg-args` was used with the built-in ffmpeg backend, which has no
    /// command line.
    #[error(
        "ffmpeg-args are arguments for ffmpeg's command line, which the built-in ffmpeg \
         does not have; encoder-options work with either"
    )]
    FfmpegArgsNeedProgram,
    /// Config file could not be parsed or serialised.
    #[error("config {}: {message}", path.display())]
    Config {
        /// Path to the config file.
        path: PathBuf,
        /// Failure reason.
        message: String,
    },
    /// Preset loading, lookup, saving, renaming or deletion failed.
    #[error(transparent)]
    Preset(#[from] PresetError),
    /// Output path resolves (through any path or link) to one of the inputs,
    /// which writing it would destroy.
    #[error("output {} is the same as an input file", .0.display())]
    OutputIsInput(PathBuf),
    /// A directory was passed as a single-file input instead of a sequence.
    #[error("{} is a directory; its images can be converted as a sequence", .0.display())]
    IsFolder(PathBuf),
    /// No inputs were provided, or sequence directories contained no images.
    #[error("no images to convert")]
    NothingToConvert,
    /// Target file already exists and overwriting was not enabled.
    #[error("{} already exists", .0.display())]
    OutputExists(PathBuf),
    /// WebM size-fitting ended with no attempt under the size limit.
    #[error(
        "could not fit within {limit} bytes (smallest attempt was {smallest} bytes); try a \
         shorter length, lower frame rate or wider fit-range"
    )]
    NothingFits {
        /// Smallest attempt size in bytes (0 if no attempt succeeded).
        smallest: u64,
        /// Target size limit in bytes.
        limit: u64,
    },
    /// Operation was stopped via [`CancelToken`](crate::CancelToken).
    #[error("cancelled")]
    Cancelled,
    /// Built-in ffmpeg library call failed.
    #[error("ffmpeg: {0}")]
    Libav(String),
    /// Reading or saving clipboard contents failed.
    #[error("failed to paste from clipboard: {0}")]
    Clipboard(ClipboardError),
    /// WebM container parsing or patching failed.
    #[error(transparent)]
    Webm(#[from] WebmError),
    /// A `.tgs` sticker could not be built.
    #[error(transparent)]
    Tgs(#[from] tgradish_tgs::Error),
    /// Static WebP encoding failed.
    #[error("failed to encode WebP: {0}")]
    Webp(String),
    /// Filesystem or process I/O error, formatted with [`io_reason`].
    #[error("failed to {action} {}: {}", path.display(), io_reason(error))]
    Io {
        /// Operation being performed.
        action: FileAction,
        /// Target file, directory or executable path.
        path: PathBuf,
        /// Underlying OS error.
        error: std::io::Error,
    },
}

/// Formats an [`std::io::Error`] as a short lowercase phrase without Rust's
/// trailing `(os error N)` suffix or extra library suffixes.
pub fn io_reason(error: &std::io::Error) -> String {
    let mut text = error.to_string();
    if let Some(end) = text.find(" (os error ") {
        text.truncate(end);
    }
    lower_first(&text)
}

/// Lowercases the first character of `text` unless it starts an all-caps prefix
/// such as `"UTF-8"`.
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
    /// Returns a closure mapping an `io::Error` into [`Error::Io`] for
    /// `action` on `path`.
    pub(crate) fn io(
        action: FileAction,
        path: impl Into<PathBuf>,
    ) -> impl FnOnce(std::io::Error) -> Error {
        let path = path.into();
        move |error| Error::Io { action, path, error }
    }

    /// Constructs [`Error::InvalidOption`] for `option`.
    pub(crate) fn invalid(option: &'static str, reason: impl Into<String>) -> Error {
        Error::InvalidOption { option, reason: reason.into() }
    }
}

/// Filesystem or process operation that failed in [`Error::Io`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileAction {
    /// Reading a file.
    Read,
    /// Writing a file.
    Write,
    /// Creating a directory or temporary directory.
    Create,
    /// Listing directory entries.
    List,
    /// Renaming a file.
    Rename,
    /// Removing a file.
    Remove,
    /// Running an external program.
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

/// Reason why ffmpeg was unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum FfmpegMissing {
    /// Configured ffmpeg or ffprobe path does not exist.
    #[error("{} does not exist", .0.display())]
    NoProgram(PathBuf),
    /// `ffprobe` was not found beside the configured `ffmpeg` binary.
    #[error("ffprobe not found at {}", .0.display())]
    NoFfprobe(PathBuf),
    /// `ffmpeg` or `ffprobe` (or both) was not found on `PATH`.
    #[error("ffmpeg or ffprobe was not found on PATH")]
    NotOnPath,
    /// Built-in ffmpeg was requested, but this binary was built without it.
    #[error("this build of tgradish does not include a built-in ffmpeg")]
    NotBuiltIn,
    /// Input is a video, which requires ffmpeg.
    #[error("{} is a video and requires ffmpeg", .0.display())]
    ForVideo(PathBuf),
    /// Requested output format requires ffmpeg (WebM, or WebP, which uses
    /// ffmpeg to read and scale the image).
    #[error("{} output requires ffmpeg", .0.name())]
    ForFormat(crate::presets::Format),
}

/// Reason why pasting from the clipboard failed (formatted after `"failed to
/// paste from clipboard: "`).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ClipboardError {
    /// Clipboard contains no supported input.
    #[error("contains no files, file paths or image")]
    Empty,
    /// System clipboard could not be read.
    #[error("{0}")]
    Unreadable(String),
    /// Pasted image could not be encoded as PNG.
    #[error("failed to save image: {0}")]
    ImageUnsaved(String),
}

/// Result of the library's functions, with [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;

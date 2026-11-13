//! The errors of the library, which front-ends print or show.

use std::path::PathBuf;

use thiserror::Error;

use crate::presets::PresetError;
use crate::webm::WebmError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("ffmpeg not found: {0}")]
    FfmpegNotFound(String),
    #[error("{program} failed ({status}):\n{stderr}")]
    Ffmpeg { program: &'static str, status: String, stderr: String },
    #[error("couldn't read {}: {message}", path.display())]
    Probe { path: PathBuf, message: String },
    #[error("{} has no video stream", .0.display())]
    NoVideo(PathBuf),
    #[error("{} has no frame at {at:.3} s", path.display())]
    NoFrame { path: PathBuf, at: f64 },
    /// An option's value can't be used: `option` is its key, as presets and
    /// `describe` name it, and `reason` follows it in the message, as in
    /// "crf must be at most 63".
    #[error("{option} {reason}")]
    InvalidOption { option: &'static str, reason: String },
    /// `extra-args` were given with the built-in ffmpeg, which has no
    /// command line to add them to.
    #[error(
        "extra-args are ffmpeg command line arguments, so they need ffmpeg as a \
         separate program: use --ffmpeg-from system or --ffmpeg PATH, or \
         --encoder-options for encoder settings, which work with the built-in ffmpeg"
    )]
    ExtraArgsNeedProgram,
    /// The config file couldn't be read as one, or written.
    #[error("config {}: {message}", path.display())]
    Config { path: PathBuf, message: String },
    #[error(transparent)]
    Preset(#[from] PresetError),
    /// The output is one of the inputs, even through another path or a
    /// link, and writing it would destroy that input.
    #[error("the output {} is an input", .0.display())]
    OutputIsInput(PathBuf),
    /// No inputs were given, or a sequence's folders hold no images.
    #[error("there are no images to convert")]
    NothingToConvert,
    #[error("{} already exists", .0.display())]
    OutputExists(PathBuf),
    #[error(
        "nothing fits the {limit} byte limit, the smallest attempt was {smallest} bytes; \
         try a shorter length, a lower frame rate or a wider --fit-range"
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
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    /// An [`Error::InvalidOption`] of the option `option`.
    pub(crate) fn invalid(option: &'static str, reason: impl Into<String>) -> Error {
        Error::InvalidOption { option, reason: reason.into() }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

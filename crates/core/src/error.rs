use std::path::PathBuf;

use thiserror::Error;

use crate::webm::WebmError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("ffmpeg not found: {0}")]
    FfmpegNotFound(String),
    #[error("{program} failed ({status}):\n{stderr}")]
    Ffmpeg { program: &'static str, status: String, stderr: String },
    #[error("could not read {path}: {message}")]
    Probe { path: PathBuf, message: String },
    #[error("{0} has no video stream")]
    NoVideo(PathBuf),
    #[error("invalid options: {0}")]
    InvalidOptions(String),
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
    #[error(transparent)]
    Webm(#[from] WebmError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

//! Telegram animated stickers (`.tgs`, gzipped Lottie JSON) from pixel art
//! animations.
//!
//! Pure Rust that builds for `wasm32-unknown-unknown`: no ffmpeg,
//! processes, filesystem or `tgradish-core`. The `threads` feature tries
//! fitting's candidates, and packs, in threads of their own, except on
//! wasm; [`sticker::make`] says what that means for callers.

use std::time::Duration;

use thiserror::Error;

pub mod check;
pub mod encode;
pub mod file;
pub mod layout;
pub mod limits;
pub mod lottie;
pub mod mark;
pub mod normalise;
pub mod reduce;
pub mod scene;
pub mod sticker;

pub use normalise::{PixelAnim, normalise};
pub use tgradish_frames as frames;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum Error {
    #[error("the start ({start:?}) is not before the end of the input ({length:?})")]
    StartPastEnd { start: Duration, length: Duration },
    #[error("the length is zero")]
    ZeroLength,
    #[error("the tempo {0} is not from 0.1 to 10")]
    Tempo(f64),
    #[error("every pixel is transparent")]
    Invisible,
    #[error(
        "the crop {}x{}+{}+{} is not within the {width}x{height} input",
        crop.width, crop.height, crop.x, crop.y
    )]
    CropOutside { crop: normalise::Rect, width: u32, height: u32 },
    #[error("the pixel scale must be at least 1")]
    ZeroScale,
    #[error("the downscale factor {0} is not between 0 and 1")]
    DownscaleFactor(f64),
    #[error("the colour distance {0} is not a number from 0 up")]
    ColourDistance(f64),
    #[error("the share of the picture that may change, {0}, is not from 0 to 1")]
    ChangedShare(f64),
    #[error("the share of frames to drop, {0}, is not from 0 to 1")]
    DroppedShare(f64),
    #[error("more than 65535 colours: this is not pixel art")]
    TooManyColours,
    #[error(transparent)]
    Encode(encode::EncodeError),
    #[error("cancelled")]
    Cancelled,
}

impl From<encode::EncodeError> for Error {
    /// A stopped encode is a stopped sticker, so callers check for one
    /// [`Error::Cancelled`] whatever stopped.
    fn from(err: encode::EncodeError) -> Error {
        match err {
            encode::EncodeError::Cancelled => Error::Cancelled,
            err => Error::Encode(err),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

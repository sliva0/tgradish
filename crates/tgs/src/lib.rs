//! Telegram animated stickers (`.tgs`, gzipped Lottie JSON) from pixel art
//! animations: the art is read on its own pixel grid ([`normalise()`]),
//! drawn as rectangles in layers, reduced until it fits Telegram's 64 KiB
//! when that is allowed ([`reduce`]), packed, and checked against
//! Telegram's limits ([`check`]). [`make`] does all of it:
//!
//! ```
//! use tgradish_tgs::frames::{DecodeOptions, Limits, decode};
//! use tgradish_tgs::{Options, make};
//!
//! // a GIF of one black pixel
//! let gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;";
//! let animation = decode(gif, &DecodeOptions::default(), &Limits::default())?;
//! let sticker = make(&animation, &Options::default(), &mut |_progress| {}, &|| false)?;
//! assert!(sticker.fits());
//! // the .tgs file, gzipped
//! assert_eq!(sticker.tgs[..2], [0x1f, 0x8b]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Pure Rust that builds for `wasm32-unknown-unknown`: no ffmpeg,
//! processes, filesystem or `tgradish-core`. The `threads` feature tries
//! fitting's candidates, and packs, in threads of their own, except on
//! wasm; [`make`] says what that means for callers. The `schema` feature
//! gives JSON Schemas of its options and reports, which are all written in
//! kebab-case.
//!
//! The `internals` feature makes the encoder's interior public, for
//! tgradish's own research tools: it is unstable, and exempt from semver.

#![warn(missing_docs)]

use std::time::Duration;

use thiserror::Error;

/// Declares a function of the interior: public with the `internals`
/// feature, the crate's own without it.
macro_rules! internal {
    ($(#[$attr:meta])* pub fn $($rest:tt)*) => {
        #[cfg(feature = "internals")]
        $(#[$attr])* pub fn $($rest)*
        #[cfg(not(feature = "internals"))]
        $(#[$attr])* pub(crate) fn $($rest)*
    };
}

pub mod check;
pub mod encode;
pub mod file;
pub mod limits;
// tgradish-core's own, and none of the API it promises
#[doc(hidden)]
pub mod mark;
pub mod normalise;
pub mod reduce;
pub mod sticker;

mod grid;

// the interior, with the `internals` feature
#[cfg(feature = "internals")]
pub mod layout;
#[cfg(not(feature = "internals"))]
mod layout;
#[cfg(feature = "internals")]
pub mod lottie;
#[cfg(not(feature = "internals"))]
mod lottie;
#[cfg(feature = "internals")]
pub mod scene;
#[cfg(not(feature = "internals"))]
mod scene;

pub use normalise::{PixelAnim, normalise};
pub use sticker::{Options, Progress, Sticker, make};
pub use tgradish_frames as frames;

/// Why a sticker couldn't be made.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum Error {
    /// [`normalise::Options::start`] is at or past the end of the input.
    #[error("the start ({start:?}) is not before the end of the input ({length:?})")]
    StartPastEnd {
        /// Where it was to start.
        start: Duration,
        /// How long the input is.
        length: Duration,
    },
    /// [`normalise::Options::length`] is zero.
    #[error("the length is zero")]
    ZeroLength,
    /// A tempo outside [`normalise::TEMPOS`].
    #[error("the tempo {0} is not from 0.1 to 10")]
    Tempo(f64),
    /// Nothing to see, which is no sticker.
    #[error("every pixel is transparent")]
    Invisible,
    /// A crop that reaches outside the input.
    #[error(
        "the crop {}x{}+{}+{} is not within the {width}x{height} input",
        crop.width, crop.height, crop.x, crop.y
    )]
    CropOutside {
        /// The crop asked for.
        crop: normalise::Rect,
        /// The input's width.
        width: u32,
        /// The input's height.
        height: u32,
    },
    /// A pixel scale of 0, given or in [`reduce::Reduction::SnapToGrid`].
    #[error("the pixel scale must be at least 1")]
    ZeroScale,
    /// [`reduce::Reduction::Downscale`] by a factor outside its range.
    #[error("the downscale factor {0} is not between 0 and 1")]
    DownscaleFactor(f64),
    /// [`reduce::Reduction::MergeColours`] by a distance outside its range.
    #[error("the colour distance {0} is not a number from 0 up")]
    ColourDistance(f64),
    /// [`reduce::Reduction::MergeFrames`] by a share outside its range.
    #[error("the share of the picture that may change, {0}, is not from 0 to 1")]
    ChangedShare(f64),
    /// [`reduce::Reduction::DropFrames`] by a share outside its range.
    #[error("the share of frames to drop, {0}, is not from 0 to 1")]
    DroppedShare(f64),
    /// More colours than a palette index holds.
    #[error("more than 65535 colours: this is not pixel art")]
    TooManyColours,
    /// The encoder couldn't draw the art within Telegram's limits.
    #[error(transparent)]
    Encode(encode::EncodeError),
    /// [`sticker::Cancelled`] said to stop.
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

/// What this crate's functions return.
pub type Result<T> = std::result::Result<T, Error>;

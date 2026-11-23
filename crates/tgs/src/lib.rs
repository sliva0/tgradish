//! Converts pixel art animations into Telegram animated stickers (`.tgs`,
//! gzipped Lottie JSON) in pure Rust, checks `.tgs` files, and renders
//! previews of its own output.
//!
//! Input art is decoded from GIF, PNG/APNG, WebP, JPEG, BMP, Aseprite files,
//! sprite sheets, or image sequences ([`frames`]), normalised onto its own
//! pixel grid ([`normalise()`]), drawn as layered rectangles, reduced until it
//! fits Telegram's 64 KiB limit when permitted ([`reduce`]), compressed, and
//! checked against Telegram's limits ([`check`]). [`make`] runs every step
//! after decoding:
//!
//! ```
//! use tgradish_tgs::frames::{DecodeOptions, Limits, decode};
//! use tgradish_tgs::{Options, make};
//!
//! // A 1x1 black pixel GIF.
//! let gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;";
//! let animation = decode(gif, &DecodeOptions::default(), &Limits::default())?;
//! let sticker = make(animation, &Options::default(), &mut |_progress| {}, &|| false)?;
//! assert!(sticker.fits());
//! // Gzipped .tgs output.
//! assert_eq!(sticker.tgs[..2], [0x1f, 0x8b]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [`check`] validates any Lottie document against Telegram's limits; unpack a
//! `.tgs` file first with [`file::unpack`]. [`picture`] renders preview frames
//! from either the normalised art or the Lottie JSON of a `.tgs` file that
//! tgradish wrote.
//!
//! Decoding memory is bounded by [`frames::Limits`], so hostile inputs fail
//! instead of exhausting RAM. Aseprite files can be inspected for tags and
//! layers first with [`frames::aseprite::Sprite`].
//!
//! This crate is pure Rust and compiles for `wasm32-unknown-unknown`: it has
//! no ffmpeg, subprocess, filesystem, or `tgradish-core` dependencies. The
//! `threads` feature runs candidate fitting and compression in parallel
//! threads (except on wasm); see [`make`] for caller requirements. The
//! `schema` feature derives JSON Schemas for options and reports (all
//! serialised in kebab-case).
//!
//! The `internals` feature exposes internal encoder modules for tgradish's own
//! research tools. It is unstable and exempt from semver.

#![warn(missing_docs)]
// Annotate feature-gated items on docs.rs.
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::time::Duration;

use thiserror::Error;

/// Declares an internal function: `pub` with the `internals` feature,
/// `pub(crate)` without it.
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
pub mod frames;
pub mod limits;
// Internal helper for tgradish-core; not part of the public API.
#[doc(hidden)]
pub mod mark;
pub mod normalise;
pub mod picture;
pub mod reduce;
pub mod sticker;

mod grid;

// Internal modules exposed by the `internals` feature.
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

/// Error returned when a sticker cannot be built.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum Error {
    /// [`normalise::Options::start`] is at or past the end of the input.
    #[error("start ({start:?}) must be before the end of the input ({length:?})")]
    StartPastEnd {
        /// Requested start time.
        start: Duration,
        /// Total input duration.
        length: Duration,
    },
    /// [`normalise::Options::length`] is zero.
    #[error("length must be greater than zero")]
    ZeroLength,
    /// Tempo is outside [`normalise::TEMPOS`].
    #[error("tempo {0} must be between 0.1 and 10")]
    Tempo(f64),
    /// Every pixel is transparent, in the input or after cropping, squaring or
    /// snapping.
    #[error("every pixel is transparent")]
    Invisible,
    /// The requested crop extends outside the input bounds or is empty.
    #[error(
        "crop {}x{}+{}+{} does not fit within the {width}×{height} input",
        crop.width, crop.height, crop.x, crop.y
    )]
    CropOutside {
        /// Requested crop rectangle.
        crop: normalise::Rect,
        /// Input width in pixels.
        width: u32,
        /// Input height in pixels.
        height: u32,
    },
    /// Pixel scale is 0 (in options or [`reduce::Reduction::SnapToGrid`]).
    #[error("pixel scale must be at least 1")]
    ZeroScale,
    /// [`reduce::Reduction::Downscale`] factor is outside `(0, 1)`.
    #[error("downscale factor {0} must be between 0 and 1")]
    DownscaleFactor(f64),
    /// [`reduce::Reduction::MergeColours`] distance is negative or non-finite.
    #[error("colour distance {0} must be a finite number, 0 or greater")]
    ColourDistance(f64),
    /// [`reduce::Reduction::MergeFrames`] share is outside `0..=1`.
    #[error("share of pixels that may change ({0}) must be between 0 and 1")]
    ChangedShare(f64),
    /// [`reduce::Reduction::DropFrames`] share is outside `0..=1`.
    #[error("dropped frame share {0} must be between 0 and 1")]
    DroppedShare(f64),
    /// The input has more distinct colours than a `u16` palette can hold.
    #[error("more than 65535 colours (input is not pixel art)")]
    TooManyColours,
    /// The encoder could not represent the art within Telegram's limits.
    #[error(transparent)]
    Encode(encode::EncodeError),
    /// [`sticker::Cancelled`] requested cancellation.
    #[error("cancelled")]
    Cancelled,
}

impl From<encode::EncodeError> for Error {
    /// Maps encoder cancellation to [`Error::Cancelled`] so callers only check
    /// one variant.
    fn from(err: encode::EncodeError) -> Error {
        match err {
            encode::EncodeError::Cancelled => Error::Cancelled,
            err => Error::Encode(err),
        }
    }
}

/// Result type for this crate.
pub type Result<T> = std::result::Result<T, Error>;

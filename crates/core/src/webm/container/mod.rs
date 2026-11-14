//! WebM inspection and in-place metadata patching.
//!
//! Patching never changes the file length. The Segment Info element is
//! rebuilt inside the space it already occupies plus any padding (Void
//! elements) right next to it. ffmpeg leaves such padding after the SeekHead,
//! so Info can grow by a few dozen bytes. If Info has to move, the SeekHead
//! entry pointing at it is updated. Nothing after Info moves.
//!
//! Spoofing is such a patch: Telegram refuses video stickers longer than 3
//! seconds by the Duration in Segment Info, while players play every frame
//! whatever it says. So a shorter Duration gets a longer sticker through.
//! ffmpeg also writes a DURATION tag for each track, patched to match, so
//! tools reading those see the same; [`WebmInfo::content_duration`] keeps
//! the truth, from the frames' own timestamps. Byte offsets in the SeekHead
//! and the Cues are why the file's length must never change.

pub(crate) mod ebml;
mod inspect;
mod patch;
#[cfg(test)]
mod tests;

pub use ebml::EbmlError;
pub(crate) use inspect::head_file;
pub use inspect::{MAX_INSPECTED_BYTES, VideoTrack, WebmInfo, inspect, inspect_file};
pub use patch::{Patch, PatchReport, patch, patch_file};

use thiserror::Error;

/// Default TimestampScale: durations are in milliseconds.
const DEFAULT_TIMESTAMP_SCALE: u64 = 1_000_000;

/// What is wrong with a WebM's bytes, for reading or patching them.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WebmError {
    /// Its EBML doesn't parse.
    #[error("malformed WebM: {0}")]
    Ebml(#[from] EbmlError),
    /// It doesn't start with an EBML header.
    #[error("not a WebM file: no EBML header")]
    NotEbml,
    /// It has no Segment, which holds everything else.
    #[error("no Segment element found")]
    NoSegment,
    /// Its Segment has no Info, which holds the duration.
    #[error("no Info element found in the Segment")]
    NoInfo,
    /// A fake duration, in seconds, not above 0 or past
    /// [`MAX_FAKE_DURATION`].
    #[error("fake duration must be more than 0 and at most {MAX_FAKE_DURATION} seconds, got {0}")]
    InvalidDuration(f64),
    /// Its `TimestampScale` is 0, so no duration can be written in it.
    #[error("the file has an invalid TimestampScale")]
    InvalidTimestampScale,
    /// A value to write is longer than an EBML element holds.
    #[error("{field} is too long ({len} bytes)")]
    ValueTooLong {
        /// What: `title`, `muxing app`, `writing app` or the whole `Info`.
        field: &'static str,
        /// Its length in bytes.
        len: usize,
    },
    /// The patched Info doesn't fit where the old one and the padding
    /// around it were.
    #[error(
        "not enough padding around the Info element for the new metadata: \
         need {needed} bytes, have {available}"
    )]
    NoRoom {
        /// Bytes it needs.
        needed: usize,
        /// Bytes there are.
        available: usize,
    },
    /// The Info moved, and the SeekHead's position of it has too few
    /// bytes for where it is now.
    #[error("the SeekHead entry for Info is too narrow to store its new position")]
    SeekPositionTooNarrow,
    /// Larger than [`MAX_INSPECTED_BYTES`], which isn't read.
    #[error("it has more than {} MiB, far more than a sticker", MAX_INSPECTED_BYTES >> 20)]
    TooLarge,
}

/// What reading and patching WebM return.
pub type Result<T> = std::result::Result<T, WebmError>;

/// Longest fake duration [`patch`] accepts, in seconds (a year).
pub const MAX_FAKE_DURATION: f64 = 365.0 * 24.0 * 3600.0;

/// Prefix of the signature text written into padding, which [`inspect`]
/// looks for: [`crate::signature`] must start with it.
pub const SIGNATURE_PREFIX: &str = "tgradish";

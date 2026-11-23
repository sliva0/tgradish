//! WebM container inspection and in-place metadata patching.
//!
//! Patching preserves the exact file length so byte offsets in `SeekHead` and
//! `Cues` stay valid. The Segment `Info` element is rebuilt inside its own
//! span plus any adjacent `Void` padding (ffmpeg leaves padding after
//! `SeekHead`, giving a few dozen bytes of headroom). If `Info` moves within
//! that span, its `SeekHead` entry is updated; nothing after `Info` moves.
//!
//! Duration spoofing uses this mechanism: Telegram rejects video stickers
//! whose Segment `Info` `Duration` exceeds 3 seconds, whereas players play all
//! frames regardless. Track `DURATION` tags written by ffmpeg are patched to
//! match, while [`WebmInfo::content_duration`] reports the true duration from
//! frame timestamps.

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

/// Errors returned when inspecting or patching a WebM container.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WebmError {
    /// Underlying EBML parsing error (no `#[from]` so error chains do not
    /// print it twice).
    #[error("malformed WebM: {0}")]
    Ebml(EbmlError),
    /// File does not begin with an EBML header.
    #[error("not a WebM file: no EBML header")]
    NotEbml,
    /// File contains no Segment element.
    #[error("no Segment element found")]
    NoSegment,
    /// Segment contains no Info element.
    #[error("no Info element found in the Segment")]
    NoInfo,
    /// Spoofed duration is not in `(0, MAX_FAKE_DURATION]`.
    #[error(
        "fake duration must be greater than 0 and at most {MAX_FAKE_DURATION} seconds (got {0})"
    )]
    InvalidDuration(f64),
    /// Segment `TimestampScale` is 0, so no duration can be written.
    #[error("file has an invalid TimestampScale")]
    InvalidTimestampScale,
    /// Field exceeds the maximum EBML element size.
    #[error("{field} is too long ({len} bytes)")]
    ValueTooLong {
        /// Field name (`title`, `muxing app`, `writing app`, or `Info`).
        field: &'static str,
        /// Byte length of the value.
        len: usize,
    },
    /// Not enough adjacent `Void` padding around `Info` to hold the rebuilt
    /// metadata.
    #[error(
        "not enough padding around the Info element for the new metadata (need {needed} bytes, \
         have {available})"
    )]
    NoRoom {
        /// Bytes required.
        needed: usize,
        /// Bytes available.
        available: usize,
    },
    /// `SeekHead`'s `SeekPosition` integer is too narrow to hold the moved
    /// `Info` offset.
    #[error("SeekHead entry for Info is too narrow to store its new position")]
    SeekPositionTooNarrow,
    /// File exceeds [`MAX_INSPECTED_BYTES`].
    #[error("file exceeds {} MiB, far larger than a sticker", MAX_INSPECTED_BYTES >> 20)]
    TooLarge,
}

impl From<EbmlError> for WebmError {
    fn from(err: EbmlError) -> WebmError {
        WebmError::Ebml(err)
    }
}

/// What reading and patching WebM return.
pub type Result<T> = std::result::Result<T, WebmError>;

/// Longest fake duration [`patch`] accepts, in seconds (a year).
pub const MAX_FAKE_DURATION: f64 = 365.0 * 24.0 * 3600.0;

/// Prefix of the signature text written into padding, which [`inspect`]
/// looks for: [`crate::signature`] must start with it.
pub const SIGNATURE_PREFIX: &str = "tgradish";

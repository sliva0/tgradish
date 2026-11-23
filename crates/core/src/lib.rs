//! Shared conversion engine used by both the CLI and the GUI.
//!
//! Converts videos, images, Aseprite files and image sequences into Telegram
//! stickers and custom emoji. Each format module exposes
//! `convert(backend, &Request, &cancel, on_event)`, or callers can dispatch via
//! [`convert::convert`]. All formats report progress through [`Event`] and
//! return [`Outcome`] (from [`events`]):
//!
//! - [`webm`]: VP9 WebM video stickers. Plans transforms from
//!   [`webm::Options`],
//!   fits output under the size limit, and inspects or patches WebM containers
//!   (including duration spoofing) in [`webm::container`].
//! - [`webp`]: Static WebP stickers, sharing picture options with [`webm`].
//! - [`tgs`]: Vector `.tgs` stickers for pixel art, built with
//!   [`tgradish_tgs`] (re-exported here).
//!
//! Supporting modules:
//!
//! - [`input`]: Represents inputs (single files or image sequences), detects
//!   file kinds ([`input::sniff`]), and chooses default output paths
//!   ([`input::Input::default_output`]).
//! - [`picture`]: Framing, scaling and pixel-art detection (so auto scaling
//!   keeps pixel art sharp).
//! - [`backend`] and [`ffmpeg`]: Runs ffmpeg via external `ffmpeg`/`ffprobe`
//!   processes or linked libraries (behind the `linked` feature). [`drawn`]
//!   renders formats ffmpeg cannot read (such as Aseprite and image sequences).
//! - [`options`], [`presets`], [`config`]: Shared option types, option
//!   layering, CLI flag conversion, presets and user config.
//! - [`inspect`], [`telegram`], [`preview`]: Inspects media against Telegram's
//!   limits ([`telegram::Issue`]) and decodes finished stickers for preview.
//! - [`protocol`]: JSON output types for `--json` and `describe`.
//! - [`licenses`], [`paths`], and (with the `clipboard` feature) `clipboard`.
//!
//! Errors use [`Error`], keeping messages free of CLI flag names.

// Keep internal items out of the public API.
#![warn(unreachable_pub)]
#![warn(missing_docs)]
// Show required Cargo features on docs.rs.
#![cfg_attr(docsrs, feature(doc_cfg))]

mod art;
pub mod backend;
mod cancel;
#[cfg(feature = "clipboard")]
#[cfg_attr(docsrs, doc(cfg(feature = "clipboard")))]
pub mod clipboard;
pub mod config;
pub mod convert;
pub mod drawn;
mod error;
pub mod events;
pub mod ffmpeg;
pub mod fsutil;
pub mod input;
pub mod inspect;
pub mod licenses;
#[cfg(feature = "linked")]
mod linked;
#[doc(hidden)]
pub mod mark;
pub mod options;
pub mod paths;
pub mod picture;
pub mod presets;
pub mod preview;
pub mod protocol;
pub mod telegram;
pub mod tgs;
pub mod webm;
pub mod webp;

pub use cancel::CancelToken;
pub use error::{ClipboardError, Error, FfmpegMissing, FileAction, Result, io_reason};
pub use events::{Event, Outcome, Started};
/// Re-export of `tgradish-tgs` (including its `frames` pixel-art reader) so
/// callers can reach every type referenced by this crate's public API.
pub use tgradish_tgs;

/// Tool name and version used in watermarks.
pub const TOOL_ID: &str = concat!("tgradish ", env!("CARGO_PKG_VERSION"));

/// Repository URL embedded in WebM watermark signatures. Keep unchanged so
/// older files remain recognisable.
pub const REPOSITORY: &str = "https://github.com/sliva0/tgradish";

/// `.tgs` sticker names when watermarking is on: `UNTITLED` + [`TOOL_ID`]
/// without a title, or the title + `TITLED`. [`mark`] checks these to
/// recognise tgradish outputs, so keep them unchanged.
pub(crate) const UNTITLED: &str = "made with ";
pub(crate) const TITLED: &str = " (made with tgradish)";

/// Watermark string written into the padding of WebM files made or modified by
/// tgradish (unless watermarks are disabled).
pub fn signature() -> String {
    format!("{TOOL_ID} {REPOSITORY}")
}

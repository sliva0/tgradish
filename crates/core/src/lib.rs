//! Everything tgradish's command line and window share: Telegram stickers
//! and emoji made from videos, pictures, Aseprite files and sequences of
//! images, as any of three kinds of result, each converted by its module's
//! `convert(backend, &Request, &cancel, on_event)`, which reports through
//! its `Event`s and returns its `Outcome`:
//!
//! - [`webm`]: VP9 WebM, planned from its [`webm::Options`] and the input,
//!   fitted to the size limit, and the container inspected and patched,
//!   as duration spoofing does, in [`webm::container`];
//! - [`webp`]: pictures, from WebM's options for a picture;
//! - [`tgs`]: `.tgs` vector animations of pixel art, made by
//!   [`tgradish_tgs`], which this crate re-exports.
//!
//! What they convert is an [`input::Input`], a file or a sequence of
//! pictures, whose result goes where [`input::Input::default_output`]
//! says unless a request names it; [`input::sniff`] tells kinds of file
//! apart. The rest:
//!
//! - [`picture`]: where a picture lands in a WebM or image sticker, and
//!   telling pixel art, which auto scaling keeps sharp;
//! - [`backend`] and [`ffmpeg`]: ffmpeg, which reads videos and encodes
//!   WebM, as the `ffmpeg` and `ffprobe` programs or with the `linked`
//!   feature its libraries, made to do what those programs' command lines
//!   do; [`drawn`] draws what ffmpeg can't read, which tgradish reads
//!   itself, for it;
//! - [`options`]: the values every format's options take, and how options
//!   layer and become flags; [`presets`] and [`config`], the user's;
//! - [`inspect`], what a sticker file holds, judged by [`telegram`]'s
//!   limits; [`preview`], finished stickers decoded for showing;
//! - [`protocol`]: what `--json` and `describe` print, by the rules its
//!   docs give;
//! - [`licenses`], [`paths`], and with the `clipboard` feature
//!   `clipboard`.
//!
//! Errors are an [`Error`], which says what is wrong without naming flags.

// what only core uses says so, which keeps it out of the API
#![warn(unreachable_pub)]

mod art;
pub mod backend;
mod cancel;
#[cfg(feature = "clipboard")]
pub mod clipboard;
pub mod config;
pub mod drawn;
mod error;
pub mod ffmpeg;
mod fsutil;
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
pub use error::{Error, FfmpegMissing, FileAction, Result};
/// `tgradish-tgs`, which makes `.tgs` stickers of pixel art, with the pixel
/// art reader under it as `tgradish_tgs::frames`: every type of theirs
/// this crate's API names is reached through it.
pub use tgradish_tgs;

/// Name and version, used in watermarks.
pub const TOOL_ID: &str = concat!("tgradish ", env!("CARGO_PKG_VERSION"));

/// Where tgradish lives. Files made carry it in their signature, which
/// [`mark`] looks for: changing it stops older files being recognised.
pub const REPOSITORY: &str = "https://github.com/sliva0/tgradish";

/// What a `.tgs` sticker made without a title is named, before
/// [`TOOL_ID`]; with a title, [`TITLED`] follows it. [`mark`] reads these
/// back to know a sticker as tgradish's, so they must stay as they are.
pub(crate) const UNTITLED: &str = "made with ";
pub(crate) const TITLED: &str = " (made with tgradish)";

/// Text hidden in WebM padding to mark files made or modified by tgradish.
pub fn signature() -> String {
    format!("{TOOL_ID} {REPOSITORY}")
}

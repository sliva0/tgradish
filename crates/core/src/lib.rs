//! Everything tgradish's command line and window share: Telegram stickers
//! and emoji as VP9 WebM from videos ([`convert`], fitted to the size
//! limit by [`fit`]), as WebP pictures ([`still`]) and as `.tgs` from pixel
//! art ([`tgs`], over the `tgradish-tgs` crate), and WebM inspection and
//! duration spoofing ([`webm`]). ffmpeg runs through a
//! [`backend::Backend`]: the ffmpeg programs ([`ffmpeg`]), or with the
//! `linked` feature its libraries, used to do what those programs' command
//! lines do. Presets, the user's config and the events front-ends follow
//! are here too.

pub mod backend;
mod cancel;
#[cfg(feature = "clipboard")]
pub mod clipboard;
pub mod config;
pub mod convert;
pub mod ebml;
mod error;
pub mod events;
pub mod ffmpeg;
pub mod fit;
mod fsutil;
pub mod inputs;
pub mod licenses;
#[cfg(feature = "linked")]
mod linked;
pub mod mark;
pub mod options;
pub mod paths;
pub mod presets;
pub mod preview;
pub mod protocol;
pub mod still;
pub mod telegram;
pub mod tgs;
pub mod webm;

pub use cancel::CancelToken;
pub use error::{Error, Result};

/// Name and version, used in watermarks.
pub const TOOL_ID: &str = concat!("tgradish ", env!("CARGO_PKG_VERSION"));

/// Where tgradish lives. Files made carry it in their signature, which
/// [`mark`] looks for: changing it stops older files being recognised.
pub const REPOSITORY: &str = "https://github.com/sliva0/tgradish";

/// Text hidden in WebM padding to mark files made or modified by tgradish.
pub fn signature() -> String {
    format!("{TOOL_ID} {REPOSITORY}")
}

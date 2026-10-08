//! Library behind the `tgradish` CLI: converts videos into Telegram video
//! stickers and patches WebM metadata, including the duration spoofing that
//! gets stickers past the 3 second limit.

pub mod config;
pub mod convert;
pub mod ebml;
mod error;
pub mod events;
pub mod ffmpeg;
pub mod fit;
mod fsutil;
pub mod options;
pub mod paths;
pub mod presets;
pub mod protocol;
pub mod telegram;
pub mod webm;

pub use error::{Error, Result};

/// Name and version, used in watermarks.
pub const TOOL_ID: &str = concat!("tgradish ", env!("CARGO_PKG_VERSION"));

/// Text hidden in WebM padding to mark files made or modified by tgradish.
pub fn signature() -> String {
    format!("{TOOL_ID} https://github.com/sliva0/tgradish")
}

//! Library behind the `tgradish` CLI: converts videos into Telegram video
//! stickers and patches WebM metadata, including the duration spoofing that
//! gets stickers past the 3 second limit.

pub mod ebml;
pub mod webm;

/// Name and version, used in watermarks.
pub const TOOL_ID: &str = concat!("tgradish ", env!("CARGO_PKG_VERSION"));

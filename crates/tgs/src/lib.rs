//! Telegram animated stickers (`.tgs`, gzipped Lottie JSON) from pixel art
//! animations. The plan, with the reasoning behind every rule here, is in
//! `docs/tgs.md`.
//!
//! Pure Rust that builds for `wasm32-unknown-unknown`: no ffmpeg,
//! processes, filesystem or `tgradish-core`. Threads will only come behind
//! a feature.

pub mod limits;

pub use tgradish_frames as frames;

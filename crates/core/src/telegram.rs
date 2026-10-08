//! Telegram requirements for video stickers and emoji.
//!
//! Source: <https://core.telegram.org/stickers>, checked 2026-10-08.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::webm::WebmInfo;

/// Largest file Telegram accepts, in bytes.
pub const MAX_BYTES: u64 = 256 * 1024;
/// Longest allowed duration in seconds, as read from the file header.
pub const MAX_SECONDS: f64 = 3.0;
pub const MAX_FPS: f64 = 30.0;

/// What kind of Telegram media to make.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// Video sticker: one side exactly 512 px, the other at most 512 px.
    #[default]
    Sticker,
    /// Custom emoji: exactly 100x100 px.
    Emoji,
}

impl Target {
    /// Size of the box the video has to fit, in pixels.
    pub fn box_size(self) -> (u32, u32) {
        match self {
            Target::Sticker => (512, 512),
            Target::Emoji => (100, 100),
        }
    }

    /// Whether the video must fill the whole box instead of touching it with
    /// one side.
    pub fn requires_exact_size(self) -> bool {
        matches!(self, Target::Emoji)
    }

    pub fn name(self) -> &'static str {
        match self {
            Target::Sticker => "sticker",
            Target::Emoji => "emoji",
        }
    }
}

/// Something about a file that Telegram will reject.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "issue", rename_all = "snake_case")]
pub enum Issue {
    NotWebm { doc_type: String },
    NoVideo,
    NotVp9 { codec_id: String },
    HasAudio,
    WrongSize { width: u64, height: u64, expected: String },
    TooBig { bytes: u64, max: u64 },
    TooLong { seconds: f64, max: f64 },
    TooManyFps { fps: f64, max: f64 },
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Issue::NotWebm { doc_type } => write!(f, "container is {doc_type:?}, not WebM"),
            Issue::NoVideo => write!(f, "no video track"),
            Issue::NotVp9 { codec_id } => write!(f, "codec is {codec_id}, not VP9"),
            Issue::HasAudio => write!(f, "has an audio track"),
            Issue::WrongSize { width, height, expected } => {
                write!(f, "size is {width}x{height}, expected {expected}")
            }
            Issue::TooBig { bytes, max } => write!(f, "file is {bytes} bytes, limit is {max}"),
            Issue::TooLong { seconds, max } => {
                write!(f, "header duration is {seconds:.3} s, limit is {max} s")
            }
            Issue::TooManyFps { fps, max } => {
                write!(f, "frame rate is {fps:.2} fps, limit is {max}")
            }
        }
    }
}

/// Lists everything that would make Telegram reject `info` as `target`.
///
/// Duration is checked against the header, which is what Telegram reads, so
/// a spoofed file passes even if its frames last longer.
pub fn check(info: &WebmInfo, target: Target) -> Vec<Issue> {
    let mut issues = Vec::new();
    if info.doc_type != "webm" {
        issues.push(Issue::NotWebm { doc_type: info.doc_type.clone() });
    }
    match &info.video {
        None => issues.push(Issue::NoVideo),
        Some(video) => {
            if video.codec_id != "V_VP9" {
                issues.push(Issue::NotVp9 { codec_id: video.codec_id.clone() });
            }
            let (box_w, box_h) = target.box_size();
            let (box_w, box_h) = (u64::from(box_w), u64::from(box_h));
            let size_ok = if target.requires_exact_size() {
                video.width == box_w && video.height == box_h
            } else {
                video.width <= box_w
                    && video.height <= box_h
                    && (video.width == box_w || video.height == box_h)
            };
            if !size_ok {
                let expected = if target.requires_exact_size() {
                    format!("{box_w}x{box_h}")
                } else {
                    format!("one side {box_w} px, the other at most {box_w} px")
                };
                issues.push(Issue::WrongSize {
                    width: video.width,
                    height: video.height,
                    expected,
                });
            }
        }
    }
    if info.audio_tracks > 0 {
        issues.push(Issue::HasAudio);
    }
    if info.file_size > MAX_BYTES {
        issues.push(Issue::TooBig { bytes: info.file_size, max: MAX_BYTES });
    }
    if let Some(seconds) = info.header_duration
        && seconds > MAX_SECONDS + 1e-3
    {
        issues.push(Issue::TooLong { seconds, max: MAX_SECONDS });
    }
    if let Some(fps) = info.fps()
        && fps > MAX_FPS + 0.01
    {
        issues.push(Issue::TooManyFps { fps, max: MAX_FPS });
    }
    issues
}

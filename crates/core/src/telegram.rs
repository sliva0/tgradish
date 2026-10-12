//! Telegram requirements for video and image stickers and emoji.
//!
//! Source: <https://core.telegram.org/stickers>, checked 2026-10-08, and
//! uploads to @Stickers, which found the exact limits.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::webm::WebmInfo;

/// Largest video sticker @Stickers accepts, in bytes; one more is "too big".
pub const MAX_STICKER_BYTES: u64 = 256 * 1024;
/// Largest video emoji @Stickers accepts, in bytes; one more is "too big".
pub const MAX_EMOJI_BYTES: u64 = 64 * 1024;
/// Largest image sticker or emoji, PNG or WebP, in bytes.
pub const MAX_STILL_BYTES: u64 = 512 * 1024;
/// Longest allowed duration in seconds, as read from the file header.
pub const MAX_SECONDS: f64 = 3.0;

/// Whether `seconds` is longer than [`MAX_SECONDS`]. A millisecond over
/// still passes, so that a result cut to exactly 3 seconds isn't taken
/// for too long over float error or WebM's millisecond times.
pub fn too_long(seconds: f64) -> bool {
    seconds > MAX_SECONDS + 1e-3
}
/// The rules say 30 fps, but @Stickers accepted a 60 fps sticker.
pub const MAX_FPS: f64 = 60.0;

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

    /// Largest file Telegram accepts, in bytes.
    pub fn max_bytes(self) -> u64 {
        match self {
            Target::Sticker => MAX_STICKER_BYTES,
            Target::Emoji => MAX_EMOJI_BYTES,
        }
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
    NotWebm {
        doc_type: String,
    },
    NoVideo,
    NotVp9 {
        codec_id: String,
    },
    HasAudio,
    WrongSize {
        width: u64,
        height: u64,
        expected: String,
    },
    TooBig {
        bytes: u64,
        max: u64,
    },
    TooLong {
        seconds: f64,
        max: f64,
    },
    TooManyFps {
        fps: f64,
        max: f64,
    },
    /// An animated image, where image stickers are one picture.
    Animated,
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
            Issue::Animated => write!(f, "it is animated; image stickers are one picture"),
        }
    }
}

/// The issue with a picture `width` by `height` pixels as `target`, if its
/// size isn't one Telegram takes.
fn size_issue(width: u64, height: u64, target: Target) -> Option<Issue> {
    let (box_w, box_h) = target.box_size();
    let (box_w, box_h) = (u64::from(box_w), u64::from(box_h));
    let size_ok = if target.requires_exact_size() {
        width == box_w && height == box_h
    } else {
        width <= box_w && height <= box_h && (width == box_w || height == box_h)
    };
    let expected = if target.requires_exact_size() {
        format!("{box_w}x{box_h}")
    } else {
        format!("one side {box_w} px, the other at most {box_w} px")
    };
    (!size_ok).then_some(Issue::WrongSize { width, height, expected })
}

/// Lists everything that would make Telegram reject an image sticker or
/// emoji of `bytes` bytes, `width` by `height` pixels, as `target`.
pub fn check_still(width: u32, height: u32, bytes: u64, target: Target) -> Vec<Issue> {
    let mut issues: Vec<Issue> =
        size_issue(u64::from(width), u64::from(height), target).into_iter().collect();
    if bytes > MAX_STILL_BYTES {
        issues.push(Issue::TooBig { bytes, max: MAX_STILL_BYTES });
    }
    issues
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
            issues.extend(size_issue(video.width, video.height, target));
        }
    }
    if info.audio_tracks > 0 {
        issues.push(Issue::HasAudio);
    }
    if info.file_size > target.max_bytes() {
        issues.push(Issue::TooBig { bytes: info.file_size, max: target.max_bytes() });
    }
    if let Some(seconds) = info.header_duration
        && too_long(seconds)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webm::VideoTrack;

    fn info(size: u64, file_size: u64, fps: f64) -> WebmInfo {
        WebmInfo {
            doc_type: "webm".into(),
            file_size,
            timestamp_scale_ns: 1_000_000,
            header_duration: Some(3.0),
            content_duration: Some(3.0),
            title: None,
            muxing_app: None,
            writing_app: None,
            duration_tags: Vec::new(),
            signature: None,
            video: Some(VideoTrack {
                number: 1,
                codec_id: "V_VP9".into(),
                width: size,
                height: size,
                alpha: true,
                default_duration_ns: Some((1e9 / fps) as u64),
                uid: None,
            }),
            video_frames: (3.0 * fps) as u64,
            audio_tracks: 0,
            other_tracks: 0,
            truncated: false,
        }
    }

    #[test]
    fn emoji_have_a_smaller_size_limit() {
        assert!(check(&info(512, MAX_STICKER_BYTES, 30.0), Target::Sticker).is_empty());
        assert_eq!(
            check(&info(512, MAX_STICKER_BYTES + 1, 30.0), Target::Sticker),
            [Issue::TooBig { bytes: MAX_STICKER_BYTES + 1, max: MAX_STICKER_BYTES }]
        );
        assert!(check(&info(100, MAX_EMOJI_BYTES, 30.0), Target::Emoji).is_empty());
        assert_eq!(
            check(&info(100, MAX_EMOJI_BYTES + 1, 30.0), Target::Emoji),
            [Issue::TooBig { bytes: MAX_EMOJI_BYTES + 1, max: MAX_EMOJI_BYTES }]
        );
    }

    #[test]
    fn allows_60_fps() {
        assert!(check(&info(512, 1000, 60.0), Target::Sticker).is_empty());
        assert!(matches!(
            check(&info(512, 1000, 75.0), Target::Sticker)[..],
            [Issue::TooManyFps { .. }]
        ));
    }
}

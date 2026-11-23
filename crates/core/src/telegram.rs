//! Telegram requirements for video and static stickers and emoji.
//!
//! Source: <https://core.telegram.org/stickers>, checked 2026-10-08, and
//! uploads to @Stickers, which found the exact limits.

use serde::{Deserialize, Serialize};

use crate::webm::container::WebmInfo;
pub use tgradish_tgs::check::Severity;
use tgradish_tgs::check::{self, IssueKind, Stats};
use tgradish_tgs::limits::telegram as tgs_limits;

/// Largest video sticker @Stickers accepts, in bytes; one more is "too big".
pub const MAX_STICKER_BYTES: u64 = 256 * 1024;
/// Largest video emoji @Stickers accepts, in bytes; one more is "too big".
pub const MAX_EMOJI_BYTES: u64 = 64 * 1024;
/// Largest static sticker or emoji, PNG or WebP, in bytes.
pub const MAX_STILL_BYTES: u64 = 512 * 1024;
/// Longest allowed duration in seconds, as read from the file header.
pub const MAX_SECONDS: f64 = 3.0;
/// The rules say 30 fps, but @Stickers accepted a 60 fps sticker.
pub const MAX_FPS: f64 = 60.0;

/// Returns true if `seconds` exceeds [`MAX_SECONDS`] (with a 1 ms tolerance
/// for float rounding and WebM millisecond timestamps).
pub fn too_long(seconds: f64) -> bool {
    seconds > MAX_SECONDS + 1e-3
}

/// Target Telegram media type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// Sticker: one side exactly 512 px, the other at most 512 px.
    #[default]
    Sticker,
    /// Custom emoji: exactly 100x100 px.
    Emoji,
}

impl Target {
    /// Target bounding box dimensions in pixels.
    pub fn box_size(self) -> (u32, u32) {
        match self {
            Target::Sticker => (512, 512),
            Target::Emoji => (100, 100),
        }
    }

    /// Returns true if output must fill the exact box (emoji) rather than
    /// merely matching one side (stickers).
    pub fn requires_exact_size(self) -> bool {
        matches!(self, Target::Emoji)
    }

    /// Maximum WebM file size Telegram accepts for this target, in bytes.
    pub fn max_bytes(self) -> u64 {
        match self {
            Target::Sticker => MAX_STICKER_BYTES,
            Target::Emoji => MAX_EMOJI_BYTES,
        }
    }

    /// Canonical name used by `--target` and JSON output.
    pub fn name(self) -> &'static str {
        match self {
            Target::Sticker => "sticker",
            Target::Emoji => "emoji",
        }
    }
}

/// Telegram validation issue shared across WebM, WebP and `.tgs` files
/// (with `.tgs`-specific checks wrapped in [`Issue::Lottie`]).
///
/// Serialised with `kind`, `severity`, `message` and variant-specific fields,
/// e.g. `{"kind":"too-big","bytes":300000,"max":262144,"severity":"error",
/// "message":"293.0 KiB exceeds the 256 KiB limit"}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(into = "Reported")]
#[non_exhaustive]
pub enum Issue {
    /// Container DocType is not `"webm"`.
    NotWebm {
        /// EBML DocType string found in the header.
        doc_type: String,
    },
    /// File contains no video track.
    NoVideo,
    /// Video track is not VP9.
    NotVp9 {
        /// Matroska CodecID (for example `"V_VP8"`).
        codec_id: String,
    },
    /// File contains an audio track.
    HasAudio,
    /// Dimensions do not satisfy Telegram's size rules (`exact` means both
    /// sides must equal `side`; otherwise one side must equal `side` and the
    /// other must be at most `side`).
    WrongSize {
        /// Width in pixels.
        width: u64,
        /// Height in pixels.
        height: u64,
        /// Required side length in pixels.
        side: u64,
        /// True for exact square dimensions (emoji and `.tgs`).
        exact: bool,
    },
    /// File size exceeds Telegram's limit (`max` bytes).
    TooBig {
        /// Actual file size in bytes.
        bytes: u64,
        /// Telegram size limit in bytes.
        max: u64,
    },
    /// Header duration exceeds Telegram's limit (`max` seconds).
    TooLong {
        /// Reported duration in seconds.
        seconds: f64,
        /// Maximum allowed duration in seconds.
        max: f64,
    },
    /// Frame rate exceeds Telegram's limit (`max` fps).
    TooManyFps {
        /// Actual frame rate.
        fps: f64,
        /// Maximum allowed frame rate.
        max: f64,
    },
    /// Animated image passed where a static single-frame sticker is expected.
    Animated,
    /// `.tgs`-specific Lottie or renderer validation issue, tagged by its own
    /// `kind` and treated as a warning only when its `severity` says so.
    Lottie(check::Issue),
}

impl Issue {
    /// Severity of this issue (`Severity::Error` for all non-Lottie issues).
    pub fn severity(&self) -> Severity {
        match self {
            Issue::Lottie(issue) => issue.severity,
            _ => Severity::Error,
        }
    }
}

/// Returns true if any issue in `issues` has [`Severity::Error`], meaning
/// Telegram will reject the file (and the CLI exits with status 3).
pub fn refuses(issues: &[Issue]) -> bool {
    issues.iter().any(|issue| issue.severity() == Severity::Error)
}

/// An [`Issue`] as it is serialised.
#[derive(Serialize)]
struct Reported {
    #[serde(flatten)]
    found: Found,
    severity: Severity,
    message: String,
}

/// What an [`Issue`] found, by its kind: a kind every format has, or one
/// only a `.tgs` is checked for.
#[derive(Serialize)]
#[serde(untagged)]
enum Found {
    /// A kind every format has.
    Shared(Shared),
    /// What a `.tgs` alone is checked for.
    Lottie {
        /// What was checked.
        kind: IssueKind,
    },
}

/// What an [`Issue`] of a kind every format has found, tagged by its kind:
/// a copy of it, as serde can't put fields that are worked out beside an
/// enum's own.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
enum Shared {
    /// The container isn't WebM.
    NotWebm { doc_type: String },
    /// There is no video track.
    NoVideo,
    /// The video isn't VP9.
    NotVp9 { codec_id: String },
    /// There is an audio track.
    HasAudio,
    /// Not a size Telegram takes: one side `side` pixels long and the
    /// other at most that, or with `exact`, both that long.
    WrongSize { width: u64, height: u64, side: u64, exact: bool },
    /// Larger than `max` bytes.
    TooBig { bytes: u64, max: u64 },
    /// Its header says it lasts longer than `max` seconds.
    TooLong { seconds: f64, max: f64 },
    /// More than `max` frames a second.
    TooManyFps { fps: f64, max: f64 },
    /// An animated image, where static stickers are one picture.
    Animated,
}

impl From<Issue> for Reported {
    fn from(issue: Issue) -> Reported {
        let (severity, message) = (issue.severity(), issue.to_string());
        let shared = match issue {
            Issue::NotWebm { doc_type } => Shared::NotWebm { doc_type },
            Issue::NoVideo => Shared::NoVideo,
            Issue::NotVp9 { codec_id } => Shared::NotVp9 { codec_id },
            Issue::HasAudio => Shared::HasAudio,
            Issue::WrongSize { width, height, side, exact } => {
                Shared::WrongSize { width, height, side, exact }
            }
            Issue::TooBig { bytes, max } => Shared::TooBig { bytes, max },
            Issue::TooLong { seconds, max } => Shared::TooLong { seconds, max },
            Issue::TooManyFps { fps, max } => Shared::TooManyFps { fps, max },
            Issue::Animated => Shared::Animated,
            Issue::Lottie(issue) => {
                return Reported { found: Found::Lottie { kind: issue.kind }, severity, message };
            }
        };
        Reported { found: Found::Shared(shared), severity, message }
    }
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Issue::NotWebm { doc_type } => write!(f, "container is {doc_type:?}, not WebM"),
            Issue::NoVideo => write!(f, "no video track"),
            Issue::NotVp9 { codec_id } => write!(f, "codec is {codec_id}, not VP9"),
            Issue::HasAudio => write!(f, "contains an audio track"),
            Issue::WrongSize { width, height, side, exact: true } => {
                write!(f, "size is {width}×{height}, expected {side}×{side}")
            }
            Issue::WrongSize { width, height, side, exact: false } => write!(
                f,
                "size is {width}×{height}, expected one side to be {side} px and the other at most \
                 {side} px"
            ),
            Issue::TooBig { bytes, max } => {
                let kib = |bytes: u64| bytes as f64 / 1024.0;
                write!(f, "{:.1} KiB exceeds the {} KiB limit", kib(*bytes), kib(*max))
            }
            Issue::TooLong { seconds, max } => {
                write!(f, "metadata duration is {seconds:.3} s (limit is {max} s)")
            }
            Issue::TooManyFps { fps, max } => {
                write!(f, "frame rate is {fps:.2} fps (limit is {max})")
            }
            Issue::Animated => {
                write!(f, "image is animated; static stickers must be a single frame")
            }
            Issue::Lottie(issue) => write!(f, "{}", issue.message),
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
    let exact = target.requires_exact_size();
    (!size_ok).then_some(Issue::WrongSize { width, height, side: box_w, exact })
}

/// Lists everything that would make Telegram reject a static sticker or
/// emoji of `bytes` bytes, `width` by `height` pixels, as `target`.
pub(crate) fn check_still(width: u32, height: u32, bytes: u64, target: Target) -> Vec<Issue> {
    let mut issues: Vec<Issue> =
        size_issue(u64::from(width), u64::from(height), target).into_iter().collect();
    if bytes > MAX_STILL_BYTES {
        issues.push(Issue::TooBig { bytes, max: MAX_STILL_BYTES });
    }
    issues
}

/// Lists what Telegram would refuse of a sticker that kept the size,
/// length and frame rate of the input `info` tells of, as `target`.
pub fn check_input(info: &crate::inspect::InputInfo, target: Target) -> Vec<Issue> {
    let mut issues: Vec<Issue> =
        size_issue(u64::from(info.width), u64::from(info.height), target).into_iter().collect();
    if let Some(seconds) = info.duration
        && too_long(seconds)
    {
        issues.push(Issue::TooLong { seconds, max: MAX_SECONDS });
    }
    if let Some(fps) = info.fps
        && fps > MAX_FPS + 0.01
    {
        issues.push(Issue::TooManyFps { fps, max: MAX_FPS });
    }
    issues
}

/// Converts `tgradish_tgs::check` issues into unified [`Issue`] values,
/// mapping canvas size, file size, duration and high-fps errors to shared
/// variants and keeping `.tgs`-specific checks under [`Issue::Lottie`].
pub fn check_lottie(found: Vec<check::Issue>, stats: &Stats) -> Vec<Issue> {
    // a canvas a whole number of pixels on each side
    let whole = |value: f64| (value >= 0.0 && value.fract() == 0.0).then_some(value as u64);
    let canvas = u64::from(tgs_limits::CANVAS);
    let fps = f64::from(tgs_limits::FPS);
    found
        .into_iter()
        .map(|found| match found.kind {
            _ if found.severity != Severity::Error => Issue::Lottie(found),
            IssueKind::Canvas => match (whole(stats.width), whole(stats.height)) {
                (Some(width), Some(height)) => {
                    Issue::WrongSize { width, height, side: canvas, exact: true }
                }
                _ => Issue::Lottie(found),
            },
            IssueKind::FileSize => match stats.tgs_bytes {
                Some(bytes) => {
                    Issue::TooBig { bytes: bytes as u64, max: tgs_limits::MAX_BYTES as u64 }
                }
                None => Issue::Lottie(found),
            },
            IssueKind::Length if stats.fps > 0.0 => Issue::TooLong {
                seconds: stats.frames / stats.fps,
                max: f64::from(tgs_limits::MAX_FRAMES) / fps,
            },
            // one under it is no other format's issue
            IssueKind::FrameRate if stats.fps > fps => {
                Issue::TooManyFps { fps: stats.fps, max: fps }
            }
            _ => Issue::Lottie(found),
        })
        .collect()
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
        // rates read back from a file's timestamps round a little over
        && fps > MAX_FPS + 0.01
    {
        issues.push(Issue::TooManyFps { fps, max: MAX_FPS });
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webm::container::VideoTrack;

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

    #[test]
    fn reports_issues_of_every_format_alike() {
        let issue = serde_json::to_value(Issue::TooBig { bytes: 300_000, max: 262_144 }).unwrap();
        let message = "293.0 KiB exceeds the 256 KiB limit";
        let expected = serde_json::json!({
            "kind": "too-big", "severity": "error", "message": message,
            "bytes": 300_000, "max": 262_144,
        });
        assert_eq!(issue, expected);
        let size = Issue::WrongSize { width: 512, height: 384, side: 100, exact: true };
        let size = serde_json::to_value(size).unwrap();
        assert_eq!(
            (&size["side"], &size["exact"]),
            (&serde_json::json!(100), &serde_json::json!(true))
        );
        // what only a .tgs is checked for has the same envelope, with no
        // data of its own
        let lottie = br#"{"v":"5.5.2","w":100,"h":100,"fr":60,"ip":0,"op":60,"layers":[]}"#;
        let (_, found) = tgradish_tgs::check::check(lottie, None).unwrap();
        let tgs = serde_json::to_value(Issue::Lottie(found[0].clone())).unwrap();
        assert_eq!(tgs, serde_json::to_value(&found[0]).unwrap());
        let keys = |value: &serde_json::Value| -> Vec<String> {
            let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };
        assert_eq!(keys(&tgs), ["kind", "message", "severity"]);
        assert!(keys(&tgs).iter().all(|key| issue.get(key).is_some()));
    }

    #[test]
    fn tells_a_tgs_by_the_kinds_every_format_has() {
        let lottie =
            br#"{"fr":25,"ip":0,"op":200,"w":512,"h":256,"layers":[{"ty":4,"shapes":[]}]}"#;
        let (stats, found) = tgradish_tgs::check::check(lottie, Some(70_000)).unwrap();
        let issues = check_lottie(found, &stats);
        assert_eq!(issues[0], Issue::WrongSize { width: 512, height: 256, side: 512, exact: true });
        // fewer frames a second than stickers have is a .tgs's own
        assert!(matches!(&issues[1], Issue::Lottie(found) if found.kind == IssueKind::FrameRate));
        assert_eq!(issues[2], Issue::TooLong { seconds: 8.0, max: 3.0 });
        assert_eq!(issues[3], Issue::TooBig { bytes: 70_000, max: 65_536 });
        assert_eq!(issues.len(), 4, "{issues:?}");
        let printed = serde_json::to_value(&issues[3]).unwrap();
        assert_eq!((&printed["kind"], &printed["max"]), (&"too-big".into(), &65_536.into()));
        // more than a sticker plays
        let fast = br#"{"fr":120,"ip":0,"op":120,"w":512,"h":512,"layers":[{"ty":4,"shapes":[]}]}"#;
        let (stats, found) = tgradish_tgs::check::check(fast, None).unwrap();
        assert_eq!(check_lottie(found, &stats), [Issue::TooManyFps { fps: 120.0, max: 60.0 }]);
    }

    #[test]
    fn refuses_for_errors_only() {
        let thirty = br#"{"fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[{"ty":4,"shapes":[]}]}"#;
        let (_, found) = tgradish_tgs::check::check(thirty, None).unwrap();
        let frowned: Vec<Issue> = found.into_iter().map(Issue::Lottie).collect();
        assert_eq!(frowned.iter().map(Issue::severity).collect::<Vec<_>>(), [Severity::Warning]);
        assert!(!refuses(&frowned) && !refuses(&[]));
        let too_big = Issue::TooBig { bytes: 2, max: 1 };
        assert!(refuses(&[frowned[0].clone(), too_big]));
    }
}

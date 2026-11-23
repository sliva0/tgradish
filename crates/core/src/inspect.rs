//! Inspects sticker files and source media against Telegram's limits.
//! Used by `tgradish inspect` and the GUI's Inspect view.

use std::path::Path;

use serde::Serialize;

use tgradish_tgs::check;

use crate::CancelToken;
use crate::backend::Backend;
use crate::error::Result;
use crate::input::{FileKind, sniff_readable};
use crate::telegram::{self, Target};
use crate::webm::container::{self, WebmInfo};
use crate::{tgs, webp};

/// Inspection result tagged by `format`, containing parsed metadata (`info`)
/// and Telegram validation issues (`issues`).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "format", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Inspected {
    /// Static or animated WebP file checked against `target`.
    Webp {
        /// Target limits applied.
        target: Target,
        /// Parsed WebP properties.
        info: webp::Info,
        /// Telegram validation issues for `target`.
        issues: Vec<telegram::Issue>,
    },
    /// `.tgs` file (stickers and emoji share the same `.tgs` limits).
    Tgs {
        /// Parsed Lottie statistics.
        info: Box<check::Stats>,
        /// Telegram validation and renderer compatibility issues.
        issues: Vec<telegram::Issue>,
    },
    /// WebM file checked against `target`.
    Webm {
        /// Target limits applied.
        target: Target,
        /// Parsed WebM container properties.
        info: Box<WebmInfo>,
        /// Telegram validation issues for `target`.
        issues: Vec<telegram::Issue>,
    },
    /// Source media file (not a sticker yet), checked against `target` as if
    /// converted without changing dimensions, duration or frame rate.
    Input {
        /// Target limits applied.
        target: Target,
        /// Parsed source media properties.
        info: Box<InputInfo>,
        /// What Telegram would reject in a sticker that kept the input's
        /// dimensions, duration and frame rate.
        issues: Vec<telegram::Issue>,
    },
}

/// Metadata collected for a non-sticker source file in [`Inspected::Input`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct InputInfo {
    /// Media category.
    pub kind: InputKind,
    /// File size in bytes.
    pub bytes: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frame count (exact for pixel-art formats; estimated from duration * fps
    /// for videos).
    pub frames: Option<u64>,
    /// Playback duration in seconds (`None` for still images).
    pub duration: Option<f64>,
    /// Average frame rate (`None` for still images).
    pub fps: Option<f64>,
    /// Whether the file can be transparent.
    pub alpha: bool,
    /// Codec name from `ffprobe` for files read via ffmpeg.
    pub codec: Option<String>,
}

/// Source media categories reported in [`InputInfo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum InputKind {
    /// Still image (PNG, JPEG, BMP, or single-frame GIF).
    Picture,
    /// Animated GIF or APNG.
    Animation,
    /// Aseprite file.
    Aseprite,
    /// Video or other format read via ffmpeg.
    Video,
}

impl Inspected {
    /// Returns true if Telegram would reject this file (always true for
    /// [`Inspected::Input`], or whenever [`telegram::refuses`] returns true).
    pub fn refused(&self) -> bool {
        match self {
            Inspected::Webp { issues, .. }
            | Inspected::Webm { issues, .. }
            | Inspected::Tgs { issues, .. } => telegram::refuses(issues),
            // Not a sticker format yet.
            Inspected::Input { .. } => true,
        }
    }
}

/// Inspects a WebM, WebP or `.tgs` sticker file against `target` (inferring
/// sticker vs emoji from dimensions when `target` is `None`). Returns
/// [`Error::NotSticker`](crate::Error::NotSticker) for other formats.
pub fn inspect(path: &Path, target: Option<Target>) -> Result<Inspected> {
    Ok(match sniff_readable(path)? {
        FileKind::Webp { .. } => {
            let info = webp::inspect_file(path)?;
            let target = target.unwrap_or_else(|| info.target());
            let issues = info.issues(target);
            Inspected::Webp { target, info, issues }
        }
        FileKind::Tgs => {
            let (info, issues) = tgs::inspect_file(path)?;
            Inspected::Tgs { info: Box::new(info), issues }
        }
        FileKind::Webm => {
            let info = container::inspect_file(path)?;
            let target = target.unwrap_or_else(|| info.target());
            let issues = telegram::check(&info, target);
            Inspected::Webm { target, info: Box::new(info), issues }
        }
        _ => return Err(crate::Error::NotSticker(path.to_path_buf())),
    })
}

/// Inspects a sticker file via [`inspect`], or inspects source media against
/// `target` (defaulting to [`Target::Sticker`]). Images, animations and
/// Aseprite files are decoded directly; `backend` is only called for videos
/// and other ffmpeg-only formats.
pub fn inspect_any(
    path: &Path,
    target: Option<Target>,
    backend: impl FnOnce() -> Result<Backend>,
    cancel: &CancelToken,
) -> Result<Inspected> {
    use tgradish_tgs::frames::{self, Limits};
    let kind = sniff_readable(path)?;
    if matches!(kind, FileKind::Webp { .. } | FileKind::Tgs | FileKind::Webm) {
        return inspect(path, target);
    }
    let bytes = std::fs::metadata(path).map_or(0, |meta| meta.len());
    let info = match kind {
        FileKind::Aseprite | FileKind::Picture => {
            let read = crate::art::read(path, crate::art::most_bytes(&Limits::default()))?;
            let stop = || cancel.is_cancelled();
            let options = frames::DecodeOptions { cancelled: Some(&stop), ..Default::default() };
            let animation = match kind {
                FileKind::Aseprite => crate::art::aseprite(&read, &options, &Limits::default())
                    .map(|(animation, _)| animation),
                _ => frames::decode(&read, &options, &Limits::default()),
            }
            .map_err(|err| crate::art::undecodable(path, err))?;
            let count = animation.frames().len();
            let length: f64 =
                animation.frames().iter().map(|frame| frame.duration.as_secs_f64()).sum();
            let still = count == 1 && kind == FileKind::Picture;
            let alpha = animation
                .frames()
                .iter()
                .any(|frame| frame.rgba.iter().skip(3).step_by(4).any(|&alpha| alpha < 255));
            InputInfo {
                kind: match kind {
                    FileKind::Aseprite => InputKind::Aseprite,
                    _ if still => InputKind::Picture,
                    _ => InputKind::Animation,
                },
                bytes,
                width: animation.width(),
                height: animation.height(),
                frames: Some(count as u64),
                duration: (!still).then_some(length),
                fps: (!still && length > 0.0).then(|| count as f64 / length),
                alpha,
                codec: None,
            }
        }
        _ => {
            let probe = backend()?.probe(path, cancel)?;
            let frames = probe
                .duration
                .zip(probe.fps)
                .map(|(duration, fps)| (duration * fps).round() as u64)
                .or(probe.still_image.then_some(1));
            InputInfo {
                kind: if probe.still_image { InputKind::Picture } else { InputKind::Video },
                bytes,
                width: probe.width,
                height: probe.height,
                frames,
                duration: probe.duration,
                fps: probe.fps,
                alpha: probe.alpha,
                codec: Some(probe.codec),
            }
        }
    };
    let target = target.unwrap_or(Target::Sticker);
    let issues = telegram::check_input(&info, target);
    Ok(Inspected::Input { target, info: Box::new(info), issues })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_stickers_are_made_of() {
        let dir = tempfile::tempdir().unwrap();
        // a GIF of three frames, 100 ms each, 64 px square
        let walker = tgradish_corpus::files();
        let gif = walker.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let path = dir.path().join("walker.gif");
        std::fs::write(&path, &gif.bytes).unwrap();
        let no_ffmpeg = || -> Result<Backend> { panic!("ffmpeg isn't needed for a GIF") };
        let inspected = inspect_any(&path, None, no_ffmpeg, &CancelToken::new()).unwrap();
        let Inspected::Input { target, info, issues } = &inspected else { panic!("{inspected:?}") };
        assert_eq!((*target, info.kind), (Target::Sticker, InputKind::Animation));
        assert!(info.frames.is_some_and(|frames| frames > 1), "{info:?}");
        assert!(info.duration.is_some() && info.fps.is_some(), "{info:?}");
        // not 512 px on a side, and no sticker yet whatever it holds
        assert!(issues.iter().any(|issue| matches!(issue, telegram::Issue::WrongSize { .. })));
        assert!(inspected.refused());
        // a sticker is inspected as one
        let not_called = || -> Result<Backend> { panic!("not asked for") };
        let tgs = dir.path().join("a.tgs");
        std::fs::write(&tgs, b"{").unwrap();
        assert!(inspect_any(&tgs, None, not_called, &CancelToken::new()).is_err());
    }
}

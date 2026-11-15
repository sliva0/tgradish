//! What a sticker file is and holds, whichever of the three kinds it is,
//! and what Telegram would say of it: what the command line's `inspect`
//! prints and the GUI's Inspect shows. The command line inspects what
//! stickers are made of too, as [`inspect_any`] reads it.

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

/// What a sticker file holds, as `info`, and what Telegram would refuse
/// of it, as `issues`, tagged by its `format`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "format", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Inspected {
    /// A WebP picture, judged as `target`.
    Webp {
        /// What it was judged as.
        target: Target,
        /// What it is.
        info: webp::Info,
        /// What Telegram would refuse of it as `target`.
        issues: Vec<telegram::Issue>,
    },
    /// A `.tgs`, whose limits are the same for stickers and emoji.
    Tgs {
        /// What its Lottie holds.
        info: Box<check::Stats>,
        /// What breaks Telegram's limits or renderers, by severity.
        issues: Vec<telegram::Issue>,
    },
    /// A WebM video, judged as `target`.
    Webm {
        /// What it was judged as.
        target: Target,
        /// What its container says.
        info: Box<WebmInfo>,
        /// What Telegram would refuse of it as `target`.
        issues: Vec<telegram::Issue>,
    },
    /// What a sticker is made of, which Telegram takes as none: a picture,
    /// an animation, an Aseprite file or a video, judged as `target` by
    /// what a sticker made of it would keep.
    Input {
        /// What it was judged as.
        target: Target,
        /// What it is.
        info: Box<InputInfo>,
        /// What Telegram would refuse of a sticker that kept its size,
        /// length and frame rate.
        issues: Vec<telegram::Issue>,
    },
}

/// What [`Inspected::Input`] found of a file a sticker is made of.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct InputInfo {
    /// What kind of file it is.
    pub kind: InputKind,
    /// Its size in bytes.
    pub bytes: u64,
    /// Its size in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// How many frames it has: counted for what tgradish reads itself, and
    /// for a video worked out from its length and frame rate.
    pub frames: Option<u64>,
    /// How long it plays, in seconds: none for a still picture.
    pub duration: Option<f64>,
    /// Its average frame rate: none for a still picture.
    pub fps: Option<f64>,
    /// Whether it can be transparent.
    pub alpha: bool,
    /// Its codec, as ffprobe names it, for what ffmpeg reads.
    pub codec: Option<String>,
}

/// The kinds of file [`InputInfo`] tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum InputKind {
    /// A still picture: PNG, JPEG, BMP, or a GIF of one frame.
    Picture,
    /// An animated GIF or PNG.
    Animation,
    /// An Aseprite file, all of its frames.
    Aseprite,
    /// Anything ffmpeg reads: videos, and other pictures.
    Video,
}

impl Inspected {
    /// Whether Telegram would refuse the file, as [`telegram::refuses`]
    /// its issues.
    pub fn refused(&self) -> bool {
        match self {
            Inspected::Webp { issues, .. }
            | Inspected::Webm { issues, .. }
            | Inspected::Tgs { issues, .. } => telegram::refuses(issues),
            // no sticker at all yet
            Inspected::Input { .. } => true,
        }
    }
}

/// What the sticker at `path` holds, judged as `target`, or else as what
/// its size says it is. A file of no kind a sticker is, as a GIF, is
/// [`Error::NotSticker`](crate::Error::NotSticker).
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

/// What the file at `path` holds: a sticker as [`inspect`] reads it, or
/// else what a sticker would be made of, judged as `target`, a sticker
/// unless it says. Pictures, animations and Aseprite files are read here;
/// anything else by the ffmpeg `backend` gives, which is asked for only
/// then.
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

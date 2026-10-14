//! Inputs tgradish's own pixel art reader reads, for every kind of
//! result: the frames of one animation in several image files, and
//! Aseprite files, with the tags, layers and frame times they tell of.
//! `.tgs` stickers are made of what it reads, and WebM and image stickers
//! of it drawn for ffmpeg, which reads neither.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tgradish_tgs::frames::{self, Animation, LayerInfo, Layers, Limits, Tag};

use crate::CancelToken;
use crate::error::{Error, Result};
use crate::tgs::DEFAULT_FPS;

/// What an Aseprite file tells of itself besides its frames, which the
/// options choose from.
#[derive(Debug, Clone, Default)]
pub(crate) struct Aseprite {
    pub(crate) tags: Vec<Tag>,
    /// As its Layers panel lists them.
    pub(crate) layers: Vec<LayerInfo>,
    /// How long each frame shows, in seconds, its tags' or not.
    pub(crate) durations: Vec<f64>,
}

/// The frames of an Aseprite file, from its bytes, with `tag` and `layers`
/// chosen, and what it tells of itself; parsed once for all of these.
pub(crate) fn aseprite(
    bytes: &[u8],
    tag: Option<&str>,
    layers: &Layers,
    limits: &Limits,
) -> frames::Result<(Animation, Aseprite)> {
    let sprite = frames::Sprite::read_with(bytes, limits)?;
    let animation = sprite.animation(tag, layers)?;
    let durations = sprite.durations().iter().map(Duration::as_secs_f64).collect();
    Ok((animation, Aseprite { tags: sprite.tags().to_vec(), layers: sprite.layers(), durations }))
}

/// The bytes of the file at `path`, failing as an unreadable input.
pub(crate) fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|err| unreadable(path, err))
}

/// `path` couldn't be read for `err`.
pub(crate) fn unreadable(path: &Path, err: impl std::fmt::Display) -> Error {
    Error::Probe { path: path.to_path_buf(), message: err.to_string() }
}

/// How long each frame of a sequence or sprite sheet shows, at `fps`
/// frames a second, [`DEFAULT_FPS`] unless it says.
pub(crate) fn frame_duration(fps: Option<f64>) -> Result<Duration> {
    let fps = fps.unwrap_or(DEFAULT_FPS);
    let most = f64::from(tgradish_tgs::limits::telegram::FPS);
    let invalid = || Error::InvalidOptions(format!("fps {fps} is not between 0 and {most}"));
    if !(fps > 0.0 && fps <= most) {
        return Err(invalid());
    }
    // tiny rates give durations past what Duration holds
    Duration::try_from_secs_f64(1.0 / fps).map_err(|_| invalid())
}

/// Image files of a sequence, numbers sorted as numbers: `2.png` before
/// `10.png`.
pub(crate) fn sequence_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(input)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<_>>()?;
            found.retain(|path| path.is_file() && crate::inputs::is_frame(path));
            files.extend(found);
        } else {
            files.push(input.clone());
        }
    }
    // the order the user gave is usually a shell's, which puts 10 before 2
    files.sort_by(|a, b| natural_order(a, b));
    if files.is_empty() {
        return Err(Error::InvalidOptions("the sequence has no images".into()));
    }
    Ok(files)
}

/// Compares paths with runs of digits as numbers.
fn natural_order(a: &Path, b: &Path) -> std::cmp::Ordering {
    let pieces = |path: &Path| -> Vec<(String, u128)> {
        let text = path.to_string_lossy();
        let mut out = Vec::new();
        let mut rest = text.as_ref();
        while !rest.is_empty() {
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if digits > 0 {
                out.push((String::new(), rest[..digits].parse().unwrap_or(u128::MAX)));
                rest = &rest[digits..];
            } else {
                let letters = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
                out.push((rest[..letters].to_lowercase(), 0));
                rest = &rest[letters..];
            }
        }
        out
    };
    pieces(a).cmp(&pieces(b)).then_with(|| a.cmp(b))
}

/// The frames of one animation in several files, `inputs`: image files,
/// or folders of them, in name order, `fps` a second. `cancel` is asked
/// between files. A file that doesn't read is named in the error.
pub(crate) fn read_sequence(
    inputs: &[PathBuf],
    fps: Option<f64>,
    cancel: &CancelToken,
) -> Result<Animation> {
    let files = sequence_files(inputs)?;
    let mut bytes = Vec::with_capacity(files.len());
    for file in &files {
        cancel.check()?;
        bytes.push(read(file)?);
    }
    // which file is being decoded, for the error if it doesn't
    let at = std::cell::Cell::new(0);
    // a file can't be stopped part of the way, but the rest are left
    let images =
        bytes.iter().enumerate().take_while(|_| !cancel.is_cancelled()).map(|(index, bytes)| {
            at.set(index);
            bytes.as_slice()
        });
    let animation = frames::sequence(images, frame_duration(fps)?, &Limits::default());
    cancel.check()?;
    animation.map_err(|err| {
        let index = match err {
            frames::Error::SequenceSize { index, .. } => index,
            _ => at.get(),
        };
        unreadable(&files[index], err)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_numbers_naturally() {
        let mut files: Vec<PathBuf> =
            ["f10.png", "f2.png", "f1.png", "F3.png"].map(PathBuf::from).to_vec();
        files.sort_by(|a, b| natural_order(a, b));
        assert_eq!(files, ["f1.png", "f2.png", "F3.png", "f10.png"].map(PathBuf::from));
    }

    #[test]
    fn sorts_listed_files_too() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = ["10.png", "2.png"].map(|name| dir.path().join(name)).to_vec();
        assert_eq!(
            sequence_files(&files).unwrap(),
            [dir.path().join("2.png"), dir.path().join("10.png")]
        );
    }

    #[test]
    fn names_the_file_that_doesnt_read() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = ["1.png", "2.png"].map(|name| dir.path().join(name)).to_vec();
        let walker = tgradish_corpus::sprite("walker_2x");
        std::fs::write(&files[0], tgradish_corpus::encode::png(&walker.frames[0])).unwrap();
        std::fs::write(&files[1], b"not a png").unwrap();
        let err = read_sequence(&files, None, &CancelToken::new()).unwrap_err();
        assert!(matches!(err, Error::Probe { path, .. } if path == files[1]));
    }
}

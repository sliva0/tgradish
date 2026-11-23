//! Reads pixel-art inputs (image sequences and Aseprite files) using
//! `tgradish-tgs`. Used directly for `.tgs` conversion and rendered via
//! [`crate::drawn`] for WebM and WebP, which ffmpeg cannot read directly.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tgradish_tgs::frames::aseprite::{LayerInfo, Sprite, Tag};
use tgradish_tgs::frames::{self, Animation, DecodeOptions, Limits};

use crate::CancelToken;
use crate::error::{Error, Result};
use crate::tgs::DEFAULT_FPS;

/// Metadata read from an Aseprite file alongside its frames.
#[derive(Debug, Clone, Default)]
pub(crate) struct Aseprite {
    pub(crate) tags: Vec<Tag>,
    /// Layers in the order shown in Aseprite's Layers panel.
    pub(crate) layers: Vec<LayerInfo>,
    /// Duration of every frame in the file, in seconds.
    pub(crate) durations: Vec<f64>,
}

/// Parses an Aseprite file once to produce both its rendered [`Animation`]
/// (filtered by `options`) and its [`Aseprite`] metadata.
pub(crate) fn aseprite(
    bytes: &[u8],
    options: &DecodeOptions,
    limits: &Limits,
) -> frames::Result<(Animation, Aseprite)> {
    let sprite = Sprite::read(bytes, limits)?;
    let animation = sprite.animation(options)?;
    let durations = sprite.durations().iter().map(Duration::as_secs_f64).collect();
    Ok((animation, Aseprite { tags: sprite.tags().to_vec(), layers: sprite.layers(), durations }))
}

/// Maximum size of a `.tgs` or WebP file read for inspection or preview.
/// Far above any valid sticker or [`tgradish_tgs::file::unpack`]'s limit.
pub(crate) const MAX_STICKER_BYTES: u64 = 16 << 20;

/// Maximum input file size for pixel-art decoding under `limits`: its
/// frame-memory limit, which a larger file cannot fit in.
pub(crate) fn most_bytes(limits: &Limits) -> u64 {
    limits.max_bytes as u64
}

/// Reads `path` up to `most` bytes (never reading past `most`), returning
/// [`Error::NoSuchFile`] if missing or [`Error::Unreadable`] if the file
/// exceeds `most` or fails to read.
pub(crate) fn read(path: &Path, most: u64) -> Result<Vec<u8>> {
    match crate::fsutil::read_at_most(path, most) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => {
            let size = std::fs::metadata(path).map_or(most + 1, |found| found.len());
            Err(unreadable(path, format!("file is {size} bytes, exceeding the {most}-byte limit")))
        }
        Err(err) => Err(read_failed(path, err)),
    }
}

/// Maps an I/O error on `path` to [`Error::NoSuchFile`] or
/// [`Error::Unreadable`].
pub(crate) fn read_failed(path: &Path, err: std::io::Error) -> Error {
    match err.kind() {
        std::io::ErrorKind::NotFound => Error::NoSuchFile(path.to_path_buf()),
        _ => unreadable(path, crate::io_reason(&err)),
    }
}

/// Builds an [`Error::Unreadable`] for `path`.
pub(crate) fn unreadable(path: &Path, err: impl std::fmt::Display) -> Error {
    Error::Unreadable { path: path.to_path_buf(), message: err.to_string() }
}

/// Converts a `frames::Error` into [`Error::Cancelled`] or
/// [`Error::Unreadable`].
pub(crate) fn undecodable(path: &Path, err: frames::Error) -> Error {
    match err {
        frames::Error::Cancelled => Error::Cancelled,
        err => unreadable(path, err),
    }
}

/// Computes the frame duration for a sequence or sprite sheet from `fps`
/// (defaulting to [`DEFAULT_FPS`]).
pub(crate) fn frame_duration(fps: Option<f64>) -> Result<Duration> {
    let fps = fps.unwrap_or(DEFAULT_FPS);
    let most = f64::from(tgradish_tgs::limits::telegram::FPS);
    let invalid =
        || Error::invalid("fps", format!("{fps} must be greater than 0 and at most {most}"));
    if !(fps > 0.0 && fps <= most) {
        return Err(invalid());
    }
    // Very small frame rates overflow Duration.
    Duration::try_from_secs_f64(1.0 / fps).map_err(|_| invalid())
}

/// Expands directories and returns sequence image files in natural numeric
/// order (so `2.png` comes before `10.png`).
pub(crate) fn sequence_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(input)
                .and_then(|entries| entries.map(|entry| entry.map(|entry| entry.path())).collect())
                .map_err(|err| read_failed(input, err))?;
            found.retain(|path| path.is_file() && crate::input::is_frame(path));
            files.extend(found);
        } else {
            files.push(input.clone());
        }
    }
    // Shell globs sort lexicographically (putting 10 before 2), so re-sort
    // naturally.
    files.sort_by(|a, b| natural_order(a, b));
    if files.is_empty() {
        return Err(Error::NothingToConvert);
    }
    Ok(files)
}

/// Compares two paths naturally, comparing runs of digits by numeric value.
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

/// Reads an image sequence from files or directories in natural order at `fps`,
/// decoding one file at a time so memory stays bounded. Checks `cancel` between
/// files and reports the failing path on error.
pub(crate) fn read_sequence(
    inputs: &[PathBuf],
    fps: Option<f64>,
    limits: &Limits,
    cancel: &CancelToken,
) -> Result<Animation> {
    let files = sequence_files(inputs)?;
    let stop = || cancel.is_cancelled();
    let mut sequence = frames::Sequence::new(frame_duration(fps)?, limits).cancelled_by(&stop);
    for file in &files {
        cancel.check()?;
        let bytes = read(file, most_bytes(limits))?;
        sequence.push(&bytes).map_err(|err| undecodable(file, err))?;
    }
    cancel.check()?;
    sequence.finish().map_err(|err| undecodable(&files[0], err))
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
    fn stops_decoding_when_cancelled() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let stop = || cancel.is_cancelled();
        let ase = tgradish_corpus::files().into_iter().find(|file| file.name == "walker_2x.ase");
        let options = DecodeOptions { cancelled: Some(&stop), ..DecodeOptions::default() };
        let stopped = aseprite(&ase.unwrap().bytes, &options, &Limits::default()).unwrap_err();
        assert!(matches!(undecodable(Path::new("walker.ase"), stopped), Error::Cancelled));
    }

    #[test]
    fn reads_no_more_than_each_use_takes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.tgs");
        // sparse: as large as it says, with nothing written
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_STICKER_BYTES + 1).unwrap();
        let message = |err: Error| match err {
            Error::Unreadable { message, .. } => message,
            err => panic!("{err:?}"),
        };
        let too_large =
            format!("file is {} bytes, exceeding the {MAX_STICKER_BYTES}-byte limit", 16 << 20 | 1);
        let refused = message(crate::tgs::inspect_file(&path).unwrap_err());
        assert!(refused.contains(&too_large), "{refused}");
        let refused = message(crate::webp::inspect_file(&path).unwrap_err());
        assert!(refused.contains(&too_large), "{refused}");
        assert_eq!(read(&path, MAX_STICKER_BYTES + 1).unwrap().len() as u64, MAX_STICKER_BYTES + 1);
    }

    #[test]
    fn names_the_file_that_doesnt_read() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = ["1.png", "2.png"].map(|name| dir.path().join(name)).to_vec();
        let walker = tgradish_corpus::sprite("walker_2x");
        std::fs::write(&files[0], tgradish_corpus::encode::png(&walker.frames[0])).unwrap();
        std::fs::write(&files[1], b"not a png").unwrap();
        let err = read_sequence(&files, None, &Limits::default(), &CancelToken::new()).unwrap_err();
        assert!(matches!(err, Error::Unreadable { path, .. } if path == files[1]));
    }
}

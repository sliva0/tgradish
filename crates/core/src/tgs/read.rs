//! Reading what a `.tgs` is made of: pixel art, read by tgradish itself,
//! or a video, read through ffmpeg and taken as pixel art.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tgradish_tgs::frames::{self, DecodeOptions, Limits};

use super::TgsOptions;
use crate::CancelToken;
use crate::backend::{Backend, FramesRequest};
use crate::error::{Error, Result};
use crate::source::{self, Aseprite};
use crate::telegram;

/// What the inputs hold, as [`convert`](super::convert) reads them.
pub(super) struct Loaded {
    pub(super) animation: frames::Animation,
    /// Tags and layers of an Aseprite file, which the options choose from.
    pub(super) tags: Vec<frames::aseprite::Tag>,
    pub(super) layers: Vec<frames::aseprite::LayerInfo>,
    /// How long each frame of an Aseprite file shows, in seconds, its
    /// tags' or not.
    pub(super) durations: Vec<f64>,
    /// A video, read through ffmpeg from where `start` says.
    pub(super) video: Option<VideoPart>,
    /// What reading gave up, worth saying.
    pub(super) warnings: Vec<String>,
}

/// What of a video was read, from where `start` says.
pub(super) struct VideoPart {
    /// How long the part used lasts, where that is known: what was read
    /// can go on to the end of its last frame, or past what plays.
    pub(super) length: Option<Duration>,
}

impl Loaded {
    fn art(animation: frames::Animation) -> Loaded {
        Loaded {
            animation,
            tags: Vec::new(),
            layers: Vec::new(),
            durations: Vec::new(),
            video: None,
            warnings: Vec::new(),
        }
    }
}

/// Pixel art as [`convert`](super::convert) reads it, with the tags and layers of an
/// Aseprite file, read from the same bytes; or a video, read through
/// `backend`. `cancel` is asked between files.
pub(super) fn load(
    inputs: &[PathBuf],
    sequence: bool,
    options: &TgsOptions,
    backend: Option<&Backend>,
    cancel: &CancelToken,
) -> Result<Loaded> {
    let limits = Limits::default();
    if sequence || inputs.len() > 1 {
        return Ok(Loaded::art(source::read_sequence(inputs, options.fps, cancel)?));
    }
    let [input] = inputs else {
        return Err(Error::NothingToConvert);
    };
    cancel.check()?;
    if !reads_as_art(input)? {
        let backend = backend.ok_or_else(|| {
            Error::FfmpegNotFound(crate::error::FfmpegMissing::ForVideo(input.clone()))
        })?;
        return read_video(backend, input, options, cancel);
    }
    let bytes = source::read(input)?;
    cancel.check()?;
    let sheet = options.sheet_layout()?;
    // a sheet is one picture, whatever tag is asked for
    let tag = options.tag.clone().filter(|_| sheet.is_none());
    let chosen =
        crate::drawn::layers(options.show_layers.as_deref(), options.hide_layers.as_deref());
    let decoded = match frames::Format::detect(&bytes) {
        Some(frames::Format::Aseprite) => {
            source::aseprite(&bytes, tag.as_deref(), &chosen, &limits)
        }
        _ => {
            let options = DecodeOptions { tag, layers: chosen };
            frames::decode(&bytes, &options, &limits)
                .map(|animation| (animation, Aseprite::default()))
        }
    };
    let (animation, aseprite) = match sheet {
        // a sheet is one picture: an animation's first frame
        Some(sheet) => decoded.and_then(|(image, aseprite)| {
            let (width, height) = (image.width(), image.height());
            let first = image.into_frames().swap_remove(0);
            let picture = frames::Animation::new(width, height, vec![first])?;
            Ok((frames::split_sheet(&picture, &sheet, &limits)?, aseprite))
        }),
        None => decoded,
    }
    .map_err(|err| source::unreadable(input, err))?;
    let Aseprite { tags, layers, durations } = aseprite;
    Ok(Loaded { tags, layers, durations, ..Loaded::art(animation) })
}

/// Whether converting `inputs` into a `.tgs` reads a video through ffmpeg.
pub fn needs_ffmpeg(inputs: &[PathBuf], sequence: bool) -> bool {
    match inputs {
        [input] if !sequence => matches!(reads_as_art(input), Ok(false)),
        _ => false,
    }
}

/// Whether the pixel art reader reads `path`, by its first bytes: a
/// picture or an Aseprite file, not a video.
fn reads_as_art(path: &Path) -> Result<bool> {
    let mut start = Vec::with_capacity(32);
    std::fs::File::open(path)
        .and_then(|file| std::io::Read::read_to_end(&mut std::io::Read::take(file, 32), &mut start))
        .map_err(|err| Error::Unreadable { path: path.to_path_buf(), message: err.to_string() })?;
    Ok(frames::Format::detect(&start).is_some())
}

/// Most frames a second a sticker shows.
const STICKER_FPS: f64 = tgradish_tgs::limits::telegram::FPS as f64;

/// The part of a video a sticker can show, read through ffmpeg: from
/// `start`, for as long as `length` says and the tempo lets play in 3
/// seconds, at most as many frames a second as the sticker shows at that
/// tempo, and no more than fit in memory.
fn read_video(
    backend: &Backend,
    input: &Path,
    options: &TgsOptions,
    cancel: &CancelToken,
) -> Result<Loaded> {
    let chosen =
        crate::drawn::layers(options.show_layers.as_deref(), options.hide_layers.as_deref());
    if let Some(err) = crate::drawn::not_aseprite(options.tag.as_deref(), &chosen) {
        return Err(err);
    }
    if options.sheet.is_some() {
        return Err(Error::invalid("sheet", "reads a picture, not a video"));
    }
    let start = options.start.unwrap_or(0.0);
    if start.is_nan() || start < 0.0 {
        return Err(Error::invalid("start", format!("{start} is not valid")));
    }
    if let Some(length) = options.length.filter(|length| length.is_nan() || *length <= 0.0) {
        return Err(Error::invalid("length", format!("{length} is not more than 0")));
    }
    let probe = backend.probe(input, cancel)?;
    let unreadable = |message: String| Error::Unreadable { path: input.to_path_buf(), message };
    let limits = Limits::default();
    let frame_bytes = (probe.width as usize)
        .checked_mul(probe.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|_| probe.width.max(probe.height) <= limits.max_dimension);
    // half the budget: normalising holds a copy of the frames it keeps
    // beside them
    let fit = frame_bytes.map_or(0, |bytes| limits.max_bytes / 2 / bytes.max(1));
    if fit == 0 {
        let (width, height) = (probe.width, probe.height);
        return Err(unreadable(format!("its frames, {width}x{height}, are too large to read")));
    }
    let available = probe.duration.filter(|_| !probe.still_image).map(|whole| whole - start);
    if let Some(available) = available.filter(|&available| available <= 0.0) {
        let reason =
            format!("{start} is past the end of the video, {:.3} s long", start + available);
        return Err(Error::invalid("start", reason));
    }
    // what is used: as long as asked, as far as the video goes
    let used = match (options.length, available) {
        (Some(length), Some(available)) => Some(length.min(available)),
        (length, available) => length.or(available),
    };
    let tempo = options.tempo_or_default(used);
    let fps = probe.fps_or_default().min(STICKER_FPS / tempo);
    let read = match (options.tempo, used) {
        (None, Some(used)) => used,
        // what plays in 3 s, or where the video doesn't say how long it is,
        // as much as could at the fastest tempo; and a little more, so
        // cutting the rest is noticed and said
        (tempo, used) => {
            let fastest = tempo.unwrap_or(*crate::options::TEMPOS.end());
            let most = telegram::MAX_SECONDS * fastest + 2.0 / fps;
            used.map_or(most, |used| used.min(most))
        }
    };
    let wanted = (read * fps).ceil() as usize + 1;
    let request = FramesRequest {
        start,
        length: Some(read),
        fps,
        max_side: probe.width.max(probe.height),
        max_frames: wanted.min(fit),
    };
    let decoded = backend.frames(input, &probe, &request, cancel)?;
    if decoded.frames.is_empty() {
        return Err(Error::NoFrame { path: input.to_path_buf(), at: start });
    }
    let mut warnings = Vec::new();
    if wanted > fit {
        warnings.push(format!(
            "read at {:.1} frames a second, as more of {}x{} wouldn't fit in memory",
            decoded.fps, decoded.width, decoded.height
        ));
    }
    // a picture shows as long as pictures do; a video's frames as it times
    // them, the cut made after
    let duration = if probe.still_image {
        frames::DEFAULT_FRAME_DURATION
    } else {
        Duration::try_from_secs_f64(1.0 / decoded.fps).unwrap_or(frames::DEFAULT_FRAME_DURATION)
    };
    let frames = decoded.frames.into_iter().map(|rgba| frames::Frame { rgba, duration });
    let animation = frames::Animation::new(decoded.width, decoded.height, frames.collect())
        .map_err(|err| unreadable(err.to_string()))?;
    // its last frame lasts a whole frame; the part used, as long as asked
    let length = used.and_then(|used| Duration::try_from_secs_f64(used).ok());
    Ok(Loaded { video: Some(VideoPart { length }), warnings, ..Loaded::art(animation) })
}

/// Pixel art as it is read, before anything is cropped or reduced.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA, each frame with how long it shows, in seconds.
    pub frames: Vec<(Vec<u8>, f64)>,
    /// Tags of an Aseprite file, which `tag` can choose from.
    pub tags: Vec<frames::aseprite::Tag>,
    /// Layers of an Aseprite file, as its Layers panel lists them, which
    /// `show-layers` and `hide-layers` choose from.
    pub layers: Vec<frames::aseprite::LayerInfo>,
    /// How long each frame of an Aseprite file shows, in seconds, its
    /// tags' or not.
    pub durations: Vec<f64>,
}

/// Reads pixel art the way [`convert`](super::convert) does, with the options that choose
/// what is read: `tag`, the layers, `sheet`, `sheet_frames` and `fps`; or a
/// video through `backend`, as much as a sticker can show. `cancel` is
/// asked between files.
pub fn read_source(
    inputs: &[PathBuf],
    sequence: bool,
    options: &TgsOptions,
    backend: Option<&Backend>,
    cancel: &CancelToken,
) -> Result<Source> {
    let loaded = load(inputs, sequence, options, backend, cancel)?;
    let (width, height) = (loaded.animation.width(), loaded.animation.height());
    let frames = loaded
        .animation
        .into_frames()
        .into_iter()
        .map(|frame| (frame.rgba, frame.duration.as_secs_f64()))
        .collect();
    let (tags, layers, durations) = (loaded.tags, loaded.layers, loaded.durations);
    Ok(Source { width, height, frames, tags, layers, durations })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_aseprite_file_for_its_tags_frames_and_sheet() {
        let dir = tempfile::tempdir().unwrap();
        let ase = tgradish_corpus::files().into_iter().find(|file| file.name == "walker_2x.ase");
        let ase = ase.unwrap();
        let path = dir.path().join(&ase.name);
        std::fs::write(&path, &ase.bytes).unwrap();
        let read = |options: TgsOptions| {
            let loaded =
                load(std::slice::from_ref(&path), false, &options, None, &CancelToken::new());
            let loaded = loaded.unwrap();
            (loaded.animation.width(), loaded.animation.frames().len(), loaded.tags)
        };
        let (width, frames, tags) = read(TgsOptions::default());
        let names: Vec<_> = tags.iter().map(|tag| tag.name.as_str()).collect();
        assert_eq!((names, frames), (vec!["idle"], ase.sprite.frames.len()));
        let idle = TgsOptions { tag: Some("idle".into()), ..TgsOptions::default() };
        assert_eq!(read(idle).1, 2);
        // a sheet is the first frame, whatever the tag
        let sheet = TgsOptions {
            tag: Some("idle".into()),
            sheet: Some(crate::options::Sheet { columns: 2, rows: 1 }),
            ..TgsOptions::default()
        };
        assert_eq!(read(sheet).0, width / 2);

        // stopped between the files of a sequence
        let cancel = CancelToken::new();
        cancel.cancel();
        let both = [path.clone(), path.clone()];
        let stopped = load(&both, true, &TgsOptions::default(), None, &cancel);
        assert!(matches!(stopped, Err(Error::Cancelled)));
    }
}

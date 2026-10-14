//! Pixel art ffmpeg can't read, drawn for it, for WebM and image
//! stickers. That is Aseprite files, with the tag and layers chosen; the
//! frames of one animation in several files; and animated WebP, which
//! ffmpeg reads only from version 8.
//!
//! tgradish's own reader draws them into a temporary PNG, an animated
//! one when it moves, with each frame's own duration, which ffmpeg then
//! reads as it reads any other input.

use std::path::{Path, PathBuf};

use tgradish_tgs::frames::{self, Animation, LayerInfo, Layers, Limits, Tag};

use crate::CancelToken;
use crate::error::{Error, Result};
use crate::options::Options;
use crate::source::{self, Aseprite};

/// An input as ffmpeg reads it: the file itself, or pixel art drawn into a
/// temporary PNG, which lasts as long as this does.
#[derive(Debug)]
pub struct Readable {
    path: PathBuf,
    dir: Option<tempfile::TempDir>,
    /// What the Aseprite file drawn tells of itself.
    aseprite: Aseprite,
    /// What was drawn, until it is taken, and whether it is a still picture.
    drawn: Option<(Animation, bool)>,
}

impl Readable {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether it is pixel art, drawn.
    pub fn is_drawn(&self) -> bool {
        self.dir.is_some()
    }

    /// The frames drawn, each lasting its own time, and whether they are a
    /// still picture, as ffmpeg reads them, for showing without it.
    pub fn take_drawn(&mut self) -> Option<(Animation, bool)> {
        self.drawn.take()
    }

    /// The tags of the Aseprite file drawn, which `tag` chooses from.
    pub fn tags(&self) -> &[Tag] {
        &self.aseprite.tags
    }

    /// The layers of the Aseprite file drawn, as its Layers panel lists
    /// them, which `show-layers` and `hide-layers` choose from.
    pub fn layers(&self) -> &[LayerInfo] {
        &self.aseprite.layers
    }

    /// How long each frame of the Aseprite file drawn shows, in seconds,
    /// its tags' or not.
    pub fn durations(&self) -> &[f64] {
        &self.aseprite.durations
    }
}

/// Whether `path` is an Aseprite file, by its first bytes.
pub(crate) fn is_aseprite(path: &Path) -> bool {
    let mut start = [0u8; 6];
    std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut start))
        .is_ok_and(|()| frames::Format::detect(&start) == Some(frames::Format::Aseprite))
}

/// Whether `path` is an animated WebP, by its extended header's flags.
fn is_animated_webp(path: &Path) -> bool {
    let mut start = [0u8; 21];
    std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut start))
        .is_ok_and(|()| {
            (&start[..4], &start[8..16]) == (b"RIFF", b"WEBPVP8X") && start[20] & 0x02 != 0
        })
}

/// The layers to show and hide, from the options that name them.
pub fn layers(show: Option<&[String]>, hide: Option<&[String]>) -> Layers {
    let names = |list: Option<&[String]>| list.map(<[String]>::to_vec).unwrap_or_default();
    Layers { show: names(show), hide: names(hide) }
}

/// `input` as ffmpeg can read it: an Aseprite file drawn with the tag and
/// layers `options` choose; with `sequence`, the frames of one animation,
/// image files or folders of them, `fps` a second, 10 unless it says; an
/// animated WebP drawn as it is; and anything else as it is. Only Aseprite
/// files have tags and layers.
pub fn readable(
    input: &Path,
    sequence: Option<&[PathBuf]>,
    options: &Options,
    cancel: &CancelToken,
) -> Result<Readable> {
    let layers = layers(options.show_layers.as_deref(), options.hide_layers.as_deref());
    let aseprite = sequence.is_none() && is_aseprite(input);
    if !aseprite && (options.tag.is_some() || !layers.is_empty()) {
        return Err(Error::InvalidOptions("tags and layers are only in Aseprite files".into()));
    }
    let (animation, aseprite) = match sequence {
        Some(frames) => (source::read_sequence(frames, options.fps, cancel)?, Aseprite::default()),
        None if aseprite => {
            cancel.check()?;
            let bytes = source::read(input)?;
            source::aseprite(&bytes, options.tag.as_deref(), &layers, &Limits::default())
                .map_err(|err| source::unreadable(input, err))?
        }
        None if is_animated_webp(input) => {
            cancel.check()?;
            let bytes = source::read(input)?;
            let animation = frames::decode(&bytes, &frames::DecodeOptions::default())
                .map_err(|err| source::unreadable(input, err))?;
            (animation, Aseprite::default())
        }
        None => {
            let path = input.to_path_buf();
            let aseprite = Aseprite::default();
            return Ok(Readable { path, dir: None, aseprite, drawn: None });
        }
    };
    cancel.check()?;
    let dir = tempfile::Builder::new().prefix("tgradish-drawn-").tempdir()?;
    // named after the input, for ffmpeg's messages
    let stem = input.file_stem().map_or("drawn".into(), |stem| stem.to_string_lossy());
    let path = dir.path().join(format!("{stem}.png"));
    // a sequence plays for as long as its frames, even only one
    let animated = sequence.is_some() || animation.frames().len() > 1;
    write_png(&path, &animation, animated)
        .map_err(|err| Error::Io(std::io::Error::other(err.to_string())))?;
    let drawn = Some((animation, !animated));
    Ok(Readable { path, dir: Some(dir), aseprite, drawn })
}

/// Writes `animation` as a PNG, `animated` with each frame lasting its own
/// time, looping, or else a picture of its first frame.
fn write_png(
    path: &Path,
    animation: &Animation,
    animated: bool,
) -> std::result::Result<(), png::EncodingError> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, animation.width(), animation.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // read once, straight away
    encoder.set_compression(png::Compression::Fast);
    let frames = if animated { animation.frames() } else { &animation.frames()[..1] };
    if animated {
        encoder.set_animated(frames.len() as u32, 0)?;
    }
    let mut writer = encoder.write_header()?;
    for frame in frames {
        if animated {
            let (numerator, denominator) = fraction(frame.duration.as_secs_f64());
            writer.set_frame_delay(numerator, denominator)?;
        }
        writer.write_image_data(&frame.rgba)?;
    }
    writer.finish()
}

/// `seconds` as a fraction of 16-bit numbers, as APNG times frames: the
/// closest of the continued fraction's that fit, so a twelfth of a second
/// stays exact as well as a whole millisecond.
fn fraction(seconds: f64) -> (u16, u16) {
    let most = f64::from(u16::MAX);
    let seconds = seconds.clamp(1.0 / most, most);
    // the last two convergents, numerators and denominators
    let (mut numerators, mut denominators) = ([0.0, 1.0], [1.0, 0.0]);
    let mut rest = seconds;
    let mut best = (seconds.round().max(1.0), 1.0);
    for _ in 0..64 {
        let whole = rest.floor();
        let numerator = whole * numerators[1] + numerators[0];
        let denominator = whole * denominators[1] + denominators[0];
        if numerator > most || denominator > most {
            break;
        }
        if numerator > 0.0 {
            best = (numerator, denominator);
        }
        numerators = [numerators[1], numerator];
        denominators = [denominators[1], denominator];
        let fraction = rest - whole;
        if fraction < 1e-9 {
            break;
        }
        rest = 1.0 / fraction;
    }
    (best.0 as u16, best.1 as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_aseprite_files_for_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        let corpus = |name: &str| &files.iter().find(|file| file.name == name).unwrap().bytes;
        let ase = write("walker.ase", corpus("walker_2x.ase"));
        let read = |path: &Path, options: Options| {
            readable(path, None, &options, &CancelToken::new()).map(|readable| {
                assert!(readable.is_drawn());
                let bytes = std::fs::read(readable.path()).unwrap();
                frames::decode(&bytes, &frames::DecodeOptions::default()).unwrap()
            })
        };
        // every frame, as long as each lasts
        let sprite = frames::Sprite::read(&std::fs::read(&ase).unwrap()).unwrap();
        let all = sprite.animation(None, &Layers::default()).unwrap();
        assert_eq!(read(&ase, Options::default()).unwrap(), all);
        let idle = Options { tag: Some("idle".into()), ..Options::default() };
        assert_eq!(read(&ase, idle).unwrap().frames().len(), 2);
        // its one layer hidden, nothing shows
        let hidden = Options { hide_layers: Some(vec!["art".into()]), ..Options::default() };
        let hidden = read(&ase, hidden).unwrap();
        assert!(hidden.frames().iter().all(|frame| frame.rgba.iter().all(|&byte| byte == 0)));
        // its hidden sketch shown, over the first frame
        let sketch = Options { show_layers: Some(vec!["sketch".into()]), ..Options::default() };
        let sketched = read(&ase, sketch).unwrap();
        assert_ne!(sketched.frames()[0], all.frames()[0]);
        assert_eq!(sketched.frames()[1..], all.frames()[1..]);
        let unknown = Options { show_layers: Some(vec!["sky".into()]), ..Options::default() };
        assert!(read(&ase, unknown).is_err());

        // one frame is a still picture
        let mut still = tgradish_corpus::sprite("walker_2x");
        (still.frames, still.delays) = (still.frames[..1].to_vec(), vec![100]);
        let one = write("one.ase", &tgradish_corpus::encode::aseprite(&still, &[]));
        let drawn = readable(&one, None, &Options::default(), &CancelToken::new()).unwrap();
        let bytes = std::fs::read(drawn.path()).unwrap();
        assert!(
            png::Decoder::new(std::io::Cursor::new(&bytes))
                .read_info()
                .is_ok_and(|reader| reader.info().animation_control.is_none())
        );

        // anything else is read as it is, and has no tags or layers
        let gif = write("walker.gif", corpus("walker_2x.gif"));
        let as_is = readable(&gif, None, &Options::default(), &CancelToken::new()).unwrap();
        assert!(!as_is.is_drawn() && as_is.path() == gif);
        let tagged = Options { tag: Some("idle".into()), ..Options::default() };
        assert!(matches!(
            readable(&gif, None, &tagged, &CancelToken::new()),
            Err(Error::InvalidOptions(_))
        ));
    }

    #[test]
    fn draws_sequences_for_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let [(_, steps), _] = tgradish_corpus::sequences();
        let folder = dir.path().join("steps");
        std::fs::create_dir(&folder).unwrap();
        for (index, frame) in steps.iter().enumerate() {
            let png = tgradish_corpus::encode::png(frame);
            std::fs::write(folder.join(format!("{}.png", index + 1)), png).unwrap();
        }
        let twelve = Options { fps: Some(12.0), ..Options::default() };
        let sequence = [folder.clone()];
        let drawn = readable(&folder, Some(&sequence), &twelve, &CancelToken::new()).unwrap();
        let bytes = std::fs::read(drawn.path()).unwrap();
        let animation = frames::decode(&bytes, &frames::DecodeOptions::default()).unwrap();
        assert_eq!(animation.frames().len(), steps.len());
        let twelfth = std::time::Duration::from_secs(1) / 12;
        for frame in animation.frames() {
            assert!(frame.duration.abs_diff(twelfth).as_micros() < 2, "{:?}", frame.duration);
        }
        // one frame of a sequence lasts, as a picture wouldn't
        let one = [folder.join("1.png")];
        let drawn = readable(&one[0], Some(&one), &twelve, &CancelToken::new()).unwrap();
        let bytes = std::fs::read(drawn.path()).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes)).read_info().unwrap();
        assert_eq!(reader.info().animation_control.map(|control| control.num_frames), Some(1));
        // the frames of a sequence have no tags
        let tagged = Options { tag: Some("idle".into()), ..twelve };
        assert!(readable(&folder, Some(&sequence), &tagged, &CancelToken::new()).is_err());
    }

    #[test]
    fn times_frames_in_fractions_that_fit() {
        assert_eq!(fraction(1.0 / 12.0), (1, 12));
        assert_eq!(fraction(0.1), (1, 10));
        assert_eq!(fraction(0.083), (83, 1000));
        assert_eq!(fraction(2.5), (5, 2));
        // what doesn't fit, as near as does
        assert_eq!(fraction(0.0), (1, u16::MAX));
        assert_eq!(fraction(1e9), (u16::MAX, 1));
        let (numerator, denominator) = fraction(std::f64::consts::PI / 10.0);
        let near = f64::from(numerator) / f64::from(denominator);
        assert!((near - std::f64::consts::PI / 10.0).abs() < 1e-6, "{near}");
    }

    #[test]
    fn draws_animated_webp_for_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let glow = files.iter().find(|file| file.name == "glow.webp").unwrap();
        let path = dir.path().join("glow.webp");
        std::fs::write(&path, &glow.bytes).unwrap();
        let drawn = readable(&path, None, &Options::default(), &CancelToken::new()).unwrap();
        assert!(drawn.is_drawn());
        let decode = |bytes: &[u8]| frames::decode(bytes, &frames::DecodeOptions::default());
        let bytes = std::fs::read(drawn.path()).unwrap();
        assert_eq!(decode(&bytes).unwrap(), decode(&glow.bytes).unwrap());
        // a still one ffmpeg reads as it is
        let still = dir.path().join("still.webp");
        std::fs::write(&still, crate::still::encode(&[255, 0, 0, 255], 1, 1, None).unwrap())
            .unwrap();
        let as_is = readable(&still, None, &Options::default(), &CancelToken::new()).unwrap();
        assert!(!as_is.is_drawn());
    }
}

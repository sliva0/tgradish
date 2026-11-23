//! Renders inputs that ffmpeg cannot read directly (Aseprite files, image
//! sequences, and animated WebP, which ffmpeg only supports from version 8)
//! into a temporary PNG or APNG for WebM and static WebP conversion.

use std::path::{Path, PathBuf};

use tgradish_tgs::frames::aseprite::{LayerInfo, Layers, Tag};
use tgradish_tgs::frames::{self, Animation, Limits};

use crate::CancelToken;
use crate::art::{self, Aseprite};
use crate::backend::Backend;
use crate::error::{Error, FileAction, Result};
use crate::ffmpeg::Probe;
use crate::input::{FileKind, Input, sniff};
use crate::tgs::DEFAULT_FPS;
use crate::webm::{self, Options};

/// Input prepared for ffmpeg: either the original file or a rendered temporary
/// PNG/APNG (cleaned up when dropped).
#[derive(Debug)]
pub struct Readable {
    path: PathBuf,
    dir: Option<tempfile::TempDir>,
    /// Metadata read from an Aseprite input, if applicable.
    aseprite: Aseprite,
    /// Rendered animation and still-image flag, available until taken.
    drawn: Option<(Animation, bool)>,
    /// Exact `(duration, fps, base_fps)` for an image sequence (see
    /// [`sequence_timing`]).
    sequence: Option<(f64, f64, f64)>,
    /// Cumulative frame boundaries in seconds (start of each frame plus end of
    /// the last). Empty for still images and non-drawn inputs.
    times: Vec<f64>,
}

impl Readable {
    /// Path that ffmpeg should read (the original input or the rendered PNG).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Probes the file with `backend`, overriding sequence duration and frame
    /// rate with exact values so APNG rounding never drops the last frame.
    pub fn probe(&self, backend: &Backend, cancel: &CancelToken) -> Result<Probe> {
        Ok(self.timed(backend.probe(&self.path, cancel)?))
    }

    /// Computes a [`Probe`] directly from the drawn frames without running
    /// ffmpeg. Pass the `animation` and `still` flag returned by
    /// [`Readable::take_drawn`].
    pub fn probe_drawn(&self, animation: &Animation, still: bool) -> Probe {
        let seconds = animation.frames().iter().map(|frame| frame.duration.as_secs_f64());
        let length: f64 = seconds.clone().sum();
        let shortest = seconds.fold(f64::INFINITY, f64::min);
        let count = animation.frames().len() as f64;
        self.timed(Probe {
            format: "apng".into(),
            codec: "apng".into(),
            width: animation.width(),
            height: animation.height(),
            fps: (!still).then(|| count / length),
            base_fps: (!still).then(|| 1.0 / shortest),
            duration: (!still).then_some(length),
            start: 0.0,
            alpha: true,
            still_image: still,
            orientation: Default::default(),
            decoder: None,
        })
    }

    /// Replaces probed duration and frame rate with exact sequence timing.
    fn timed(&self, mut probe: Probe) -> Probe {
        if let Some((duration, fps, base_fps)) = self.sequence.filter(|_| !probe.still_image) {
            probe.duration = Some(duration);
            (probe.fps, probe.base_fps) = (Some(fps), Some(base_fps));
        }
        probe
    }

    /// Returns true if the input was rendered to a temporary PNG.
    pub fn is_drawn(&self) -> bool {
        self.dir.is_some()
    }

    /// Snaps rounded trim boundaries in `options` to the drawn frame timestamps
    /// (see [`webm::drawn_cut`]).
    pub fn cut(&self, options: &Options) -> Options {
        webm::drawn_cut(options, &self.times)
    }

    /// Takes the rendered animation and its `still` flag for previewing without
    /// ffmpeg.
    pub fn take_drawn(&mut self) -> Option<(Animation, bool)> {
        self.drawn.take()
    }

    /// Tags present in a drawn Aseprite file.
    pub fn tags(&self) -> &[Tag] {
        &self.aseprite.tags
    }

    /// Layers present in a drawn Aseprite file, in Layers-panel order.
    pub fn layers(&self) -> &[LayerInfo] {
        &self.aseprite.layers
    }

    /// Frame durations (in seconds) of all frames in a drawn Aseprite file.
    pub fn durations(&self) -> &[f64] {
        &self.aseprite.durations
    }
}

/// Builds an Aseprite [`Layers`] filter from optional show/hide name lists.
pub fn layers(show: Option<&[String]>, hide: Option<&[String]>) -> Layers {
    let names = |list: Option<&[String]>| list.map(<[String]>::to_vec).unwrap_or_default();
    Layers { show: names(show), hide: names(hide) }
}

/// Returns an invalid-option error if `tag` or `chosen` layers were set on a
/// non-Aseprite input, or `None` if neither was set.
pub(crate) fn not_aseprite(tag: Option<&str>, chosen: &Layers) -> Option<Error> {
    let option = match (tag, chosen.show.is_empty()) {
        (Some(_), _) => "tag",
        (None, false) => "show-layers",
        (None, true) if !chosen.hide.is_empty() => "hide-layers",
        (None, true) => return None,
    };
    Some(Error::invalid(option, "only applies to Aseprite files"))
}

/// Prepares `input` so ffmpeg can read it. Renders Aseprite files (applying
/// `tag` and layer visibility), image sequences (at `options.fps` or
/// [`DEFAULT_FPS`]), and animated WebP files into a temporary PNG/APNG. All
/// other files are passed through directly. Rejects Aseprite-only options on
/// non-Aseprite inputs.
pub fn readable(input: &Input, options: &Options, cancel: &CancelToken) -> Result<Readable> {
    input.refuse_folder()?;
    let layers = layers(options.show_layers.as_deref(), options.hide_layers.as_deref());
    let aseprite = matches!(input, Input::File(path) if sniff(path) == FileKind::Aseprite);
    if let Some(err) = not_aseprite(options.tag.as_deref(), &layers).filter(|_| !aseprite) {
        return Err(err);
    }
    let sequence = matches!(input, Input::Sequence(_));
    let stop = || cancel.is_cancelled();
    let (animation, aseprite) = match input {
        Input::Sequence(frames) => (
            art::read_sequence(frames, options.fps, &Limits::default(), cancel)?,
            Aseprite::default(),
        ),
        Input::File(path) if aseprite => {
            cancel.check()?;
            let bytes = art::read(path, art::most_bytes(&Limits::default()))?;
            let tag = options.tag.clone();
            let decode = frames::DecodeOptions { tag, layers, cancelled: Some(&stop) };
            art::aseprite(&bytes, &decode, &Limits::default())
                .map_err(|err| art::undecodable(path, err))?
        }
        Input::File(path) if sniff(path) == (FileKind::Webp { animated: true }) => {
            cancel.check()?;
            let bytes = art::read(path, art::most_bytes(&Limits::default()))?;
            let decode = frames::DecodeOptions {
                cancelled: Some(&stop),
                ..frames::DecodeOptions::default()
            };
            let animation = frames::decode(&bytes, &decode, &Limits::default())
                .map_err(|err| art::undecodable(path, err))?;
            (animation, Aseprite::default())
        }
        Input::File(path) => {
            let (path, aseprite) = (path.clone(), Aseprite::default());
            let times = Vec::new();
            return Ok(Readable { path, dir: None, aseprite, drawn: None, sequence: None, times });
        }
    };
    cancel.check()?;
    let dir = tempfile::Builder::new()
        .prefix("tgradish-drawn-")
        .tempdir()
        .map_err(Error::io(FileAction::Create, std::env::temp_dir()))?;
    // named after the input, for ffmpeg's messages
    let stem = input.first().file_stem().map_or("drawn".into(), |stem| stem.to_string_lossy());
    let path = dir.path().join(format!("{stem}.png"));
    // a sequence plays for as long as its frames, even only one
    let animated = sequence || animation.frames().len() > 1;
    write_png(&path, &animation, animated)
        .map_err(|err| Error::io(FileAction::Write, &path)(std::io::Error::other(err)))?;
    let rate = sequence.then(|| sequence_timing(&animation, options.fps.unwrap_or(DEFAULT_FPS)));
    let mut times = Vec::new();
    if animated {
        let mut at = std::time::Duration::ZERO;
        times.push(0.0);
        for frame in animation.frames() {
            at = at.saturating_add(frame.duration);
            times.push(at.as_secs_f64());
        }
    }
    let drawn = Some((animation, !animated));
    Ok(Readable { path, dir: Some(dir), aseprite, drawn, sequence: rate, times })
}

/// Computes `(duration, fps, base_fps)` for an image sequence.
/// When every frame has the uniform duration for `fps`, uses exact arithmetic
/// so ffmpeg's APNG delay rounding cannot cut off the final frame. When the
/// sequence contains animated files with their own timings, uses actual frame
/// durations, matching [`Readable::probe_drawn`].
fn sequence_timing(animation: &Animation, fps: f64) -> (f64, f64, f64) {
    let frames = animation.frames();
    let still = art::frame_duration(Some(fps)).ok();
    if still.is_some_and(|still| frames.iter().all(|frame| frame.duration == still)) {
        return (frames.len() as f64 / fps, fps, fps);
    }
    let seconds = frames.iter().map(|frame| frame.duration.as_secs_f64());
    let length: f64 = seconds.clone().sum();
    let shortest = seconds.fold(f64::INFINITY, f64::min);
    (length, frames.len() as f64 / length, 1.0 / shortest)
}

/// Writes `animation` as a still PNG or looping APNG with per-frame delays.
fn write_png(
    path: &Path,
    animation: &Animation,
    animated: bool,
) -> std::result::Result<(), png::EncodingError> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, animation.width(), animation.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // Temporary file read immediately once; prioritise speed over size.
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

/// Approximates `seconds` as a `u16 / u16` fraction for APNG frame delays using
/// continued-fraction convergents, keeping values like `1/12` and whole
/// milliseconds exact.
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
            readable(&Input::File(path.to_path_buf()), &options, &CancelToken::new()).map(
                |readable| {
                    assert!(readable.is_drawn());
                    let bytes = std::fs::read(readable.path()).unwrap();
                    frames::decode(&bytes, &frames::DecodeOptions::default(), &Limits::default())
                        .unwrap()
                },
            )
        };
        // every frame, as long as each lasts
        let sprite =
            frames::aseprite::Sprite::read(&std::fs::read(&ase).unwrap(), &Limits::default())
                .unwrap();
        let all = sprite.animation(&frames::DecodeOptions::default()).unwrap();
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
        let drawn =
            readable(&Input::File(one.clone()), &Options::default(), &CancelToken::new()).unwrap();
        let bytes = std::fs::read(drawn.path()).unwrap();
        assert!(
            png::Decoder::new(std::io::Cursor::new(&bytes))
                .read_info()
                .is_ok_and(|reader| reader.info().animation_control.is_none())
        );

        // anything else is read as it is, and has no tags or layers
        let gif = write("walker.gif", corpus("walker_2x.gif"));
        let as_is =
            readable(&Input::File(gif.clone()), &Options::default(), &CancelToken::new()).unwrap();
        assert!(!as_is.is_drawn() && as_is.path() == gif);
        let tagged = Options { tag: Some("idle".into()), ..Options::default() };
        assert!(matches!(
            readable(&Input::File(gif.clone()), &tagged, &CancelToken::new()),
            Err(Error::InvalidOption { option: "tag", .. })
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
        let sequence = Input::Sequence(vec![folder.clone()]);
        let drawn = readable(&sequence, &twelve, &CancelToken::new()).unwrap();
        let bytes = std::fs::read(drawn.path()).unwrap();
        let animation =
            frames::decode(&bytes, &frames::DecodeOptions::default(), &Limits::default()).unwrap();
        assert_eq!(animation.frames().len(), steps.len());
        let twelfth = std::time::Duration::from_secs(1) / 12;
        for frame in animation.frames() {
            assert!(frame.duration.abs_diff(twelfth).as_micros() < 2, "{:?}", frame.duration);
        }
        // as ffmpeg would find it, worked out without it, at the rate asked
        let mut drawn = drawn;
        let (taken, still) = drawn.take_drawn().unwrap();
        let probe = drawn.probe_drawn(&taken, still);
        assert!(!probe.still_image && probe.alpha);
        assert_eq!((probe.width, probe.height), (animation.width(), animation.height()));
        assert_eq!((probe.fps, probe.base_fps), (Some(12.0), Some(12.0)));
        assert_eq!(probe.duration, Some(steps.len() as f64 / 12.0));
        // one frame of a sequence lasts, as a picture wouldn't
        let one = Input::Sequence(vec![folder.join("1.png")]);
        let drawn = readable(&one, &twelve, &CancelToken::new()).unwrap();
        let bytes = std::fs::read(drawn.path()).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes)).read_info().unwrap();
        assert_eq!(reader.info().animation_control.map(|control| control.num_frames), Some(1));
        // the frames of a sequence have no tags
        let tagged = Options { tag: Some("idle".into()), ..twelve };
        assert!(readable(&sequence, &tagged, &CancelToken::new()).is_err());
    }

    #[test]
    fn times_sequences_with_an_animation_in_them_by_its_frames() {
        // ten frames of 40 ms, then a picture at 12 fps: 0.483 s, not the
        // 11 / 12 s eleven pictures would last
        let dir = tempfile::tempdir().unwrap();
        let canvas = |shade: u8| {
            let mut canvas = tgradish_corpus::Canvas::new(2, 2);
            canvas.rect(0, 0, 2, 2, [shade, 100, 40, 255]);
            canvas
        };
        let gif = tgradish_corpus::Sprite {
            name: "blink",
            about: "",
            frames: (0..10).map(|frame| canvas(frame * 20)).collect(),
            delays: vec![40; 10],
            scale: 1,
            likely_scale: None,
        };
        let files = [dir.path().join("1.gif"), dir.path().join("2.png")];
        std::fs::write(&files[0], tgradish_corpus::encode::gif(&gif)).unwrap();
        std::fs::write(&files[1], tgradish_corpus::encode::png(&canvas(250))).unwrap();
        let twelve = Options { fps: Some(12.0), ..Options::default() };
        let input = Input::Sequence(files.to_vec());
        let mut drawn = readable(&input, &twelve, &CancelToken::new()).unwrap();
        let (taken, still) = drawn.take_drawn().unwrap();
        let probe = drawn.probe_drawn(&taken, still);
        let length = 10.0 * 0.04 + 1.0 / 12.0;
        let near = |found: Option<f64>, expected: f64| {
            found.is_some_and(|found| (found - expected).abs() < 1e-6)
        };
        assert!(near(probe.duration, length), "{:?}", probe.duration);
        assert!(near(probe.fps, 11.0 / length), "{:?}", probe.fps);
        assert!(near(probe.base_fps, 25.0), "{:?}", probe.base_fps);
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
        let drawn =
            readable(&Input::File(path.clone()), &Options::default(), &CancelToken::new()).unwrap();
        assert!(drawn.is_drawn());
        let decode = |bytes: &[u8]| {
            frames::decode(bytes, &frames::DecodeOptions::default(), &Limits::default())
        };
        let bytes = std::fs::read(drawn.path()).unwrap();
        assert_eq!(decode(&bytes).unwrap(), decode(&glow.bytes).unwrap());
        // a still one ffmpeg reads as it is
        let still = dir.path().join("still.webp");
        std::fs::write(&still, crate::webp::encode(&[255, 0, 0, 255], 1, 1, None).unwrap())
            .unwrap();
        let as_is = readable(&Input::File(still.clone()), &Options::default(), &CancelToken::new())
            .unwrap();
        assert!(!as_is.is_drawn());
    }
}

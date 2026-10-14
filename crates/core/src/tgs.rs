//! Animated stickers (`.tgs`) from pixel art: options, events and the
//! file handling around `tgradish-tgs`, which works on bytes only.

use std::path::{Path, PathBuf};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use tgradish_tgs::check::{Issue, Severity, Stats};
use tgradish_tgs::encode::Effort;
use tgradish_tgs::frames::{self, DecodeOptions, Limits, Sheet};
pub use tgradish_tgs::limits;
use tgradish_tgs::normalise::{self, Report};
pub use tgradish_tgs::reduce::{Compromise, Kind, Reduction};
use tgradish_tgs::sticker::{self, Fit, Progress, Step};

use crate::CancelToken;
use crate::backend::{Backend, FramesRequest};
use crate::error::{Error, Result};
use crate::options::{Crop, Resize, Speed, TEMPOS};
use crate::preview::Preview;
use crate::telegram::{self, Target};

/// Frame rate of still images, sprite sheets and image sequences unless
/// `fps` says otherwise.
pub const DEFAULT_FPS: f64 = 10.0;

/// Whether the art is kept as it is unless asked otherwise: changing it to
/// fit is the user's to choose.
pub const LOSSLESS: bool = true;

/// Largest `.tgs` Telegram accepts, in bytes.
pub const MAX_BYTES: u64 = tgradish_tgs::limits::telegram::MAX_BYTES as u64;

/// Options for `.tgs` output. Like the WebM [`Options`](crate::options::Options),
/// every field is optional so presets and flags can be layered.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TgsOptions {
    /// What to make. Both are square animations. [default: sticker]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Seconds to skip at the start of the input.
    #[schemars(range(min = 0.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Seconds of the input to use, from `start`. [default: the rest of
    /// the input]
    #[schemars(extend("exclusiveMinimum" = 0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// How many times as fast as the input it plays, 0.1 to 10; what still
    /// lasts longer than 3 seconds is cut. [default: as fast as fits a long
    /// input into 3 seconds]
    #[schemars(range(min = 0.1, max = 10.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<f64>,
    /// How hard to look for a small encoding; best takes a few seconds.
    /// [default: best]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    /// Never change the art to make it fit; a sticker that is too large is
    /// reported instead. Set it to false to let fitting change it. [default:
    /// true]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lossless: Option<bool>,
    /// Reductions fitting may use, least visible first. [default: all]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reductions: Option<Vec<Kind>>,
    /// What fitting gives up first: motion (frames merged and dropped,
    /// full detail), detail (smooth motion, a coarser picture), or auto,
    /// whatever changes the art least. [default: auto]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compromise: Option<Compromise>,
    /// The part of the input to use, in input pixels. [default: all of it]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// How art that isn't square fills the square sticker: pad (whole,
    /// with transparent margins), crop (its middle) or
    /// stretch (its art pixels no longer square). [default: pad]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// Keep the input's canvas, or the crop, instead of cropping to the
    /// visible pixels. [default: false]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_canvas: Option<bool>,
    /// Size of one art pixel in input pixels. Pixels off its grid are moved
    /// onto it. [default: detected]
    #[schemars(range(min = 1))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_scale: Option<u32>,
    /// Aseprite tag to export. [default: all frames]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Aseprite layers to show, by name, hidden ones included. [default:
    /// those the file shows]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_layers: Option<Vec<String>>,
    /// Aseprite layers to hide, by name; a hidden group hides the layers in
    /// it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_layers: Option<Vec<String>>,
    /// Read the input as a sprite sheet of COLUMNSxROWS equal cells, frames
    /// row by row, for example 4x2.
    #[schemars(regex(pattern = r"^[1-9][0-9]*x[1-9][0-9]*$"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    /// How many cells of the sprite sheet hold frames. [default: all but
    /// transparent ones at the end]
    #[schemars(range(min = 1))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet_frames: Option<u32>,
    /// Frame rate of sprite sheets and image sequences. [default: 10]
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 60))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Name stored in the sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Name the sticker as made by tgradish. A mark hidden in its shapes
    /// stays either way. [default: true]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
}

impl TgsOptions {
    /// These options with every value `overlay` sets replaced.
    pub fn merged(mut self, overlay: &TgsOptions) -> TgsOptions {
        macro_rules! merge {
            ($($field:ident),* $(,)?) => {
                // destructuring makes this fail to compile when a field is added
                let TgsOptions { $($field: _),* } = overlay;
                $(if overlay.$field.is_some() {
                    self.$field = overlay.$field.clone();
                })*
            };
        }
        merge!(
            target,
            start,
            length,
            tempo,
            speed,
            lossless,
            reductions,
            compromise,
            crop,
            resize,
            keep_canvas,
            pixel_scale,
            tag,
            show_layers,
            hide_layers,
            sheet,
            sheet_frames,
            fps,
            title,
            watermark,
        );
        self
    }

    fn sheet_layout(&self) -> Result<Option<Sheet>> {
        let Some(text) = &self.sheet else { return Ok(None) };
        let invalid = || Error::InvalidOptions(format!("sheet {text:?} is not COLUMNSxROWS"));
        let (columns, rows) = text.split_once(['x', 'X']).ok_or_else(invalid)?;
        let number = |n: &str| n.trim().parse::<u32>().ok().filter(|&n| n > 0).ok_or_else(invalid);
        Ok(Some(Sheet {
            columns: number(columns)?,
            rows: number(rows)?,
            frames: self.sheet_frames,
            frame_duration: self.frame_duration()?,
        }))
    }

    fn frame_duration(&self) -> Result<Duration> {
        frame_duration(self.fps)
    }

    fn seconds(value: Option<f64>, name: &str) -> Result<Option<Duration>> {
        value
            .map(|seconds| {
                Duration::try_from_secs_f64(seconds)
                    .map_err(|_| Error::InvalidOptions(format!("{name} {seconds} is not valid")))
            })
            .transpose()
    }

    /// The settings for the tgs crate.
    pub fn sticker_options(&self) -> Result<sticker::Options> {
        let name = match (self.watermark.unwrap_or(true), &self.title) {
            (true, Some(title)) => Some(format!("{title} (made with tgradish)")),
            (true, None) => Some(format!("made with tgradish {}", env!("CARGO_PKG_VERSION"))),
            (false, title) => title.clone(),
        };
        Ok(sticker::Options {
            normalise: normalise::Options {
                keep_canvas: self.keep_canvas.unwrap_or(false),
                crop: self.crop.map(|crop| normalise::Rect {
                    x: crop.x,
                    y: crop.y,
                    width: crop.width,
                    height: crop.height,
                }),
                pixel_scale: self.pixel_scale,
                start: Self::seconds(self.start, "start")?.unwrap_or_default(),
                length: Self::seconds(self.length, "length")?,
                tempo: match self.tempo {
                    Some(tempo) if !TEMPOS.contains(&tempo) => {
                        let (low, high) = (TEMPOS.start(), TEMPOS.end());
                        return Err(Error::InvalidOptions(format!(
                            "tempo {tempo} is not from {low} to {high}"
                        )));
                    }
                    tempo => tempo,
                },
                square: match self.resize.unwrap_or(Resize::Pad) {
                    Resize::Pad => normalise::Square::Pad,
                    Resize::Crop => normalise::Square::Crop,
                    Resize::Stretch => normalise::Square::Stretch,
                    Resize::Contain => {
                        return Err(Error::InvalidOptions(
                            "TGS stickers are square: resize pad, crop or stretch".into(),
                        ));
                    }
                },
            },
            effort: match self.speed.unwrap_or(Speed::Best) {
                Speed::Fast => Effort::Fast,
                Speed::Balanced => Effort::Balanced,
                Speed::Best => Effort::Best,
            },
            fit: if self.lossless.unwrap_or(LOSSLESS) { Fit::Lossless } else { Fit::Auto },
            reductions: self.reductions.clone().unwrap_or_else(|| Kind::ALL.to_vec()),
            compromise: self.compromise.unwrap_or_default(),
            name,
            mark: Some(crate::mark::Mark::current().to_bytes().to_vec()),
            ..sticker::Options::default()
        })
    }
}

/// Progress of a `.tgs` conversion, printed by `convert --json` one per
/// line.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TgsEvent {
    /// The input is read: what normalising found and did.
    Started { input: PathBuf, report: Box<Report> },
    /// Losslessly, the sticker would come to about this many bytes, more
    /// than fits; fitting starts.
    TooLarge { bytes: usize },
    /// Fitting applied a reduction.
    Reduced { step: Step },
    /// The result is being compressed.
    Packing,
    /// Something about the art worth knowing, as normalising found it.
    Warning { message: String },
    Finished {
        output: PathBuf,
        bytes: u64,
        /// Size of the Lottie JSON inside.
        json_bytes: u64,
        /// Whether anything visible was given up: reductions, a forced pixel
        /// scale, the end of a long input.
        lossy: bool,
        /// Reductions fitting applied, in order.
        steps: Vec<Step>,
        layers: usize,
        rectangles: usize,
        /// Problems Telegram would still have with the result.
        issues: Vec<Issue>,
    },
    /// A conversion failed. Never sent by the library, which returns
    /// errors: the CLI prints them as [`crate::events::Event::Error`], the
    /// same as this, which is here so this stream's schema has it.
    Error { message: String, input: Option<PathBuf> },
}

/// `NAME.sticker.tgs` or `NAME.emoji.tgs` next to the input.
pub fn default_output(input: &Path, target: Target) -> PathBuf {
    crate::fsutil::beside(input, &format!("{}.tgs", target.name()))
}

pub struct TgsRequest {
    /// One animation, image or video, or with `sequence` the frames of one
    /// animation (image files, or directories of them).
    pub inputs: Vec<PathBuf>,
    pub sequence: bool,
    pub output: PathBuf,
    pub options: TgsOptions,
    pub overwrite: bool,
    /// ffmpeg, which reads videos; pixel art doesn't need it.
    pub backend: Option<Backend>,
}

/// What converting made: all that [`TgsEvent::Finished`] says, whether or
/// not events were followed, and a preview.
#[derive(Debug, Clone)]
pub struct TgsOutcome {
    pub output: PathBuf,
    pub bytes: u64,
    /// Size of the Lottie JSON inside.
    pub json_bytes: u64,
    /// See [`TgsEvent::Finished`].
    pub lossy: bool,
    /// Reductions fitting applied, in order.
    pub steps: Vec<Step>,
    pub layers: usize,
    pub rectangles: usize,
    /// Problems Telegram would still have with the result.
    pub issues: Vec<Issue>,
    /// What the sticker shows, for previews.
    pub preview: Preview,
}

/// The longest side of a `.tgs` preview: plenty for a window, and within
/// what any GPU takes as a texture.
pub const PREVIEW_MAX_SIDE: u32 = 1024;

impl Preview {
    /// What `anim` shows, `stretch`ed square as the sticker is, or not.
    fn of(anim: &tgradish_tgs::PixelAnim, stretch: bool) -> Preview {
        let grid = anim.grid();
        let width = *grid.columns.last().unwrap_or(&0);
        let height = *grid.rows.last().unwrap_or(&0);
        // input pixels per preview pixel: an art pixel, unless the canvas
        // cuts those at the sides, which then show only in part
        let whole = |edges: &[u32], size: u32| {
            let unit = grid.scale().max(1);
            let last = size - edges[edges.len().max(2) - 2];
            edges.len() <= 2 || (edges[1].is_multiple_of(unit) && last.is_multiple_of(unit))
        };
        let unit =
            if whole(&grid.columns, width) && whole(&grid.rows, height) { grid.scale() } else { 1 };
        let step = unit.max(width.max(height).div_ceil(PREVIEW_MAX_SIDE)).max(1);
        let most = width.max(height).div_ceil(step);
        // the cell under the middle of each step, or stretched, of each of
        // as many preview pixels as along the longer side
        let cells = |edges: &[u32], size: u32| -> Vec<usize> {
            let middles: Vec<u32> = if stretch {
                (0..most)
                    .map(|i| ((2 * i + 1) as u64 * size as u64 / (2 * most) as u64) as u32)
                    .collect()
            } else {
                (0..size.div_ceil(step)).map(|i| i * step + step / 2).collect()
            };
            middles
                .into_iter()
                .map(|x| edges.partition_point(|&edge| edge <= x.min(size - 1)) - 1)
                .collect()
        };
        let (columns, rows) = (cells(&grid.columns, width), cells(&grid.rows, height));
        let frames = anim
            .frames()
            .iter()
            .map(|frame| {
                let mut rgba = Vec::with_capacity(columns.len() * rows.len() * 4);
                for &row in &rows {
                    for &column in &columns {
                        let colour = frame.pixels[row * anim.width() as usize + column];
                        rgba.extend(anim.palette()[colour as usize]);
                    }
                }
                (rgba, frame.ticks)
            })
            .collect();
        Preview { width: columns.len() as u32, height: rows.len() as u32, frames }
    }
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

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path)
        .map_err(|err| Error::Probe { path: path.to_path_buf(), message: err.to_string() })
}

/// How long each frame of a sequence or sprite sheet shows, at `fps`
/// frames a second, [`DEFAULT_FPS`] unless it says.
fn frame_duration(fps: Option<f64>) -> Result<Duration> {
    let fps = fps.unwrap_or(DEFAULT_FPS);
    let invalid = || Error::InvalidOptions(format!("fps {fps} is not between 0 and 60"));
    if !(fps > 0.0 && fps <= 60.0) {
        return Err(invalid());
    }
    // tiny rates give durations past what Duration holds
    Duration::try_from_secs_f64(1.0 / fps).map_err(|_| invalid())
}

/// The frames of one animation in several files, `inputs`: image files,
/// or folders of them, in name order, `fps` a second. `cancel` is asked
/// between files.
pub(crate) fn read_sequence(
    inputs: &[PathBuf],
    fps: Option<f64>,
    cancel: &CancelToken,
) -> Result<frames::Animation> {
    let files = sequence_files(inputs)?;
    let mut bytes = Vec::with_capacity(files.len());
    for file in &files {
        cancel.check()?;
        bytes.push(read(file)?);
    }
    // a file can't be stopped part of the way, but the rest are left
    let images = bytes.iter().map(Vec::as_slice).take_while(|_| !cancel.is_cancelled());
    let animation = frames::sequence(images, frame_duration(fps)?, &Limits::default());
    cancel.check()?;
    animation.map_err(|err| Error::Probe { path: files[0].clone(), message: err.to_string() })
}

/// What the inputs hold, as [`convert`] reads them.
struct Loaded {
    animation: frames::Animation,
    /// Tags and layers of an Aseprite file, which the options choose from.
    tags: Vec<frames::Tag>,
    layers: Vec<frames::LayerInfo>,
    /// How long each frame of an Aseprite file shows, in seconds, its
    /// tags' or not.
    durations: Vec<f64>,
    /// A video, read through ffmpeg from where `start` says.
    video: Option<VideoPart>,
    /// What reading gave up, worth saying.
    warnings: Vec<String>,
}

/// What of a video was read, from where `start` says.
struct VideoPart {
    /// How long the part used lasts, where that is known: what was read
    /// can go on to the end of its last frame, or past what plays.
    length: Option<Duration>,
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

/// Pixel art as [`convert`] reads it, with the tags and layers of an
/// Aseprite file, read from the same bytes; or a video, read through
/// `backend`. `cancel` is asked between files.
fn load(
    inputs: &[PathBuf],
    sequence: bool,
    options: &TgsOptions,
    backend: Option<&Backend>,
    cancel: &CancelToken,
) -> Result<Loaded> {
    let limits = Limits::default();
    let decode_error = |path: &Path| {
        let path = path.to_path_buf();
        move |err: frames::Error| Error::Probe { path: path.clone(), message: err.to_string() }
    };
    if sequence || inputs.len() > 1 {
        return Ok(Loaded::art(read_sequence(inputs, options.fps, cancel)?));
    }
    let [input] = inputs else {
        return Err(Error::InvalidOptions("nothing to convert".into()));
    };
    cancel.check()?;
    if !reads_as_art(input)? {
        let backend = backend.ok_or_else(|| {
            Error::FfmpegNotFound(format!("{} is a video, which ffmpeg reads", input.display()))
        })?;
        return read_video(backend, input, options, cancel);
    }
    let bytes = read(input)?;
    cancel.check()?;
    let sheet = options.sheet_layout()?;
    // a sheet is one picture, whatever tag is asked for
    let tag = options.tag.clone().filter(|_| sheet.is_none());
    let chosen =
        crate::drawn::layers(options.show_layers.as_deref(), options.hide_layers.as_deref());
    // an Aseprite file is parsed once, for its tags, layers and frames
    let (decoded, tags, layers, durations) = match frames::Format::detect(&bytes) {
        Some(frames::Format::Aseprite) => {
            let sprite = frames::Sprite::read_with(&bytes, &limits).map_err(decode_error(input))?;
            let tags = sprite.tags().to_vec();
            let durations = sprite.durations().iter().map(Duration::as_secs_f64).collect();
            (sprite.animation(tag.as_deref(), &chosen), tags, sprite.layers(), durations)
        }
        _ => {
            let options = DecodeOptions { tag, layers: chosen, limits };
            (frames::decode(&bytes, &options), Vec::new(), Vec::new(), Vec::new())
        }
    };
    let animation = match sheet {
        Some(sheet) => decoded.and_then(|image| frames::split_sheet(&image, &sheet, &limits)),
        None => decoded,
    }
    .map_err(decode_error(input))?;
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
        .map_err(|err| Error::Probe { path: path.to_path_buf(), message: err.to_string() })?;
    Ok(frames::Format::detect(&start).is_some())
}

/// Most frames a second a sticker shows.
const STICKER_FPS: f64 = 60.0;

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
    if options.tag.is_some() || !chosen.is_empty() {
        return Err(Error::InvalidOptions("tags and layers are only in Aseprite files".into()));
    }
    if options.sheet.is_some() {
        return Err(Error::InvalidOptions("a sprite sheet is a picture, not a video".into()));
    }
    let start = options.start.unwrap_or(0.0);
    if start.is_nan() || start < 0.0 {
        return Err(Error::InvalidOptions(format!("start {start} is not valid")));
    }
    if let Some(length) = options.length.filter(|length| length.is_nan() || *length <= 0.0) {
        return Err(Error::InvalidOptions(format!("length {length} is not more than 0")));
    }
    let probe = backend.probe(input, cancel)?;
    let unreadable = |message: String| Error::Probe { path: input.to_path_buf(), message };
    let limits = Limits::default();
    let frame_bytes = (probe.width as usize)
        .checked_mul(probe.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|_| probe.width.max(probe.height) <= limits.max_dimension);
    // what ffmpeg prints is held beside the frames made of it
    let fit = frame_bytes.map_or(0, |bytes| limits.max_bytes / 2 / bytes.max(1));
    if fit == 0 {
        let (width, height) = (probe.width, probe.height);
        return Err(unreadable(format!("its frames, {width}x{height}, are too large to read")));
    }
    let available = probe.duration.filter(|_| !probe.still_image).map(|whole| whole - start);
    if let Some(available) = available.filter(|&available| available <= 0.0) {
        return Err(Error::InvalidOptions(format!(
            "start {start} is past the end of the video, {:.3} s long",
            start + available
        )));
    }
    // what is used: as long as asked, as far as the video goes
    let used = match (options.length, available) {
        (Some(length), Some(available)) => Some(length.min(available)),
        (length, available) => length.or(available),
    };
    // as fast as normalising plays it, which fits a long part into 3 s
    let tempo = options
        .tempo
        .unwrap_or_else(|| used.map_or(1.0, |used| (used / telegram::MAX_SECONDS).max(1.0)));
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
    pub tags: Vec<frames::Tag>,
    /// Layers of an Aseprite file, as its Layers panel lists them, which
    /// `show-layers` and `hide-layers` choose from.
    pub layers: Vec<frames::LayerInfo>,
    /// How long each frame of an Aseprite file shows, in seconds, its
    /// tags' or not.
    pub durations: Vec<f64>,
}

/// Reads pixel art the way [`convert`] does, with the options that choose
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

/// Whether `path` is a `.tgs` sticker: by its extension, or for other
/// names by starting like gzip.
pub fn is_sticker(path: &Path) -> bool {
    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("tgs")) {
        return true;
    }
    let mut start = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut start))
        .is_ok_and(|()| start == [0x1f, 0x8b])
}

/// The Lottie JSON of a `.tgs` (or plain Lottie JSON) file, and the
/// file's size when it was packed.
fn read_lottie(path: &Path) -> Result<(Vec<u8>, Option<usize>)> {
    let bytes = read(path)?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let json = tgradish_tgs::file::unpack(&bytes)
            .map_err(|err| Error::Probe { path: path.to_path_buf(), message: err.to_string() })?;
        Ok((json, Some(bytes.len())))
    } else {
        Ok((bytes, None))
    }
}

/// The Lottie JSON of a `.tgs` (or plain Lottie JSON) file.
pub fn read_json(path: &Path) -> Result<Vec<u8>> {
    read_lottie(path).map(|(json, _)| json)
}

/// Reads a `.tgs` (or plain Lottie JSON) and checks it against Telegram's
/// rules and renderers.
pub fn inspect_file(path: &Path) -> Result<(Stats, Vec<Issue>)> {
    let (json, packed) = read_lottie(path)?;
    tgradish_tgs::check::check(&json, packed)
        .map_err(|message| Error::Probe { path: path.to_path_buf(), message })
}

/// Fails when the output is one of the inputs, even through another path
/// or a link.
fn refuse_overwriting_inputs(request: &TgsRequest) -> Result<()> {
    let Ok(output) = request.output.canonicalize() else { return Ok(()) };
    let inputs = if request.sequence || request.inputs.len() > 1 {
        sequence_files(&request.inputs)?
    } else {
        request.inputs.clone()
    };
    for input in inputs {
        if input.canonicalize().is_ok_and(|input| input == output) {
            return Err(Error::InvalidOptions(format!(
                "the output {} is an input",
                request.output.display()
            )));
        }
    }
    Ok(())
}

/// Converts pixel art into a `.tgs` sticker.
pub fn convert(
    request: &TgsRequest,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(TgsEvent),
) -> Result<TgsOutcome> {
    if request.output.exists() && !request.overwrite {
        return Err(Error::OutputExists(request.output.clone()));
    }
    refuse_overwriting_inputs(request)?;
    let mut settings = request.options.sticker_options()?;
    let loaded = load(
        &request.inputs,
        request.sequence,
        &request.options,
        request.backend.as_ref(),
        cancel,
    )?;
    if let Some(part) = &loaded.video {
        // read from the start: as long as the part used
        settings.normalise.start = Duration::ZERO;
        settings.normalise.length = part.length;
    }
    let input = request.inputs.first().cloned().unwrap_or_default();
    let sticker = sticker::make(
        &loaded.animation,
        &settings,
        &mut |progress| match progress {
            // what reading the art found worth saying, after it
            Progress::Normalised { report } => {
                let found = report.warnings.iter().map(|w| w.to_string());
                let warnings: Vec<String> = loaded.warnings.iter().cloned().chain(found).collect();
                on_event(TgsEvent::Started { input: input.clone(), report });
                for message in warnings {
                    on_event(TgsEvent::Warning { message });
                }
            }
            Progress::TooLarge { bytes } => on_event(TgsEvent::TooLarge { bytes }),
            Progress::Reduced { step } => on_event(TgsEvent::Reduced { step }),
            Progress::Packing => on_event(TgsEvent::Packing),
        },
        &|| cancel.is_cancelled(),
    )
    .map_err(|err| match err {
        tgradish_tgs::Error::Cancelled => Error::Cancelled,
        err => err.into(),
    })?;
    let lossy = sticker.lossy();
    // stopped as the last of it was packed: nothing is written
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    crate::fsutil::write_output(&request.output, &sticker.tgs, request.overwrite)?;
    let outcome = TgsOutcome {
        output: request.output.clone(),
        bytes: sticker.bytes as u64,
        json_bytes: sticker.json_bytes as u64,
        lossy,
        steps: sticker.steps,
        layers: sticker.layers,
        rectangles: sticker.rectangles,
        issues: sticker.issues,
        preview: Preview::of(
            &sticker.anim,
            settings.normalise.square == normalise::Square::Stretch,
        ),
    };
    on_event(TgsEvent::Finished {
        output: outcome.output.clone(),
        bytes: outcome.bytes,
        json_bytes: outcome.json_bytes,
        lossy,
        steps: outcome.steps.clone(),
        layers: outcome.layers,
        rectangles: outcome.rectangles,
        issues: outcome.issues.clone(),
    });
    Ok(outcome)
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
            sheet: Some("2x1".into()),
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

    #[test]
    fn refuses_to_overwrite_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("art.png");
        std::fs::write(&input, b"not even a png").unwrap();
        let request = |output: PathBuf| TgsRequest {
            inputs: vec![dir.path().to_path_buf()],
            sequence: true,
            output,
            options: TgsOptions::default(),
            overwrite: true,
            backend: None,
        };
        let err = convert(&request(input.clone()), &CancelToken::new(), &mut |_| {}).unwrap_err();
        assert!(err.to_string().contains("is an input"), "{err}");
        assert_eq!(std::fs::read(&input).unwrap(), b"not even a png");
        // the same file by another path
        let other = dir.path().join(".").join("art.png");
        assert!(convert(&request(other), &CancelToken::new(), &mut |_| {}).is_err());
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
    fn previews_a_pixel_per_art_pixel() {
        let anim = |width: u32, height: u32, colour: &dyn Fn(u32, u32) -> [u8; 4]| {
            let rgba =
                (0..height).flat_map(|y| (0..width).flat_map(move |x| colour(x, y))).collect();
            let frame = frames::Frame { rgba, duration: Duration::from_millis(100) };
            let animation = frames::Animation::new(width, height, vec![frame]).unwrap();
            tgradish_tgs::normalise(&animation, &Default::default()).unwrap().0
        };
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        // 3x2 art at 4x
        let art = anim(12, 8, &|x, y| if (x / 4 + y / 4) % 2 == 0 { red } else { blue });
        let preview = Preview::of(&art, false);
        assert_eq!((preview.width, preview.height), (3, 2));
        assert_eq!(preview.frames[0].0, [red, blue, red, blue, red, blue].concat());
        // stretched square: its rows twice as high
        let preview = Preview::of(&art, true);
        assert_eq!((preview.width, preview.height), (3, 3));
        let rows = [[red, blue, red], [blue, red, blue], [blue, red, blue]];
        assert_eq!(preview.frames[0].0, rows.concat().concat());
        // too wide to show a pixel per art pixel
        let wide = anim(4000, 2, &|x, _| if x % 2 == 0 { red } else { blue });
        let preview = Preview::of(&wide, false);
        assert_eq!((preview.width, preview.height), (1000, 1));
        assert_eq!(preview.frames[0].0.len(), 1000 * 4);
    }

    #[test]
    fn makes_art_square_three_ways() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let input = dir.path().join("walker.gif");
        std::fs::write(&input, &walker.bytes).unwrap();
        let make = |resize| {
            let options = TgsOptions { resize: Some(resize), ..TgsOptions::default() };
            let request = TgsRequest {
                inputs: vec![input.clone()],
                sequence: false,
                output: dir.path().join(format!("{resize:?}.tgs")),
                options,
                overwrite: true,
                backend: None,
            };
            convert(&request, &CancelToken::new(), &mut |_| {})
        };
        // taller than wide: padded keeps its shape, the others are square
        let padded = make(Resize::Pad).unwrap();
        assert!(padded.preview.width < padded.preview.height);
        for resize in [Resize::Crop, Resize::Stretch] {
            let made = make(resize).unwrap();
            assert!(made.issues.is_empty(), "{resize:?}: {:?}", made.issues);
            assert_eq!(made.preview.width, made.preview.height, "{resize:?}");
        }
        assert!(matches!(make(Resize::Contain), Err(Error::InvalidOptions(_))));
    }

    #[test]
    fn keeps_the_art_unless_asked() {
        let fit = |lossless| TgsOptions { lossless, ..TgsOptions::default() }.sticker_options();
        assert_eq!(fit(None).unwrap().fit, Fit::Lossless);
        assert_eq!(fit(Some(false)).unwrap().fit, Fit::Auto);
    }

    #[test]
    fn refuses_tiny_frame_rates() {
        let slow = TgsOptions { fps: Some(1e-100), ..Default::default() };
        assert!(matches!(slow.frame_duration(), Err(Error::InvalidOptions(_))));
    }

    #[test]
    fn orders_numbers_naturally() {
        let mut files: Vec<PathBuf> =
            ["f10.png", "f2.png", "f1.png", "F3.png"].map(PathBuf::from).to_vec();
        files.sort_by(|a, b| natural_order(a, b));
        assert_eq!(files, ["f1.png", "f2.png", "F3.png", "f10.png"].map(PathBuf::from));
    }

    #[test]
    fn layers_options() {
        let preset = TgsOptions { speed: Some(Speed::Fast), fps: Some(5.0), ..Default::default() };
        let flags = TgsOptions { fps: Some(12.0), lossless: Some(true), ..Default::default() };
        let merged = preset.merged(&flags);
        assert_eq!(
            (merged.speed, merged.fps, merged.lossless),
            (Some(Speed::Fast), Some(12.0), Some(true))
        );
        let settings = merged.sticker_options().unwrap();
        assert_eq!((settings.effort, settings.fit), (Effort::Fast, Fit::Lossless));
        let sheet = TgsOptions { sheet: Some("4x2".into()), ..Default::default() };
        let layout = sheet.sheet_layout().unwrap().unwrap();
        assert_eq!(
            (layout.columns, layout.rows, layout.frame_duration),
            (4, 2, Duration::from_millis(100))
        );
        let bad = TgsOptions { sheet: Some("4 by 2".into()), ..Default::default() };
        assert!(bad.sheet_layout().is_err());
        assert_eq!(default_output(Path::new("a/b.gif"), Target::Emoji), Path::new("a/b.emoji.tgs"));
    }
}

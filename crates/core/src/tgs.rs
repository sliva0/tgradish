//! Animated stickers (`.tgs`) from pixel art: options, events and the
//! file handling around `tgradish-tgs`, which works on bytes only.

use std::path::{Path, PathBuf};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use tgradish_tgs::check::{Issue, Severity, Stats};
use tgradish_tgs::encode::Effort;
use tgradish_tgs::frames::{self, DecodeOptions, Limits, Sheet};
pub use tgradish_tgs::normalise::Long;
use tgradish_tgs::normalise::{self, Report};
pub use tgradish_tgs::reduce::{Compromise, Kind, Reduction};
use tgradish_tgs::sticker::{self, Fit, Progress, Step};

use crate::error::{Error, Result};
use crate::ffmpeg::CancelToken;
use crate::options::{Crop, Speed};
use crate::telegram::Target;

/// Frame rate of still images, sprite sheets and image sequences unless
/// `fps` says otherwise.
pub const DEFAULT_FPS: f64 = 10.0;

/// Largest `.tgs` Telegram accepts, in bytes.
pub const MAX_BYTES: u64 = tgradish_tgs::limits::telegram::MAX_BYTES as u64;

/// Options for `.tgs` output. Like the WebM [`Options`](crate::options::Options),
/// every field is optional so presets and flags can be layered.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TgsOptions {
    /// What to make. Both are 512x512 animations. [default: sticker]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Seconds to skip at the start of the input.
    #[schemars(range(min = 0.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Length of the result in seconds. [default: the rest of the input]
    #[schemars(extend("exclusiveMinimum" = 0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// What to do with more than 3 seconds. [default: speed-up]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long: Option<Long>,
    /// How hard to look for a small encoding; best takes a few seconds.
    /// [default: best]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    /// Never change the art to make it fit; a sticker that is too large is
    /// reported instead. [default: false]
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
            long,
            speed,
            lossless,
            reductions,
            compromise,
            crop,
            keep_canvas,
            pixel_scale,
            tag,
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
        let fps = self.fps.unwrap_or(DEFAULT_FPS);
        let invalid = || Error::InvalidOptions(format!("fps {fps} is not between 0 and 60"));
        if !(fps > 0.0 && fps <= 60.0) {
            return Err(invalid());
        }
        // tiny rates give durations past what Duration holds
        Duration::try_from_secs_f64(1.0 / fps).map_err(|_| invalid())
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
                long: self.long.unwrap_or_default(),
            },
            effort: match self.speed.unwrap_or(Speed::Best) {
                Speed::Fast => Effort::Fast,
                Speed::Balanced => Effort::Balanced,
                Speed::Best => Effort::Best,
            },
            fit: if self.lossless.unwrap_or(false) { Fit::Lossless } else { Fit::Auto },
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
    Started {
        input: PathBuf,
        report: Box<Report>,
    },
    /// Losslessly, the sticker would come to about this many bytes, more
    /// than fits; fitting starts.
    TooLarge {
        bytes: usize,
    },
    /// Fitting applied a reduction.
    Reduced {
        step: Step,
    },
    /// The result is being compressed.
    Packing,
    Warning {
        message: String,
    },
    Finished {
        output: PathBuf,
        bytes: u64,
        /// Size of the Lottie JSON inside.
        json_bytes: u64,
        /// Whether anything visible was given up: reductions, a forced pixel
        /// scale, a long input sped up or cut.
        lossy: bool,
        /// Reductions fitting applied, in order.
        steps: Vec<Step>,
        layers: usize,
        rectangles: usize,
        /// Problems Telegram would still have with the result.
        issues: Vec<Issue>,
    },
    /// A conversion failed. Only printed by the CLI; the library returns
    /// errors instead.
    Error {
        message: String,
        input: Option<PathBuf>,
    },
}

/// `NAME.sticker.tgs` or `NAME.emoji.tgs` next to the input.
pub fn default_output(input: &Path, target: Target) -> PathBuf {
    let name = input.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("frames"));
    input.with_file_name(name).with_extension(format!("{}.tgs", target.name()))
}

pub struct TgsRequest {
    /// One animation or image, or with `sequence` the frames of one
    /// animation (image files, or directories of them).
    pub inputs: Vec<PathBuf>,
    pub sequence: bool,
    pub output: PathBuf,
    pub options: TgsOptions,
    pub overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct TgsOutcome {
    pub output: PathBuf,
    pub bytes: u64,
    pub lossy: bool,
    /// What the sticker shows, for previews.
    pub preview: Preview,
}

/// Frames of a sticker as straight RGBA, each with how many 60 fps frames
/// it shows for. `.tgs` previews have a pixel per art pixel, WebM previews
/// fit the size asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<(Vec<u8>, u32)>,
}

/// The longest side of a `.tgs` preview: plenty for a window, and within
/// what any GPU takes as a texture.
pub const PREVIEW_MAX_SIDE: u32 = 1024;

impl Preview {
    fn of(anim: &tgradish_tgs::PixelAnim) -> Preview {
        let grid = anim.grid();
        let width = *grid.columns.last().unwrap_or(&0);
        let height = *grid.rows.last().unwrap_or(&0);
        // input pixels per preview pixel
        let step = grid.scale().max(width.max(height).div_ceil(PREVIEW_MAX_SIDE)).max(1);
        // the cell under the middle of each step
        let cells = |edges: &[u32], size: u32| -> Vec<usize> {
            (0..size.div_ceil(step))
                .map(|i| (i * step + step / 2).min(size - 1))
                .map(|x| edges.partition_point(|&edge| edge <= x) - 1)
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
fn sequence_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(input)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<_>>()?;
            found.retain(|path| {
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                path.is_file()
                    && ["png", "gif", "webp", "jpg", "jpeg", "bmp", "ase", "aseprite"]
                        .contains(&extension.to_ascii_lowercase().as_str())
            });
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

/// The animation the inputs describe.
fn load(inputs: &[PathBuf], sequence: bool, options: &TgsOptions) -> Result<frames::Animation> {
    let limits = Limits::default();
    let decode_error = |path: &Path| {
        let path = path.to_path_buf();
        move |err: frames::Error| Error::Probe { path: path.clone(), message: err.to_string() }
    };
    if sequence || inputs.len() > 1 {
        let files = sequence_files(inputs)?;
        let bytes: Vec<Vec<u8>> = files.iter().map(|file| read(file)).collect::<Result<_>>()?;
        return frames::sequence(
            bytes.iter().map(Vec::as_slice),
            options.frame_duration()?,
            &limits,
        )
        .map_err(decode_error(&files[0]));
    }
    let [input] = inputs else {
        return Err(Error::InvalidOptions("nothing to convert".into()));
    };
    let bytes = read(input)?;
    match options.sheet_layout()? {
        Some(sheet) => frames::sprite_sheet(&bytes, &sheet, &limits),
        None => frames::decode(&bytes, &DecodeOptions { tag: options.tag.clone(), limits }),
    }
    .map_err(decode_error(input))
}

/// Pixel art as it is read, before anything is cropped or reduced.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA, each frame with how long it shows, in seconds.
    pub frames: Vec<(Vec<u8>, f64)>,
    /// Tags of an Aseprite file, which `tag` can choose from.
    pub tags: Vec<String>,
}

/// Reads pixel art the way [`convert`] does, with the options that choose
/// what is read: `tag`, `sheet`, `sheet_frames` and `fps`.
pub fn read_source(inputs: &[PathBuf], sequence: bool, options: &TgsOptions) -> Result<Source> {
    let animation = load(inputs, sequence, options)?;
    let tags = match inputs {
        [input] if !sequence && !input.is_dir() => {
            let bytes = read(input)?;
            match frames::Format::detect(&bytes) {
                Some(frames::Format::Aseprite) => {
                    frames::Sprite::read_with(&bytes, &Limits::default())
                        .map(|sprite| sprite.tags().iter().map(|tag| tag.name.clone()).collect())
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    let (width, height) = (animation.width(), animation.height());
    let frames = animation
        .into_frames()
        .into_iter()
        .map(|frame| (frame.rgba, frame.duration.as_secs_f64()))
        .collect();
    Ok(Source { width, height, frames, tags })
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
    let settings = request.options.sticker_options()?;
    let animation = load(&request.inputs, request.sequence, &request.options)?;
    let input = request.inputs.first().cloned().unwrap_or_default();
    let sticker = sticker::make(
        &animation,
        &settings,
        &mut |progress| {
            on_event(match progress {
                Progress::Normalised { report } => {
                    TgsEvent::Started { input: input.clone(), report }
                }
                Progress::TooLarge { bytes } => TgsEvent::TooLarge { bytes },
                Progress::Reduced { step } => TgsEvent::Reduced { step },
                Progress::Packing => TgsEvent::Packing,
            })
        },
        &|| cancel.is_cancelled(),
    )
    .map_err(|err| match err {
        tgradish_tgs::Error::Cancelled => Error::Cancelled,
        err => err.into(),
    })?;
    let lossy = sticker.lossy();
    crate::fsutil::write_file(&request.output, &sticker.tgs, request.overwrite).map_err(|err| {
        match err.kind() {
            std::io::ErrorKind::AlreadyExists => Error::OutputExists(request.output.clone()),
            _ => err.into(),
        }
    })?;
    on_event(TgsEvent::Finished {
        output: request.output.clone(),
        bytes: sticker.bytes as u64,
        json_bytes: sticker.json_bytes as u64,
        lossy,
        steps: sticker.steps.clone(),
        layers: sticker.layers,
        rectangles: sticker.rectangles,
        issues: sticker.issues.clone(),
    });
    Ok(TgsOutcome {
        output: request.output.clone(),
        bytes: sticker.bytes as u64,
        lossy,
        preview: Preview::of(&sticker.anim),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let preview = Preview::of(&art);
        assert_eq!((preview.width, preview.height), (3, 2));
        assert_eq!(preview.frames[0].0, [red, blue, red, blue, red, blue].concat());
        // too wide to show a pixel per art pixel
        let wide = anim(4000, 2, &|x, _| if x % 2 == 0 { red } else { blue });
        let preview = Preview::of(&wide);
        assert_eq!((preview.width, preview.height), (1000, 1));
        assert_eq!(preview.frames[0].0.len(), 1000 * 4);
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

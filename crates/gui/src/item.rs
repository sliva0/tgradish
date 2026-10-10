//! A file in the list: what it is, what to make of it, and what came out.

use std::path::{Path, PathBuf};

use eframe::egui;
use tgradish_core::config::Config;
use tgradish_core::options::{Crop, Options};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;
use tgradish_core::tgs::TgsOptions;

use crate::jobs::Job;
use crate::media::{Art, Clip, Load, Video};

/// What kind of input an item is, which limits what it can become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Videos and photos, which only ffmpeg reads: WebM only.
    Video,
    /// Images and GIFs the pixel art reader reads too: either format.
    Image,
    /// Aseprite files, folders of frames and several images as one
    /// animation: `.tgs` only.
    Frames,
}

const IMAGES: [&str; 7] = ["png", "apng", "gif", "webp", "jpg", "jpeg", "bmp"];
const ASEPRITE: [&str; 2] = ["ase", "aseprite"];

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

impl Kind {
    pub fn of(inputs: &[PathBuf], sequence: bool) -> Kind {
        match inputs {
            [input] if !sequence && !input.is_dir() => {
                let extension = extension(input);
                if ASEPRITE.contains(&extension.as_str()) {
                    Kind::Frames
                } else if IMAGES.contains(&extension.as_str()) {
                    Kind::Image
                } else {
                    Kind::Video
                }
            }
            _ => Kind::Frames,
        }
    }

    pub fn allows(self, format: Format) -> bool {
        match self {
            Kind::Video => format == Format::Webm,
            Kind::Image => true,
            Kind::Frames => format == Format::Tgs,
        }
    }

    /// The format to start with; images switch to `.tgs` once they turn
    /// out to be pixel art.
    pub fn first_format(self) -> Format {
        match self {
            Kind::Frames => Format::Tgs,
            Kind::Video | Kind::Image => Format::Webm,
        }
    }
}

/// What the user chose for an item: a preset, then their own values over
/// it. Target, crop and times apply to both formats.
#[derive(Debug, Clone, PartialEq)]
pub struct Choices {
    /// `None` for the default preset of the format.
    pub preset: Option<String>,
    pub target: Target,
    pub crop: Option<Crop>,
    pub start: Option<f64>,
    pub length: Option<f64>,
    /// Values set over the preset's, for WebM and `.tgs` results.
    pub webm: Options,
    pub tgs: TgsOptions,
}

impl Choices {
    pub fn new(target: Target) -> Choices {
        Choices {
            preset: None,
            target,
            crop: None,
            start: None,
            length: None,
            webm: Options::default(),
            tgs: TgsOptions::default(),
        }
    }

    pub fn preset_name<'a>(&'a self, format: Format, config: &'a Config) -> &'a str {
        self.preset.as_deref().unwrap_or_else(|| config.preset_for(format))
    }

    /// The preset's WebM options, which values not set fall back to.
    pub fn webm_base(&self, presets: &Presets, config: &Config) -> Result<Options, String> {
        presets.webm(self.preset_name(Format::Webm, config)).map_err(|err| err.to_string())
    }

    pub fn tgs_base(&self, presets: &Presets, config: &Config) -> Result<TgsOptions, String> {
        presets.tgs(self.preset_name(Format::Tgs, config)).map_err(|err| err.to_string())
    }

    /// Everything a WebM conversion gets.
    pub fn webm_options(&self, presets: &Presets, config: &Config) -> Result<Options, String> {
        let shared = Options {
            target: Some(self.target),
            crop: self.crop,
            start: self.start,
            length: self.length,
            ..Options::default()
        };
        Ok(self.webm_base(presets, config)?.merged(&self.webm).merged(&shared))
    }

    /// Everything a `.tgs` conversion gets.
    pub fn tgs_options(&self, presets: &Presets, config: &Config) -> Result<TgsOptions, String> {
        let shared = TgsOptions {
            target: Some(self.target),
            crop: self.crop,
            start: self.start,
            length: self.length,
            ..TgsOptions::default()
        };
        Ok(self.tgs_base(presets, config)?.merged(&self.tgs).merged(&shared))
    }

    /// The options that change what pixel art is read as, the preset's
    /// included.
    pub fn reading(&self, presets: &Presets, config: &Config) -> TgsOptions {
        let options = self.tgs_options(presets, config).unwrap_or_else(|_| self.tgs.clone());
        TgsOptions {
            tag: options.tag,
            sheet: options.sheet,
            sheet_frames: options.sheet_frames,
            fps: options.fps,
            ..TgsOptions::default()
        }
    }
}

/// Which picture the preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    Input,
    Result,
}

/// Shapes the crop is kept to while dragging it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aspect {
    Free,
    /// The input's own.
    Input,
    Ratio(f64, f64),
}

/// How close a picture is looked at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    /// 1 shows all of the picture.
    pub level: f32,
    /// The picture point in the middle of the view, from 0 to 1.
    pub centre: egui::Vec2,
}

impl Default for Zoom {
    fn default() -> Zoom {
        Zoom { level: 1.0, centre: egui::vec2(0.5, 0.5) }
    }
}

/// How the preview of an item is looked at: not part of the conversion.
#[derive(Debug, Clone)]
pub struct View {
    pub show: Show,
    pub aspect: Aspect,
    /// Seconds into the input.
    pub time: f64,
    /// Seconds into the result.
    pub result_time: f64,
    pub playing: bool,
    pub input_zoom: Zoom,
    pub result_zoom: Zoom,
    /// The small picture of what the result will look like, over the input.
    pub inset: bool,
    /// What the framing was when last shown, to show the input when it
    /// changes.
    pub framing: Option<Framing>,
}

impl View {
    /// The zoom of the picture shown.
    pub fn zoom_mut(&mut self) -> &mut Zoom {
        match self.show {
            Show::Input => &mut self.input_zoom,
            Show::Result => &mut self.result_zoom,
        }
    }
}

impl Default for View {
    fn default() -> View {
        View {
            show: Show::Input,
            aspect: Aspect::Free,
            time: 0.0,
            result_time: 0.0,
            playing: true,
            input_zoom: Zoom::default(),
            result_zoom: Zoom::default(),
            inset: true,
            framing: None,
        }
    }
}

/// The choices that change which part of the input is used and how it is
/// fitted: changing them is something to look at on the input.
#[derive(Debug, Clone, PartialEq)]
pub struct Framing {
    format: Format,
    target: Target,
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
    webm: Options,
    tgs: TgsOptions,
}

impl Framing {
    pub fn of(item: &Item) -> Framing {
        let (webm, tgs) = (&item.choices.webm, &item.choices.tgs);
        Framing {
            format: item.format,
            target: item.choices.target,
            crop: item.choices.crop,
            start: item.choices.start,
            length: item.choices.length,
            webm: Options { resize: webm.resize, scaling: webm.scaling, ..Options::default() },
            tgs: TgsOptions {
                keep_canvas: tgs.keep_canvas,
                pixel_scale: tgs.pixel_scale,
                tag: tgs.tag.clone(),
                sheet: tgs.sheet.clone(),
                sheet_frames: tgs.sheet_frames,
                fps: tgs.fps,
                long: tgs.long,
                ..TgsOptions::default()
            },
        }
    }
}

/// A finished conversion: the job and what it was made from.
pub struct Made {
    pub job: Job,
    pub format: Format,
    pub choices: Choices,
}

pub struct Item {
    pub id: u64,
    /// One file, or the frames of one animation.
    pub inputs: Vec<PathBuf>,
    pub sequence: bool,
    /// Where a pasted image is kept.
    pub pasted: Option<tempfile::TempDir>,
    pub kind: Kind,
    pub format: Format,
    /// The format was not picked by the user yet, so finding pixel art may
    /// change it.
    pub format_guessed: bool,
    pub choices: Choices,
    pub view: View,
    /// The input read by ffmpeg, for WebM.
    pub video: Load<Video>,
    /// The part of the video that is played: where it starts and how long.
    pub part: Option<((f64, f64), Load<Clip>)>,
    /// The frame shown while paused, at the input's full size, and its
    /// time: sharp enough to crop by.
    pub still: Option<(f64, Load<Clip>)>,
    /// The input read as pixel art, for `.tgs`, with what it was read with.
    pub art: Load<Art>,
    pub art_reading: TgsOptions,
    pub input_thumb: Option<egui::TextureHandle>,
    /// The conversion waiting or running.
    pub job: Option<(Job, Format, Choices)>,
    /// The last conversion that made a sticker.
    pub made: Option<Made>,
    /// The last conversion, if it failed, was stopped or found a file in
    /// the way: newer than `made`.
    pub failed: Option<Made>,
    /// A finished WebM result, decoded.
    pub result_clip: Load<Clip>,
    pub result_thumb: Option<egui::TextureHandle>,
}

impl Item {
    pub fn new(id: u64, inputs: Vec<PathBuf>, sequence: bool, target: Target) -> Item {
        let kind = Kind::of(&inputs, sequence);
        Item {
            id,
            inputs,
            sequence,
            pasted: None,
            kind,
            format: kind.first_format(),
            format_guessed: kind == Kind::Image,
            choices: Choices::new(target),
            view: View::default(),
            video: Load::Idle,
            part: None,
            still: None,
            art: Load::Idle,
            art_reading: TgsOptions::default(),
            input_thumb: None,
            job: None,
            made: None,
            failed: None,
            result_clip: Load::Idle,
            result_thumb: None,
        }
    }

    pub fn name(&self) -> String {
        let first = self.inputs.first().and_then(|p| p.file_name());
        let first = first.map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match (self.inputs.len(), &self.pasted) {
            (_, Some(_)) => "pasted image".into(),
            (1, None) if self.sequence => format!("{first} (frames)"),
            (1, None) => first,
            (n, None) => format!("{first} and {} more", n - 1),
        }
    }

    /// The input as it is shown for the current format.
    pub fn input_clip(&self) -> Option<&Clip> {
        match self.format {
            Format::Webm => self.video.ready().map(|video| &video.overview),
            Format::Tgs => self.art.ready().map(|art| &art.clip),
        }
    }

    /// Size of the input picture.
    pub fn input_size(&self) -> Option<(u32, u32)> {
        match self.format {
            Format::Webm => self.video.ready().map(|v| (v.probe.width, v.probe.height)),
            Format::Tgs => self.art.ready().map(|art| (art.clip.width, art.clip.height)),
        }
    }

    /// Length of the input in seconds, `None` for still images.
    pub fn input_length(&self) -> Option<f64> {
        match self.format {
            Format::Webm => self.video.ready().and_then(|video| {
                (!video.probe.still_image).then_some(video.probe.duration).flatten()
            }),
            Format::Tgs => {
                self.art.ready().filter(|art| art.clip.is_animated()).map(|art| art.clip.end())
            }
        }
    }

    /// Why the input can't be shown, if reading it failed.
    pub fn input_error(&self) -> Option<&str> {
        let load = match self.format {
            Format::Webm => match &self.video {
                Load::Failed(message) => Some(message),
                _ => None,
            },
            Format::Tgs => match &self.art {
                Load::Failed(message) => Some(message),
                _ => None,
            },
        };
        load.map(String::as_str)
    }

    /// The range of the input that is used, in seconds.
    pub fn range(&self) -> Option<(f64, f64)> {
        let length = self.input_length()?;
        let start = self.choices.start.unwrap_or(0.0).clamp(0.0, length);
        let end = self.choices.length.map_or(length, |l| (start + l).min(length));
        Some((start, end))
    }

    /// Whether the last sticker was made with what is chosen now.
    pub fn result_is_current(&self) -> bool {
        self.made
            .as_ref()
            .is_some_and(|made| made.format == self.format && made.choices == self.choices)
    }

    pub fn is_busy(&self) -> bool {
        self.job.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_inputs_apart() {
        let dir = tempfile::tempdir().unwrap();
        let path = |name: &str| vec![PathBuf::from(name)];
        assert_eq!(Kind::of(&path("a.MP4"), false), Kind::Video);
        assert_eq!(Kind::of(&path("a.gif"), false), Kind::Image);
        assert_eq!(Kind::of(&path("photo.JPG"), false), Kind::Image);
        assert_eq!(Kind::of(&path("a.mkv"), false), Kind::Video);
        assert_eq!(Kind::of(&path("a.aseprite"), false), Kind::Frames);
        assert_eq!(Kind::of(&[dir.path().to_path_buf()], false), Kind::Frames);
        assert_eq!(Kind::of(&[PathBuf::from("1.png"), PathBuf::from("2.png")], true), Kind::Frames);
        assert!(!Kind::Video.allows(Format::Tgs) && !Kind::Frames.allows(Format::Webm));
    }

    #[test]
    fn reads_art_with_the_presets_sheet() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sheets.toml"), "[tgs]\nsheet = '4x1'\nfps = 2.0\n")
            .unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let mut choices = Choices::new(Target::Sticker);
        choices.preset = Some("sheets".into());
        let reading = choices.reading(&presets, &Config::default());
        assert_eq!((reading.sheet.as_deref(), reading.fps), (Some("4x1"), Some(2.0)));
    }

    #[test]
    fn layers_choices_over_the_preset() {
        let config = Config::default();
        let presets = Presets::builtin();
        let mut choices = Choices::new(Target::Emoji);
        choices.preset = Some("fast".into());
        choices.webm.crf = Some(20);
        choices.start = Some(1.0);
        let options = choices.webm_options(&presets, &config).unwrap();
        assert_eq!(options.target, Some(Target::Emoji));
        assert_eq!((options.crf, options.start), (Some(20), Some(1.0)));
        assert_eq!(options.speed, presets.webm("fast").unwrap().speed);
        let tgs = choices.tgs_options(&presets, &config).unwrap();
        assert_eq!((tgs.target, tgs.start), (Some(Target::Emoji), Some(1.0)));
    }
}

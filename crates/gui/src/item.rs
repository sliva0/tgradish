//! A file in the list: what it is, what to make of it, and what came out.

use std::path::PathBuf;

use eframe::egui;
use tgradish_core::config::Config;
use tgradish_core::inputs;
use tgradish_core::options::{Crop, Fit, Options, Resize, Scaling, Spoof};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::{self, Target};
use tgradish_core::tgs::TgsOptions;
use tgradish_tgs::frames::{LayerInfo, Tag};

use crate::jobs::Job;
use crate::media::{Art, Clip, Load, Video};
use crate::timeline::{Cut, Frames};

/// What kind of input an item is, which limits what it can become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Videos and photos, which only ffmpeg reads: any format, `.tgs` from
    /// the frames ffmpeg reads.
    Video,
    /// Images and GIFs the pixel art reader reads too: any format.
    Image,
    /// Aseprite files, which the pixel art reader draws for ffmpeg: any
    /// format.
    Aseprite,
    /// Folders of frames and several images as one animation, which the
    /// pixel art reader joins, and draws for ffmpeg: any format.
    Frames,
}

impl Kind {
    pub fn of(inputs: &[PathBuf], sequence: bool) -> Kind {
        match inputs {
            [input] if !sequence && !input.is_dir() => {
                let extension = inputs::extension(input);
                if inputs::ASEPRITE.contains(&extension.as_str()) {
                    Kind::Aseprite
                } else if inputs::IMAGES.contains(&extension.as_str()) {
                    Kind::Image
                } else {
                    Kind::Video
                }
            }
            _ => Kind::Frames,
        }
    }

    /// The format to start with; images switch to `.tgs` once they turn
    /// out to be pixel art.
    pub fn first_format(self) -> Format {
        match self {
            Kind::Aseprite | Kind::Frames => Format::Tgs,
            Kind::Video | Kind::Image => Format::Webm,
        }
    }
}

/// What the user chose for an item: a preset, then their own values over
/// it. Target, crop, times and what of an Aseprite file is read apply to
/// every format.
#[derive(Debug, Clone, PartialEq)]
pub struct Choices {
    /// `None` for the default preset of the format.
    pub preset: Option<String>,
    pub target: Target,
    /// An Aseprite tag, and layers shown and hidden over what the file
    /// shows.
    pub tag: Option<String>,
    pub show_layers: Option<Vec<String>>,
    pub hide_layers: Option<Vec<String>>,
    pub crop: Option<Crop>,
    pub start: Option<f64>,
    pub length: Option<f64>,
    /// Seconds into the input of the frame an image sticker is made of.
    pub frame: Option<f64>,
    /// Values set over the preset's, for WebM and `.tgs` results; images
    /// take WebM's.
    pub webm: Options,
    pub tgs: TgsOptions,
}

impl Choices {
    pub fn new(target: Target) -> Choices {
        Choices {
            preset: None,
            target,
            tag: None,
            show_layers: None,
            hide_layers: None,
            crop: None,
            start: None,
            length: None,
            frame: None,
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

    /// The preset's WebM options with the item's over them, those of
    /// hidden settings too.
    pub fn webm_merged(&self, presets: &Presets, config: &Config) -> Result<Options, String> {
        let shared = Options {
            target: Some(self.target),
            tag: self.tag.clone(),
            show_layers: self.show_layers.clone(),
            hide_layers: self.hide_layers.clone(),
            crop: self.crop,
            start: self.start,
            length: self.length,
            ..Options::default()
        };
        Ok(self.webm_base(presets, config)?.merged(&self.webm).merged(&shared))
    }

    /// Everything a WebM conversion gets.
    pub fn webm_options(&self, presets: &Presets, config: &Config) -> Result<Options, String> {
        let merged = self.webm_merged(presets, config)?;
        Ok(shown(merged.clone(), &merged))
    }

    /// Everything an image conversion gets: WebM's options a still picture
    /// has, and the frame for its start.
    pub fn image_options(&self, presets: &Presets, config: &Config) -> Result<Options, String> {
        let options = self.webm_options(presets, config)?;
        Ok(Options { start: self.frame, ..tgradish_core::still::picture_options(&options) })
    }

    /// Everything a `.tgs` conversion gets.
    pub fn tgs_options(&self, presets: &Presets, config: &Config) -> Result<TgsOptions, String> {
        let shared = TgsOptions {
            target: Some(self.target),
            tag: self.tag.clone(),
            show_layers: self.show_layers.clone(),
            hide_layers: self.hide_layers.clone(),
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
            show_layers: options.show_layers,
            hide_layers: options.hide_layers,
            sheet: options.sheet,
            sheet_frames: options.sheet_frames,
            fps: options.fps,
            ..TgsOptions::default()
        }
    }

    /// The options that change how pixel art is drawn for ffmpeg for
    /// `format`, the preset's included: an Aseprite file's tag and layers,
    /// and how fast a sequence's frames play, which pictures leave at the
    /// default, as making one does.
    pub fn drawing(&self, format: Format, presets: &Presets, config: &Config) -> Options {
        let shared = || Options {
            tag: self.tag.clone(),
            show_layers: self.show_layers.clone(),
            hide_layers: self.hide_layers.clone(),
            fps: self.webm.fps,
            ..Options::default()
        };
        let options = self.webm_merged(presets, config).unwrap_or_else(|_| shared());
        Options {
            tag: options.tag,
            show_layers: options.show_layers,
            hide_layers: options.hide_layers,
            fps: options.fps.filter(|_| format == Format::Webm),
            ..Options::default()
        }
    }
}

/// `options` without the values of settings that are hidden or greyed
/// out, as `effective`, everything a conversion gets, shows them: they are
/// kept for when they show again, but have no say until then.
pub fn shown(mut options: Options, effective: &Options) -> Options {
    let fit = effective.fit.unwrap_or(Fit::Auto);
    let constant_quality = matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    let lossless = constant_quality && effective.lossless == Some(true);
    if fit == Fit::Off {
        (options.attempts, options.fit_range) = (None, None);
    }
    if !constant_quality {
        (options.crf, options.lossless) = (None, None);
    }
    if fit != Fit::Off || lossless {
        options.bitrate = None;
    }
    if lossless {
        options.crf = None;
    }
    if effective.spoof == Some(Spoof::Never) {
        options.fake_duration = None;
    }
    // a scale of whole multiples, which keeps the whole picture
    if effective.scaling != Some(Scaling::PixelPerfect) {
        options.exact_scale = None;
    } else if effective.exact_scale.is_some()
        && matches!(effective.resize, Some(Resize::Crop | Resize::Stretch))
    {
        options.resize = None;
    }
    options
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
    /// Width to height, as typed.
    Ratio(f64, f64),
}

/// How close a picture is looked at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    /// 1 shows all of the picture.
    pub level: f32,
    /// The picture point in the middle of the view, from 0 to 1.
    pub centre: egui::Vec2,
    /// The view and the part of it the picture fits, as last shown: zoomed
    /// in, the picture stays where it is when only that part changes.
    pub anchor: Option<(egui::Rect, egui::Rect)>,
}

impl Default for Zoom {
    fn default() -> Zoom {
        Zoom { level: 1.0, centre: egui::vec2(0.5, 0.5), anchor: None }
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
    /// The small picture of what the result will look like, beside the
    /// input.
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
    tag: Option<String>,
    layers: (Option<Vec<String>>, Option<Vec<String>>),
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
    frame: Option<f64>,
    webm: Options,
    tgs: TgsOptions,
}

impl Framing {
    pub fn of(item: &Item) -> Framing {
        let (webm, tgs) = (&item.choices.webm, &item.choices.tgs);
        Framing {
            format: item.format,
            target: item.choices.target,
            tag: item.choices.tag.clone(),
            layers: (item.choices.show_layers.clone(), item.choices.hide_layers.clone()),
            crop: item.choices.crop,
            start: item.choices.start,
            length: item.choices.length,
            frame: item.choices.frame,
            webm: Options {
                resize: webm.resize,
                scaling: webm.scaling,
                exact_scale: webm.exact_scale,
                tempo: webm.tempo,
                ..Options::default()
            },
            tgs: TgsOptions {
                keep_canvas: tgs.keep_canvas,
                pixel_scale: tgs.pixel_scale,
                resize: tgs.resize,
                sheet: tgs.sheet.clone(),
                sheet_frames: tgs.sheet_frames,
                fps: tgs.fps,
                tempo: tgs.tempo,
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
    /// The name of the pasted image's copy kept with the session.
    pub kept: Option<String>,
    pub kind: Kind,
    pub format: Format,
    /// The format was not picked by the user yet, so finding pixel art may
    /// change it.
    pub format_guessed: bool,
    pub choices: Choices,
    pub view: View,
    /// The input read by ffmpeg, for WebM and pictures, with how an
    /// Aseprite file was drawn for it.
    pub video: Load<Video>,
    pub video_drawing: Options,
    /// The part of the video that is played: where it starts and how long.
    pub part: Option<((f64, f64), Load<Clip>)>,
    /// The frame shown while paused, at the input's full size, and its
    /// time: sharp enough to crop by.
    pub still: Option<(f64, Load<Clip>)>,
    /// The input read as pixel art, for `.tgs`, with what it was read with.
    pub art: Load<Art>,
    pub art_reading: TgsOptions,
    /// An Aseprite file's tags and layers as last read, shown while it is
    /// read again, so nothing under them moves.
    pub aseprite_seen: Option<(Vec<Tag>, Vec<LayerInfo>, Vec<f64>)>,
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
            kept: None,
            kind,
            format: kind.first_format(),
            format_guessed: kind == Kind::Image,
            choices: Choices::new(target),
            view: View::default(),
            video: Load::Idle,
            video_drawing: Options::default(),
            part: None,
            still: None,
            art: Load::Idle,
            art_reading: TgsOptions::default(),
            aseprite_seen: None,
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

    /// The tags and layers of an Aseprite input, as read for the current
    /// format.
    /// Its tags, layers, and how long each of its frames shows, as read for
    /// the current format.
    pub fn aseprite(&self) -> Option<(&[Tag], &[LayerInfo], &[f64])> {
        if self.kind != Kind::Aseprite {
            return None;
        }
        match self.format {
            Format::Webm | Format::Image => {
                let readable = &self.video.ready()?.readable;
                Some((readable.tags(), readable.layers(), readable.durations()))
            }
            Format::Tgs => {
                let art = self.art.ready()?;
                Some((&art.tags[..], &art.layers[..], &art.durations[..]))
            }
        }
    }

    /// Whether the input is being read for the current format.
    pub fn input_reading(&self) -> bool {
        match self.format {
            Format::Webm | Format::Image => self.video.is_loading(),
            Format::Tgs => self.art.is_loading(),
        }
    }

    /// The input as it is shown for the current format.
    pub fn input_clip(&self) -> Option<&Clip> {
        match self.format {
            Format::Webm | Format::Image => self.video.ready().map(|video| &video.overview),
            Format::Tgs => self.art.ready().map(|art| &art.clip),
        }
    }

    /// Size of the input picture.
    pub fn input_size(&self) -> Option<(u32, u32)> {
        match self.format {
            Format::Webm | Format::Image => {
                self.video.ready().map(|v| (v.probe.width, v.probe.height))
            }
            Format::Tgs => self.art.ready().map(|art| (art.clip.width, art.clip.height)),
        }
    }

    /// Length of the input in seconds, `None` for still images.
    pub fn input_length(&self) -> Option<f64> {
        match self.format {
            Format::Webm | Format::Image => self.video.ready().and_then(|video| {
                (!video.probe.still_image).then_some(video.probe.duration).flatten()
            }),
            Format::Tgs => {
                self.art.ready().filter(|art| art.clip.is_animated()).map(|art| art.clip.end())
            }
        }
    }

    /// Where the input's frames start, which cuts land between: a video's
    /// as its packets time them, or else at its frame rate, and art's as
    /// it times them.
    pub fn input_frames(&self) -> Option<Frames> {
        match self.format {
            Format::Webm | Format::Image => {
                let (video, length) = (self.video.ready()?, self.input_length()?);
                Frames::starting(&video.starts, length, Cut::Video).or_else(|| {
                    let fps = video.probe.fps.or(video.probe.base_fps)?;
                    Frames::even(length, fps, Cut::Video)
                })
            }
            Format::Tgs => {
                let art = self.art.ready().filter(|art| art.clip.is_animated())?;
                Some(Frames { times: art.clip.times.clone(), cut: Cut::Art })
            }
        }
    }

    /// Why the input can't be shown, if reading it failed.
    pub fn input_error(&self) -> Option<&str> {
        let load = match self.format {
            Format::Webm | Format::Image => match &self.video {
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

    /// How many times as fast as the input the result plays: the tempo
    /// set, the item's or its preset's, or for `.tgs` as fast as fits the
    /// used part into 3 seconds.
    pub fn tempo(&self, presets: &Presets, config: &Config) -> f64 {
        let set = match self.format {
            Format::Webm => self.choices.webm_options(presets, config).ok().and_then(|o| o.tempo),
            Format::Tgs => self.choices.tgs_options(presets, config).ok().and_then(|o| o.tempo),
            Format::Image => None,
        };
        set.unwrap_or_else(|| match self.format {
            Format::Webm | Format::Image => 1.0,
            Format::Tgs => {
                let used = self.range().map_or(0.0, |(start, end)| end - start);
                (used / telegram::MAX_SECONDS).max(1.0)
            }
        })
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
        assert_eq!(Kind::of(&path("a.aseprite"), false), Kind::Aseprite);
        assert_eq!(Kind::of(&[dir.path().to_path_buf()], false), Kind::Frames);
        assert_eq!(Kind::of(&[PathBuf::from("1.png"), PathBuf::from("2.png")], true), Kind::Frames);
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
        assert_eq!(options.speed, presets.webm("fast").unwrap().speed);
        // fast fits the bitrate, which hides the quality: it has no say
        assert_eq!((options.crf, options.start), (None, Some(1.0)));
        choices.webm.fit = Some(Fit::Length);
        let options = choices.webm_options(&presets, &config).unwrap();
        assert_eq!(options.crf, Some(20));
        choices.webm.lossless = Some(true);
        let options = choices.webm_options(&presets, &config).unwrap();
        assert_eq!((options.crf, options.lossless), (None, Some(true)));
        // and lossless has no say once fitting hides it again
        choices.webm.fit = None;
        let options = choices.webm_options(&presets, &config).unwrap();
        assert_eq!((options.fit, options.lossless), (Some(Fit::Bitrate), None));
        let tgs = choices.tgs_options(&presets, &config).unwrap();
        assert_eq!((tgs.target, tgs.start), (Some(Target::Emoji), Some(1.0)));
    }
}

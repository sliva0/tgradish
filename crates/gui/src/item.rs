//! Open file item state: input kind, user conversion choices, loaded media,
//! conversion jobs, and preview state.

use std::path::{Path, PathBuf};

use crate::config::Config;
use eframe::egui;
use serde::{Deserialize, Serialize};
use tgradish_core::input::{self, FileKind, Input};
use tgradish_core::options::{Crop, IntegerScale, Resize, Scaling, Selection, Sheet};
use tgradish_core::picture;
use tgradish_core::presets::{Format, FormatOptions, Presets};
use tgradish_core::resolve::{Resolved, resolve};
use tgradish_core::telegram::Target;
use tgradish_core::tgradish_tgs::frames::aseprite::{LayerInfo, Tag};
use tgradish_core::webm::Options;
use tgradish_core::{tgs, webp};

use crate::cuts::{self, Frames};
use crate::jobs::{Done, Job};
use crate::l10n::fl;
use crate::media::{Art, Clip, Load, Video};

/// What kind of input an item is, which limits what it can become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
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
    /// What `inputs` are taken for until they are read: a single file
    /// by its name, which [`Kind::of_file`] then checks by its content.
    pub(crate) fn of(inputs: &[PathBuf], sequence: bool) -> Kind {
        match inputs {
            [input] if !sequence && !input.is_dir() => Kind::from(FileKind::of_name(input)),
            _ => Kind::Frames,
        }
    }

    /// What the single file at `path` is by its content, as core tells
    /// it; this reads the file.
    pub(crate) fn of_file(path: &Path) -> Kind {
        Kind::from(input::sniff(path))
    }

    /// The format to start with; images switch to `.tgs` once they turn
    /// out to be pixel art.
    pub(crate) fn first_format(self) -> Format {
        match self {
            Kind::Aseprite | Kind::Frames => Format::Tgs,
            Kind::Video | Kind::Image => Format::Webm,
        }
    }
}

impl From<FileKind> for Kind {
    fn from(kind: FileKind) -> Kind {
        match kind {
            FileKind::Aseprite => Kind::Aseprite,
            FileKind::Webp { .. } | FileKind::Picture => Kind::Image,
            // WebM, .tgs and the rest, which only ffmpeg reads
            _ => Kind::Video,
        }
    }
}

/// What the user chose for an item: a preset, then their own values over
/// it. Target, crop, times, title and what of an Aseprite file or a sprite
/// sheet is read are the file's own, and go to every format that takes
/// them. Kept with the session as [`SavedChoices`] has them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "SavedChoices", into = "SavedChoices")]
pub(crate) struct Choices {
    /// `None` for the default preset of the format.
    pub(crate) preset: Option<String>,
    pub(crate) target: Target,
    /// An Aseprite tag, and layers shown and hidden over what the file
    /// shows.
    pub(crate) tag: Option<String>,
    pub(crate) show_layers: Option<Vec<String>>,
    pub(crate) hide_layers: Option<Vec<String>>,
    pub(crate) crop: Option<Crop>,
    pub(crate) start: Option<f64>,
    pub(crate) length: Option<f64>,
    /// Seconds into the input of the frame a static sticker is made of.
    pub(crate) frame: Option<f64>,
    pub(crate) title: Option<String>,
    /// The sprite sheet a `.tgs` reads its frames from, and how many of
    /// them; kept while the sheet is off, for when it is on again.
    pub(crate) sheet: Option<Sheet>,
    pub(crate) sheet_frames: Option<u32>,
    /// Values set over the preset's, for WebM and `.tgs` results; images
    /// take WebM's.
    pub(crate) webm: Options,
    pub(crate) tgs: tgs::Options,
}

/// [`Choices`] as the session file holds them, as version 1 wrote them:
/// the title, the sheet and its frames within the options of a format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SavedChoices {
    preset: Option<String>,
    target: Target,
    tag: Option<String>,
    show_layers: Option<Vec<String>>,
    hide_layers: Option<Vec<String>>,
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
    frame: Option<f64>,
    webm: Options,
    tgs: tgs::Options,
}

impl From<SavedChoices> for Choices {
    fn from(saved: SavedChoices) -> Choices {
        let SavedChoices { webm, tgs, .. } = saved;
        // one title, which version 1 kept for each format, though it set
        // neither
        let title = webm.title.clone().or(tgs.title.clone());
        // the file's own the GUI never wrote there are read and left out:
        // its own are the item's
        let webm = Options {
            target: None,
            tag: None,
            show_layers: None,
            hide_layers: None,
            crop: None,
            start: None,
            length: None,
            title: None,
            ..webm
        };
        Choices {
            preset: saved.preset,
            target: saved.target,
            tag: saved.tag,
            show_layers: saved.show_layers,
            hide_layers: saved.hide_layers,
            crop: saved.crop,
            start: saved.start,
            length: saved.length,
            frame: saved.frame,
            title,
            sheet: tgs.sheet,
            sheet_frames: tgs.sheet_frames,
            webm,
            tgs: tgs::Options {
                target: None,
                start: None,
                length: None,
                crop: None,
                tag: None,
                show_layers: None,
                hide_layers: None,
                sheet: None,
                sheet_frames: None,
                title: None,
                ..tgs
            },
        }
    }
}

impl From<Choices> for SavedChoices {
    fn from(choices: Choices) -> SavedChoices {
        let title = choices.title;
        SavedChoices {
            preset: choices.preset,
            target: choices.target,
            tag: choices.tag,
            show_layers: choices.show_layers,
            hide_layers: choices.hide_layers,
            crop: choices.crop,
            start: choices.start,
            length: choices.length,
            frame: choices.frame,
            // to both, as either format reads it
            webm: Options { title: title.clone(), ..choices.webm },
            tgs: tgs::Options {
                title,
                sheet: choices.sheet,
                sheet_frames: choices.sheet_frames,
                ..choices.tgs
            },
        }
    }
}

impl Choices {
    pub(crate) fn new(target: Target) -> Choices {
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
            title: None,
            sheet: None,
            sheet_frames: None,
            webm: Options::default(),
            tgs: tgs::Options::default(),
        }
    }

    pub(crate) fn preset_name<'a>(&'a self, format: Format, config: &'a Config) -> &'a str {
        self.preset.as_deref().unwrap_or_else(|| config.preset_for(format))
    }

    /// The preset's WebM options, which values not set fall back to.
    pub(crate) fn webm_base(&self, presets: &Presets, config: &Config) -> Result<Options, String> {
        presets.webm(self.preset_name(Format::Webm, config)).map_err(|err| err.to_string())
    }

    pub(crate) fn tgs_base(
        &self,
        presets: &Presets,
        config: &Config,
    ) -> Result<tgs::Options, String> {
        presets.tgs(self.preset_name(Format::Tgs, config)).map_err(|err| err.to_string())
    }

    /// What the item chose of its file for `format`. Its target is always
    /// set, so recipes saved with one stay equal.
    pub(crate) fn selection(&self, format: Format) -> Selection {
        Selection {
            target: Some(self.target),
            tag: self.tag.clone(),
            show_layers: self.show_layers.clone(),
            hide_layers: self.hide_layers.clone(),
            crop: self.crop,
            // a picture is of its frame
            start: if format == Format::Webp { self.frame } else { self.start },
            length: self.length,
            title: self.title.clone(),
            sheet: self.sheet,
            // kept for when the sheet is on again, and of none till then
            sheet_frames: self.sheet_frames.filter(|_| self.sheet.is_some()),
        }
        .for_format(format)
    }

    /// The item's values for `format`, as a layer over its preset's: its
    /// drafts, what it chose of its file, and the GUI's own rules for an
    /// integer scale, as values.
    pub(crate) fn layer(
        &self,
        format: Format,
        presets: &Presets,
        config: &Config,
    ) -> FormatOptions {
        let options = match format {
            Format::Webm => FormatOptions::Webm(self.webm_layer(presets, config)),
            Format::Webp => FormatOptions::Webp((&self.webm_layer(presets, config)).into()),
            Format::Tgs => FormatOptions::Tgs(self.tgs.clone()),
        };
        self.selection(format).into_options(options)
    }

    /// The item's options for WebM, as [`Choices::layer`] says, which a
    /// picture's are made of.
    pub(crate) fn webm_layer(&self, presets: &Presets, config: &Config) -> Options {
        let mut layer = self.webm.clone();
        // the GUI scales by an integer scale only under integer scaling,
        // which then keeps the whole picture, where core would scale by it
        // whatever the scaling, and refuse it with a crop or a stretch: the
        // preset's values that would are cleared, and the item's left out
        let base = self.webm_base(presets, config).unwrap_or_default();
        let effective = base.clone().merged(&layer);
        if effective.scaling != Some(Scaling::Integer) {
            layer.integer_scale = base.integer_scale().map(|_| IntegerScale::NONE);
        } else if effective.integer_scale.is_some()
            && matches!(effective.resize, Some(Resize::Crop | Resize::Stretch))
        {
            layer.resize = base.resize.map(|_| picture::default_resize(self.target));
        }
        layer
    }

    /// What a conversion to `format` gets: the item's layer over its
    /// preset, checked, with what has no say set aside.
    pub(crate) fn resolved(
        &self,
        format: Format,
        presets: &Presets,
        config: &Config,
    ) -> Result<Resolved, String> {
        let layer = [self.layer(format, presets, config)];
        resolve(presets, config, format, self.preset.as_deref(), &layer)
            .map_err(|err| err.to_string())
    }

    /// Everything a conversion to `format` gets.
    pub(crate) fn recipe(
        &self,
        format: Format,
        presets: &Presets,
        config: &Config,
    ) -> Result<Recipe, String> {
        Ok(self.resolved(format, presets, config)?.options.into())
    }

    /// The options that change what pixel art is read as: the item's
    /// tag, layers and sheet, which are of its file alone, and the rate
    /// its frames play at, the preset's included.
    pub(crate) fn reading(&self, presets: &Presets, config: &Config) -> tgs::Options {
        // a broken preset still leaves the item's own
        let fps = self.tgs.fps.or_else(|| self.tgs_base(presets, config).ok()?.fps);
        tgs::Options {
            tag: self.tag.clone(),
            show_layers: self.show_layers.clone(),
            hide_layers: self.hide_layers.clone(),
            sheet: self.sheet,
            sheet_frames: self.sheet_frames,
            fps,
            ..tgs::Options::default()
        }
    }

    /// The options that change how pixel art is drawn for ffmpeg for
    /// `format`: the item's tag and layers, which are of its file alone,
    /// and how fast a sequence's frames play, the preset's included, which
    /// pictures leave at the default, as making one does.
    pub(crate) fn drawing(&self, format: Format, presets: &Presets, config: &Config) -> Options {
        // a broken preset still leaves the item's own
        let fps = self.webm.fps.or_else(|| self.webm_base(presets, config).ok()?.fps);
        Options {
            tag: self.tag.clone(),
            show_layers: self.show_layers.clone(),
            hide_layers: self.hide_layers.clone(),
            fps: fps.filter(|_| format == Format::Webm),
            ..Options::default()
        }
    }
}

/// Everything a conversion gets, for the format it makes, with the preset
/// in force then: two conversions with the same make the same result,
/// whatever else of the choices differs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Recipe {
    Webm(Options),
    Tgs(tgs::Options),
    Webp(webp::Options),
}

impl Recipe {
    pub(crate) fn format(&self) -> Format {
        match self {
            Recipe::Webm(_) => Format::Webm,
            Recipe::Tgs(_) => Format::Tgs,
            Recipe::Webp(_) => Format::Webp,
        }
    }

    /// What it makes: a sticker or an emoji.
    pub(crate) fn target(&self) -> Target {
        match self {
            Recipe::Webm(options) => options.target,
            Recipe::Tgs(options) => options.target,
            Recipe::Webp(options) => options.target,
        }
        .unwrap_or_default()
    }

    /// Its options as WebM's, which a picture's are some of; none of a
    /// `.tgs`'s.
    pub(crate) fn into_webm(self) -> Options {
        match self {
            Recipe::Webm(options) => options,
            Recipe::Webp(options) => options.into(),
            Recipe::Tgs(_) => Options::default(),
        }
    }

    /// Its options as `.tgs`'s; none of another format's.
    pub(crate) fn into_tgs(self) -> tgs::Options {
        match self {
            Recipe::Tgs(options) => options,
            _ => tgs::Options::default(),
        }
    }
}

impl From<FormatOptions> for Recipe {
    fn from(options: FormatOptions) -> Recipe {
        match options {
            FormatOptions::Webm(options) => Recipe::Webm(options),
            FormatOptions::Tgs(options) => Recipe::Tgs(options),
            FormatOptions::Webp(options) => Recipe::Webp(options),
        }
    }
}

/// An Aseprite file's tags, layers and frame durations.
pub(crate) type AsepriteSeen = (Vec<Tag>, Vec<LayerInfo>, Vec<f64>);

/// Which picture the preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Show {
    Input,
    Result,
}

/// Shapes the crop is kept to while dragging it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Aspect {
    Free,
    /// The input's own.
    Input,
    /// Width to height, as typed.
    Ratio(f64, f64),
}

/// How close a picture is looked at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Zoom {
    /// 1 shows all of the picture.
    pub(crate) level: f32,
    /// The picture point in the middle of the view, from 0 to 1.
    pub(crate) centre: egui::Vec2,
    /// The view and the part of it the picture fits, as last shown: zoomed
    /// in, the picture stays where it is when only that part changes.
    pub(crate) anchor: Option<(egui::Rect, egui::Rect)>,
    /// How many of the screen's pixels a pixel of the picture took, as it
    /// was last shown.
    pub(crate) scale: Option<f32>,
    /// Of pixel art, how many of the screen's pixels its pixels take for
    /// each point an input pixel takes, as it was last shown: where it
    /// fits, they take a whole number of them.
    pub(crate) grid: Option<f32>,
    /// Zooming out to all of it, from this level and centre, since this
    /// time, while it eases.
    pub(crate) easing: Option<(f32, egui::Vec2, f64)>,
}

impl Default for Zoom {
    fn default() -> Zoom {
        let centre = egui::vec2(0.5, 0.5);
        Zoom { level: 1.0, centre, anchor: None, scale: None, grid: None, easing: None }
    }
}

impl Zoom {
    /// Zooms out to all of the picture: eased from `now`, where `smooth`,
    /// as with motion, or at once.
    pub(crate) fn zoom_out(&mut self, now: f64, smooth: bool) {
        match smooth {
            true => self.easing = Some((self.level, self.centre, now)),
            false => (self.level, self.centre, self.easing) = (1.0, egui::vec2(0.5, 0.5), None),
        }
    }
}

/// How the preview of an item is looked at: not part of the conversion.
#[derive(Debug, Clone)]
pub(crate) struct View {
    pub(crate) show: Show,
    pub(crate) aspect: Aspect,
    /// Seconds into the input.
    pub(crate) time: f64,
    /// Seconds into the result.
    pub(crate) result_time: f64,
    pub(crate) playing: bool,
    pub(crate) input_zoom: Zoom,
    pub(crate) result_zoom: Zoom,
    /// The small picture of what the result will look like, beside the
    /// input.
    pub(crate) inset: bool,
    /// What the framing was when last shown, to show the input when it
    /// changes.
    pub(crate) framing: Option<Framing>,
}

impl View {
    /// The zoom of the picture `shown`.
    pub(crate) fn zoom_mut(&mut self, shown: Show) -> &mut Zoom {
        match shown {
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
/// fitted: changing them is something to look at on the input, so the
/// preview turns back to it. A new setting of that kind belongs here, or
/// changing it leaves the old result in view.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Framing {
    format: Format,
    target: Target,
    tag: Option<String>,
    layers: (Option<Vec<String>>, Option<Vec<String>>),
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
    frame: Option<f64>,
    sheet: (Option<Sheet>, Option<u32>),
    webm: Options,
    tgs: tgs::Options,
}

impl Framing {
    pub(crate) fn of(item: &Item) -> Framing {
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
            sheet: (item.choices.sheet, item.choices.sheet_frames),
            webm: Options {
                resize: webm.resize,
                scaling: webm.scaling,
                integer_scale: webm.integer_scale,
                tempo: webm.tempo,
                // the rate a sequence's pictures are read at, as .tgs's is,
                // which draws its frames again; a video's is only the rate
                // it is encoded at
                fps: webm.fps.filter(|_| item.kind == Kind::Frames && item.format == Format::Webm),
                ..Options::default()
            },
            tgs: tgs::Options {
                keep_canvas: tgs.keep_canvas,
                pixel_scale: tgs.pixel_scale,
                resize: tgs.resize,
                fps: tgs.fps,
                tempo: tgs.tempo,
                ..tgs::Options::default()
            },
        }
    }
}

/// What reading an item's input found that its choices must agree with,
/// kept once the frames read are let go of.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Facts {
    /// Read for a `.tgs`, as pixel art, rather than for ffmpeg.
    art: bool,
    /// The picture's size: a sprite sheet's cell's, where it was read as
    /// one.
    size: (u32, u32),
    /// An Aseprite file's tags and layers, by name.
    tags: Vec<String>,
    layers: Vec<String>,
    /// Read as one picture, as the sprite sheet chosen doesn't split it:
    /// then `size` is all of it.
    unsplit: bool,
}

impl Facts {
    /// Whether it was read for a `.tgs`, as pixel art.
    pub(crate) fn art(&self) -> bool {
        self.art
    }

    /// What the input as read for the item's format says, once it is.
    fn of(item: &Item) -> Option<Facts> {
        let size = item.input_size()?;
        let (tags, layers) = match item.aseprite() {
            Some((tags, layers, _)) => (
                tags.iter().map(|tag| tag.name.clone()).collect(),
                layers.iter().map(|layer| layer.name.clone()).collect(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        let (art, unsplit) = match item.format {
            Format::Tgs => {
                let art = item.art.ready()?;
                let sheet = item.art_reading.sheet.is_some();
                (true, sheet && !art.as_chosen && art.sheet_refused.is_none())
            }
            Format::Webm | Format::Webp => (false, false),
        };
        Some(Facts { art, size, tags, layers, unsplit })
    }
}

/// What keeps an item from being converted as it is: its choices ask for
/// what its input hasn't, or for ffmpeg, which isn't there. Each has a way
/// out where it is chosen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Blocker {
    /// The crop, kept from another picture, is outside this one.
    CropOutside,
    /// The sprite sheet's cells don't divide its picture, `width` by
    /// `height`.
    SheetUneven { width: u32, height: u32, columns: u32, rows: u32 },
    /// An Aseprite file without the tag chosen.
    TagMissing(String),
    /// An Aseprite file without the layers shown or hidden.
    LayerMissing(Vec<String>),
    /// ffmpeg reads the input or makes the result, and isn't there.
    NeedsFfmpeg,
}

impl Blocker {
    /// What it is, as the rest of "Can't convert yet: " says it.
    pub(crate) fn reason(&self) -> String {
        match self {
            Blocker::CropOutside => fl!("blocked-crop"),
            Blocker::SheetUneven { .. } => fl!("blocked-sheet"),
            Blocker::TagMissing(tag) => fl!("blocked-tag", tag = tag.as_str()),
            Blocker::LayerMissing(layers) => {
                fl!("blocked-layers", count = layers.len(), layers = quoted(layers))
            }
            Blocker::NeedsFfmpeg => fl!("blocked-ffmpeg"),
        }
    }
}

/// Names each in quotes, apart by commas: "sketch", "shadow".
pub(crate) fn quoted(names: &[String]) -> String {
    names.iter().map(|name| format!("\"{name}\"")).collect::<Vec<_>>().join(", ")
}

/// The layout of a sprite sheet nearest `columns` by `rows` whose cells
/// divide a `width` by `height` picture.
pub(crate) fn dividing_sheet(width: u32, height: u32, columns: u32, rows: u32) -> Sheet {
    let nearest = |side: u32, cells: u32| {
        (1..=side.clamp(1, 256))
            .filter(|count| side.is_multiple_of(*count))
            // the more cells of two as near
            .min_by_key(|count| (count.abs_diff(cells), std::cmp::Reverse(*count)))
            .unwrap_or(1)
    };
    Sheet { columns: nearest(width, columns), rows: nearest(height, rows) }
}

/// What the blockers of an item were worked out from, so they are worked
/// out again only when it changes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Blocking {
    format: Format,
    kind: Kind,
    crop: Option<Crop>,
    tag: Option<String>,
    layers: (Option<Vec<String>>, Option<Vec<String>>),
    sheet: Option<Sheet>,
    facts: Option<Facts>,
    ffmpeg: bool,
}

/// A conversion of an item: the job and what it is made from. As
/// [`Item::job`] it waits or runs; as [`Item::made`] its status is always
/// [`crate::jobs::Status::Done`]; as [`Item::failed`], never.
pub(crate) struct Conversion {
    pub(crate) job: Job,
    pub(crate) format: Format,
    /// The choices it is made with, which come to its recipe as it starts;
    /// `None` for the result of an earlier run, which only its recipe
    /// tells.
    pub(crate) choices: Option<Choices>,
    /// What the choices came to as it started; `None` before, or where
    /// they couldn't be made into one.
    pub(crate) recipe: Option<Recipe>,
}

impl Conversion {
    pub(crate) fn of(job: Job, format: Format, choices: Choices) -> Conversion {
        Conversion { job, format, choices: Some(choices), recipe: None }
    }

    /// The result of an earlier run, made with `recipe`.
    pub(crate) fn earlier(job: Job, recipe: Recipe) -> Conversion {
        Conversion { job, format: recipe.format(), choices: None, recipe: Some(recipe) }
    }
}

pub(crate) struct Item {
    pub(crate) id: u64,
    /// One file, or the frames of one animation.
    pub(crate) inputs: Vec<PathBuf>,
    pub(crate) sequence: bool,
    /// The folder of its own a pasted image is in, which goes with the item.
    pub(crate) paste_dir: Option<tempfile::TempDir>,
    /// The name of the pasted image's copy kept with the session.
    pub(crate) pasted_copy: Option<String>,
    /// What the input is, by the name of a single file until
    /// [`Item::sniffed`] says.
    pub(crate) kind: Kind,
    /// What a single file is by its content, read on a thread before the
    /// file itself is, as what reads it depends on it.
    pub(crate) sniffed: Load<Kind>,
    pub(crate) format: Format,
    /// The format was not picked by the user yet, so what the file turns
    /// out to be, and for an image whether it is pixel art, may change it.
    pub(crate) format_guessed: bool,
    pub(crate) choices: Choices,
    pub(crate) view: View,
    /// The input read by ffmpeg, for WebM and pictures, with how an
    /// Aseprite file was drawn for it.
    pub(crate) video: Load<Video>,
    pub(crate) video_drawing: Options,
    /// The part of the video that is played: where it starts and how long.
    pub(crate) part: Option<((f64, f64), Load<Clip>)>,
    /// The frame shown while paused, at the input's full size, and its
    /// time: sharp enough to crop by.
    pub(crate) still: Option<(f64, Load<Clip>)>,
    /// The input read as pixel art, for `.tgs`, with what it was read with.
    pub(crate) art: Load<Art>,
    pub(crate) art_reading: tgs::Options,
    /// An Aseprite file's tags and layers as last read, shown while it is
    /// read again, so nothing under them moves.
    pub(crate) aseprite_seen: Option<std::rc::Rc<AsepriteSeen>>,
    pub(crate) input_thumb: Option<egui::TextureHandle>,
    /// The conversion waiting or running.
    pub(crate) job: Option<Conversion>,
    /// The last conversion that made a sticker.
    pub(crate) made: Option<Conversion>,
    /// The last result of an earlier run, read again before it counts as
    /// made: what it is, as it may have changed, or whether it is there.
    pub(crate) earlier: Option<(Conversion, Load<Done>)>,
    /// The last conversion, if it failed, was stopped or found a file in
    /// the way: newer than `made`.
    pub(crate) failed: Option<Conversion>,
    /// The result's frames: a WebM's decoded, a picture's read, or a
    /// `.tgs`'s as drawn when it was made.
    pub(crate) result_clip: Load<Clip>,
    pub(crate) result_thumb: Option<egui::TextureHandle>,
    /// What converting would make now, worked out each frame, as presets
    /// and the default ones may change too; `None` where it can't.
    pub(crate) recipe: Option<Recipe>,
    /// What reading the input last found.
    pub(crate) facts: Option<Facts>,
    /// What keeps it from being converted, worked out from `blocking`.
    pub(crate) blockers: Vec<Blocker>,
    pub(crate) blocking: Option<Blocking>,
    /// Its settings as they were, for undoing changes to them.
    pub(crate) history: crate::history::History,
}

impl Item {
    pub(crate) fn new(id: u64, inputs: Vec<PathBuf>, sequence: bool, target: Target) -> Item {
        let kind = Kind::of(&inputs, sequence);
        Item {
            id,
            inputs,
            sequence,
            paste_dir: None,
            pasted_copy: None,
            kind,
            // folders and sequences are what they are named
            sniffed: if kind == Kind::Frames { Load::Ready(kind) } else { Load::Idle },
            format: kind.first_format(),
            format_guessed: kind != Kind::Frames,
            choices: Choices::new(target),
            view: View::default(),
            video: Load::Idle,
            video_drawing: Options::default(),
            part: None,
            still: None,
            art: Load::Idle,
            art_reading: tgs::Options::default(),
            aseprite_seen: None,
            input_thumb: None,
            job: None,
            made: None,
            earlier: None,
            failed: None,
            result_clip: Load::Idle,
            result_thumb: None,
            recipe: None,
            facts: None,
            blockers: Vec::new(),
            blocking: None,
            history: Default::default(),
        }
    }

    /// What converting the item converts: its file, or its files and
    /// folders as one animation.
    /// Takes in what the input turned out to be by its content, and the
    /// format that goes with that, unless one was picked.
    pub(crate) fn take_kind(&mut self, kind: Kind) {
        self.kind = kind;
        if self.format_guessed {
            self.format = kind.first_format();
            self.format_guessed = kind == Kind::Image;
            self.history.follow_guess(self.format, self.format_guessed);
        }
    }

    pub(crate) fn input(&self) -> Input {
        match &self.inputs[..] {
            [input] if !self.sequence => Input::File(input.clone()),
            inputs => Input::Sequence(inputs.to_vec()),
        }
    }

    pub(crate) fn name(&self) -> String {
        let first = self.inputs.first().and_then(|p| p.file_name());
        let first = first.map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match (self.inputs.len(), &self.paste_dir) {
            (_, Some(_)) => fl!("name-pasted"),
            (1, None) if self.sequence => fl!("name-frames", name = first),
            (1, None) => first,
            (n, None) => {
                let more = n - 1;
                fl!("name-and-more", name = first, more = more)
            }
        }
    }

    /// An Aseprite input's tags, layers, and how long each of its frames
    /// shows, as read for the current format.
    pub(crate) fn aseprite(&self) -> Option<(&[Tag], &[LayerInfo], &[f64])> {
        if self.kind != Kind::Aseprite {
            return None;
        }
        match self.format {
            Format::Webm | Format::Webp => {
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
    pub(crate) fn input_reading(&self) -> bool {
        match self.format {
            Format::Webm | Format::Webp => self.video.is_loading(),
            Format::Tgs => self.art.is_loading(),
        }
    }

    /// The input as it is shown for the current format.
    pub(crate) fn input_clip(&self) -> Option<&Clip> {
        match self.format {
            Format::Webm | Format::Webp => self.video.ready().map(|video| &video.overview),
            Format::Tgs => self.art.ready().map(|art| &art.clip),
        }
    }

    /// Size of the input picture.
    pub(crate) fn input_size(&self) -> Option<(u32, u32)> {
        match self.format {
            Format::Webm | Format::Webp => {
                self.video.ready().map(|v| (v.probe.width, v.probe.height))
            }
            Format::Tgs => self.art.ready().map(|art| (art.clip.width, art.clip.height)),
        }
    }

    /// Length of the input in seconds, `None` for still images.
    pub(crate) fn input_length(&self) -> Option<f64> {
        match self.format {
            Format::Webm | Format::Webp => self.video.ready().and_then(|video| {
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
    pub(crate) fn input_frames(&self) -> Option<std::sync::Arc<Frames>> {
        match self.format {
            Format::Webm | Format::Webp => self.video.ready()?.frames.clone(),
            Format::Tgs => self.art.ready()?.frames.clone(),
        }
    }

    /// Why the input can't be shown, if reading it failed.
    pub(crate) fn input_error(&self) -> Option<&str> {
        let load = match self.format {
            Format::Webm | Format::Webp => match &self.video {
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
    pub(crate) fn range(&self) -> Option<(f64, f64)> {
        let length = self.input_length()?;
        Some(cuts::part(length, self.choices.start, self.choices.length))
    }

    /// How many times as fast as the input the result plays: the tempo
    /// set, the item's or its preset's, or for `.tgs` as fast as fits the
    /// used part into 3 seconds.
    pub(crate) fn tempo(&self, presets: &Presets, config: &Config) -> f64 {
        // a broken preset still leaves the defaults
        let recipe = |format| self.choices.recipe(format, presets, config).ok();
        match self.format {
            Format::Webm => {
                recipe(Format::Webm).map(Recipe::into_webm).unwrap_or_default().tempo_or_default()
            }
            Format::Tgs => {
                let used = self.range().map(|(start, end)| end - start);
                recipe(Format::Tgs).map(Recipe::into_tgs).unwrap_or_default().tempo_or_default(used)
            }
            Format::Webp => 1.0,
        }
    }

    /// Whether the last sticker is what converting would make now.
    pub(crate) fn result_is_current(&self) -> bool {
        self.made.as_ref().is_some_and(|made| made.recipe.is_some() && made.recipe == self.recipe)
    }

    /// The last result, or the one of an earlier run while that is read.
    pub(crate) fn last_result(&self) -> Option<&Conversion> {
        self.made.as_ref().or(self.earlier.as_ref().map(|(earlier, _)| earlier))
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    /// Whether ffmpeg reads its input or makes its result: every WebM and
    /// picture, and a `.tgs` of a video.
    pub(crate) fn needs_ffmpeg(&self) -> bool {
        self.format != Format::Tgs || self.kind == Kind::Video
    }

    /// Whether something keeps it from being converted as it is.
    pub(crate) fn blocked(&self) -> bool {
        !self.blockers.is_empty()
    }

    /// Takes in what reading its input found, once it is read.
    pub(crate) fn take_facts(&mut self) {
        if let Some(facts) = Facts::of(self) {
            self.facts = Some(facts);
        }
    }

    /// Works out what keeps it from being converted, where its choices,
    /// what its input was found to be, or whether there is ffmpeg, as
    /// `ffmpeg` says, changed.
    pub(crate) fn find_blockers(&mut self, ffmpeg: bool) {
        let choices = &self.choices;
        // what was found of the input as it is read for this format
        let art = self.format == Format::Tgs;
        let facts = self.facts.clone().filter(|facts| facts.art == art);
        let blocking = Blocking {
            format: self.format,
            kind: self.kind,
            crop: choices.crop,
            tag: choices.tag.clone(),
            layers: (choices.show_layers.clone(), choices.hide_layers.clone()),
            sheet: choices.sheet.filter(|_| art),
            facts,
            ffmpeg,
        };
        if self.blocking.as_ref() == Some(&blocking) {
            return;
        }
        let mut blockers = Vec::new();
        if let Some(facts) = &blocking.facts {
            if let (Some(sheet), true) = (blocking.sheet, facts.unsplit) {
                let (width, height) = facts.size;
                if !width.is_multiple_of(sheet.columns) || !height.is_multiple_of(sheet.rows) {
                    let (columns, rows) = (sheet.columns, sheet.rows);
                    blockers.push(Blocker::SheetUneven { width, height, columns, rows });
                }
            }
            // of a cell, which can't be told while the sheet doesn't split
            let unsplit = blocking.sheet.is_some() && facts.unsplit;
            if let Some(crop) = blocking.crop
                && !unsplit
                && !crop.fits(facts.size.0, facts.size.1)
            {
                blockers.push(Blocker::CropOutside);
            }
            if self.kind == Kind::Aseprite {
                if let Some(tag) = &blocking.tag
                    && !facts.tags.contains(tag)
                {
                    blockers.push(Blocker::TagMissing(tag.clone()));
                }
                let (show, hide) = &blocking.layers;
                let mut missing: Vec<String> = show
                    .iter()
                    .chain(hide)
                    .flatten()
                    .filter(|name| !facts.layers.contains(name))
                    .cloned()
                    .collect();
                missing.dedup();
                if !missing.is_empty() {
                    blockers.push(Blocker::LayerMissing(missing));
                }
            }
        }
        if !ffmpeg && self.needs_ffmpeg() {
            blockers.push(Blocker::NeedsFfmpeg);
        }
        self.blockers = blockers;
        self.blocking = Some(blocking);
    }

    /// Which picture the preview shows: the one chosen, though the input
    /// while the result chosen is still being read, as a WebM's is once
    /// made, which then shows as it is there.
    pub(crate) fn shown(&self) -> Show {
        match self.view.show {
            Show::Result if self.result_clip.ready().is_some() => Show::Result,
            _ => Show::Input,
        }
    }
}

#[cfg(test)]
mod tests {
    use tgradish_core::options::Fit;

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
    fn says_of_every_option_whether_it_frames_the_input() {
        use serde_json::{Value, json};
        // what the choices hold apart from the options, which Framing
        // takes from them, and the item's preset, whose change shows on
        // the result
        let Choices {
            preset: _,
            target: _,
            tag: _,
            show_layers: _,
            hide_layers: _,
            crop: _,
            start: _,
            length: _,
            frame: _,
            title: _,
            sheet: _,
            sheet_frames: _,
            webm: _,
            tgs: _,
        } = Choices::new(Target::Sticker);
        // of those the file's own, all but its title
        let shared = [
            "target",
            "tag",
            "show-layers",
            "hide-layers",
            "crop",
            "start",
            "length",
            "sheet",
            "sheet-frames",
        ];
        let mut file: Vec<String> = Selection::keys().collect();
        file.sort();
        let mut decided_file: Vec<&str> = shared.iter().copied().chain(["title"]).collect();
        decided_file.sort();
        assert_eq!(file, decided_file, "one file's options");
        // of the rest, those that change what of the input is used or how
        // it fits, with a value other than their default, and those that
        // don't: an option core adds goes in one or the other
        let webm_framing = json!({
            "resize": "pad",
            "scaling": "sharp",
            "integer-scale": "2",
            "tempo": 2.0,
        });
        let webm_other = [
            "fit",
            "attempts",
            "fit-range",
            "bitrate",
            "crf",
            "effort",
            "lossless",
            "spoof",
            "fake-duration",
            "watermark",
            "encoder-options",
            "ffmpeg-args",
        ];
        // the rate a video is encoded at, and the rate a sequence's
        // pictures are read at, which redraws them
        let webm_sequence_framing = ["fps"];
        let tgs_framing = json!({
            "keep-canvas": true,
            "pixel-scale": 2,
            "resize": "pad",
            "fps": 5.0,
            "tempo": 2.0,
        });
        let tgs_other = ["effort", "exact", "reductions", "compromise", "watermark"];
        let keys = |format: &str| -> Vec<String> {
            let mut keys: Vec<String> = match format {
                "webm" => tgradish_core::webm::Options::keys().collect(),
                _ => tgradish_core::tgs::Options::keys().collect(),
            };
            keys.retain(|key| !Selection::keys().any(|file| file == *key));
            keys.sort();
            keys
        };
        let decided = |framing: &Value, other: &[&str]| -> Vec<String> {
            let framing = framing.as_object().unwrap().keys().map(String::as_str);
            let mut keys: Vec<String> =
                other.iter().copied().chain(framing).map(str::to_owned).collect();
            keys.sort();
            keys
        };
        let webm_other = [&webm_other[..], &webm_sequence_framing].concat();
        assert_eq!(keys("webm"), decided(&webm_framing, &webm_other), "WebM's options");
        assert_eq!(keys("tgs"), decided(&tgs_framing, &tgs_other), ".tgs's options");
        // and Framing takes those that frame it, each on its own
        let item = || Item::new(1, vec!["a.mp4".into()], false, Target::Sticker);
        let unchanged = Framing::of(&item());
        for (key, value) in webm_framing.as_object().unwrap() {
            let mut changed = item();
            changed.choices.webm = serde_json::from_value(json!({ key: value })).unwrap();
            assert_ne!(Framing::of(&changed), unchanged, "WebM's {key}");
        }
        for (key, value) in tgs_framing.as_object().unwrap() {
            let mut changed = item();
            changed.choices.tgs = serde_json::from_value(json!({ key: value })).unwrap();
            assert_ne!(Framing::of(&changed), unchanged, ".tgs's {key}");
        }
        let mut sheet = item();
        sheet.choices.sheet = "2x2".parse().ok();
        let mut frames = item();
        frames.choices.sheet_frames = Some(3);
        for changed in [sheet, frames] {
            assert_ne!(Framing::of(&changed), unchanged, "{:?}", changed.choices);
        }
        let sequence = || {
            let mut item = Item::new(1, vec!["walk".into()], true, Target::Sticker);
            item.format = Format::Webm;
            item
        };
        for key in webm_sequence_framing {
            let mut video = item();
            video.choices.webm = serde_json::from_value(json!({ key: 5.0 })).unwrap();
            assert_eq!(Framing::of(&video), unchanged, "a video's WebM {key}");
            let mut changed = sequence();
            changed.choices.webm = video.choices.webm.clone();
            assert_ne!(Framing::of(&changed), Framing::of(&sequence()), "a sequence's WebM {key}");
        }
    }

    /// As a session holds them: one title, read from either format's
    /// options, WebM's first, and written to both; the sheet in `.tgs`'s;
    /// and the rest of the file's own there, which the GUI never wrote,
    /// read and left out.
    #[test]
    fn keeps_choices_as_a_session_holds_them() {
        use serde_json::json;
        let saved = |webm: serde_json::Value, tgs: serde_json::Value| {
            let choices = json!({
                "preset": null,
                "target": "sticker",
                "tag": null,
                "show-layers": null,
                "hide-layers": null,
                "crop": null,
                "start": null,
                "length": null,
                "frame": null,
                "webm": webm,
                "tgs": tgs,
            });
            serde_json::from_value::<Choices>(choices).unwrap()
        };
        let crop = json!({ "x": 0, "y": 0, "width": 8, "height": 8 });
        let tgs = json!({ "title": "pig", "sheet": "4x1", "sheet-frames": 3, "tag": "idle" });
        let choices = saved(json!({ "crf": 20, "crop": crop, "start": 1.0 }), tgs);
        let sheet = (choices.sheet, choices.sheet_frames);
        assert_eq!((choices.title.as_deref(), sheet), (Some("pig"), ("4x1".parse().ok(), Some(3))));
        assert_eq!((choices.crop, choices.start, &choices.tag), (None, None, &None));
        assert_eq!(choices.webm, Options { crf: Some(20), ..default() });
        assert_eq!(choices.tgs, tgs::Options::default());
        let written = serde_json::to_value(&choices).unwrap();
        assert_eq!(written["webm"], json!({ "crf": 20, "title": "pig" }));
        assert_eq!(written["tgs"], json!({ "title": "pig", "sheet": "4x1", "sheet-frames": 3 }));
        let both = saved(json!({ "title": "webm" }), json!({ "title": "tgs" }));
        assert_eq!(both.title.as_deref(), Some("webm"));
        // and nothing else
        let newer = json!({ "preset": null, "target": "sticker", "made-up": 1 });
        assert!(serde_json::from_value::<Choices>(newer).is_err());
    }

    #[test]
    fn keeps_a_format_picked_before_the_file_is_read() {
        let mut item = Item::new(1, vec!["a.mp4".into()], false, Target::Sticker);
        assert!(item.format_guessed);
        item.take_kind(Kind::Aseprite);
        assert_eq!((item.format, item.format_guessed), (Format::Tgs, false));
        let mut picked = Item::new(2, vec!["a.mp4".into()], false, Target::Sticker);
        (picked.format, picked.format_guessed) = (Format::Webp, false);
        picked.take_kind(Kind::Image);
        assert_eq!((picked.kind, picked.format), (Kind::Image, Format::Webp));
    }

    #[test]
    fn leaves_a_presets_range_to_its_way_of_fitting() {
        let dir = tempfile::tempdir().unwrap();
        let preset = "[webm]\nfit = 'crf'\nfit-range = { min = 20, max = 40 }\n";
        std::fs::write(dir.path().join("quality.toml"), preset).unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let mut choices = Choices::new(Target::Sticker);
        choices.preset = Some("quality".into());
        let config = Config::default();
        let range = |choices: &Choices| {
            choices.recipe(Format::Webm, &presets, &config).unwrap().into_webm().fit_range
        };
        assert!(range(&choices).is_some());
        // a range of qualities is none of bitrates
        choices.webm.fit = Some(Fit::Bitrate);
        assert_eq!(range(&choices), None);
    }

    #[test]
    fn reads_art_at_the_presets_frame_rate() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("slow.toml"), "[tgs]\nfps = 2.0\n").unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let mut choices = Choices::new(Target::Sticker);
        choices.preset = Some("slow".into());
        choices.sheet = "4x1".parse().ok();
        let reading = choices.reading(&presets, &Config::default());
        assert_eq!((reading.sheet, reading.fps), ("4x1".parse().ok(), Some(2.0)));
    }

    #[test]
    fn draws_art_with_the_items_tag_at_the_presets_frame_rate() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("slow.toml"), "[webm]\nfps = 2.0\n").unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let mut choices = Choices::new(Target::Sticker);
        choices.preset = Some("slow".into());
        choices.tag = Some("walk".into());
        choices.hide_layers = Some(vec!["sketch".into()]);
        let config = Config::default();
        let drawing = choices.drawing(Format::Webm, &presets, &config);
        assert_eq!(drawing.tag.as_deref(), Some("walk"));
        assert_eq!(
            (drawing.hide_layers.as_deref(), drawing.fps),
            (Some(&["sketch".into()][..]), Some(2.0))
        );
        // a picture is made of one frame, at no rate
        assert_eq!(choices.drawing(Format::Webp, &presets, &config).fps, None);
        // and a broken preset leaves the item's own
        choices.preset = Some("no such preset".into());
        choices.webm.fps = Some(5.0);
        let drawing = choices.drawing(Format::Webm, &presets, &config);
        assert_eq!((drawing.tag.as_deref(), drawing.fps), (Some("walk"), Some(5.0)));
    }

    #[test]
    fn layers_choices_over_the_preset() {
        let config = Config::default();
        let presets = Presets::builtin();
        let mut choices = Choices::new(Target::Emoji);
        choices.preset = Some("fast".into());
        choices.webm.crf = Some(20);
        choices.start = Some(1.0);
        let options = choices.recipe(Format::Webm, &presets, &config).unwrap().into_webm();
        assert_eq!(options.target, Some(Target::Emoji));
        assert_eq!(options.effort, presets.webm("fast").unwrap().effort);
        // fast fits the bitrate, which hides the quality: it has no say
        assert_eq!((options.crf, options.start), (None, Some(1.0)));
        choices.webm.fit = Some(Fit::Length);
        let options = choices.recipe(Format::Webm, &presets, &config).unwrap().into_webm();
        assert_eq!(options.crf, Some(20));
        choices.webm.lossless = Some(true);
        let options = choices.recipe(Format::Webm, &presets, &config).unwrap().into_webm();
        assert_eq!((options.crf, options.lossless), (None, Some(true)));
        // and lossless has no say once fitting hides it again
        choices.webm.fit = None;
        let options = choices.recipe(Format::Webm, &presets, &config).unwrap().into_webm();
        assert_eq!((options.fit, options.lossless), (Some(Fit::Bitrate), None));
        let tgs = choices.recipe(Format::Tgs, &presets, &config).unwrap().into_tgs();
        assert_eq!((tgs.target, tgs.start), (Some(Target::Emoji), Some(1.0)));
    }

    // What the GUI makes of each scenario the command line's tests in
    // crates/cli/src/convert.rs pin, each in a twin of the same name. Where
    // the two differ, both name the divergence by its number in the plan to
    // unify how options are layered, so that unifying them changes these
    // on purpose.

    use tgradish_core::options::{Effort, IntegerScale, Spoof};

    /// The built-in presets and `files`, each `(name, text)`.
    fn presets(files: &[(&str, &str)]) -> Presets {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(format!("{name}.toml")), text).unwrap();
        }
        Presets::load(Some(dir.path())).unwrap()
    }

    /// The choices of a sticker made with `preset`.
    fn with_preset(preset: &str) -> Choices {
        Choices { preset: Some(preset.into()), ..Choices::new(Target::Sticker) }
    }

    /// WebM's options as the GUI's recipe has them, its target named.
    fn sticker(options: Options) -> Options {
        Options { target: Some(Target::Sticker), ..options }
    }

    fn tgs_sticker(options: tgs::Options) -> tgs::Options {
        tgs::Options { target: Some(Target::Sticker), ..options }
    }

    fn picture_sticker(options: webp::Options) -> webp::Options {
        webp::Options { target: Some(Target::Sticker), ..options }
    }

    #[test]
    fn layers_an_inherited_preset() {
        let child = "extends = 'fast'\n[webm]\nfit = 'crf'\ncrf = 20\n[tgs]\nexact = false\n";
        let presets = presets(&[("child", child)]);
        let config = Config::default();
        let recipe = |format| with_preset("child").recipe(format, &presets, &config).unwrap();
        // the crf fitting has no say over is set aside (divergence 7)
        let fitted =
            sticker(Options { fit: Some(Fit::Crf), effort: Some(Effort::Fast), ..default() });
        assert_eq!(recipe(Format::Webm), Recipe::Webm(fitted));
        let tgs = tgs::Options { effort: Some(Effort::Fast), exact: Some(false), ..default() };
        assert_eq!(recipe(Format::Tgs), Recipe::Tgs(tgs_sticker(tgs)));
        // a picture takes only a picture's (divergence 5)
        assert_eq!(recipe(Format::Webp), Recipe::Webp(picture_sticker(default())));
    }

    #[test]
    fn keeps_an_explicit_false_over_a_presets_true() {
        let on = "[webm]\nfit = 'off'\nlossless = true\nwatermark = true\n\
                  [tgs]\nkeep-canvas = true\nwatermark = true\n";
        let presets = presets(&[("on", on)]);
        let config = Config::default();
        let mut choices = with_preset("on");
        (choices.webm.watermark, choices.webm.lossless) = (Some(false), Some(false));
        (choices.tgs.watermark, choices.tgs.keep_canvas) = (Some(false), Some(false));
        let recipe = |format| choices.recipe(format, &presets, &config).unwrap();
        let webm_off = Options {
            fit: Some(Fit::Off),
            lossless: Some(false),
            watermark: Some(false),
            ..default()
        };
        assert_eq!(recipe(Format::Webm), Recipe::Webm(sticker(webm_off)));
        let picture_off = webp::Options { watermark: Some(false), ..default() };
        assert_eq!(recipe(Format::Webp), Recipe::Webp(picture_sticker(picture_off)));
        let tgs = tgs::Options { keep_canvas: Some(false), watermark: Some(false), ..default() };
        assert_eq!(recipe(Format::Tgs), Recipe::Tgs(tgs_sticker(tgs)));
    }

    #[test]
    fn crf_under_a_presets_bitrate_fit() {
        let presets = presets(&[("by bitrate", "[webm]\nfit = 'bitrate'\ncrf = 20\n")]);
        let recipe = with_preset("by bitrate").recipe(Format::Webm, &presets, &Config::default());
        // set aside, without a word (divergence 7)
        let fitted = sticker(Options { fit: Some(Fit::Bitrate), ..default() });
        assert_eq!(recipe.unwrap(), Recipe::Webm(fitted));
    }

    #[test]
    fn lossless_under_fit_auto() {
        let presets = presets(&[("lossless", "[webm]\nlossless = true\n")]);
        let mut choices = with_preset("lossless");
        choices.webm.fit = Some(Fit::Auto);
        let recipe = choices.recipe(Format::Webm, &presets, &Config::default());
        // set aside, without a word (divergence 1)
        let fitted = sticker(Options { fit: Some(Fit::Auto), ..default() });
        assert_eq!(recipe.unwrap(), Recipe::Webm(fitted));
    }

    #[test]
    fn drops_a_fit_range_of_another_unit() {
        let quality = "[webm]\nfit = 'crf'\nfit-range = { min = 20, max = 40 }\n";
        let presets = presets(&[("quality", quality)]);
        let mut choices = with_preset("quality");
        choices.webm.fit = Some(Fit::Bitrate);
        let recipe = choices.recipe(Format::Webm, &presets, &Config::default());
        let fitted = sticker(Options { fit: Some(Fit::Bitrate), ..default() });
        assert_eq!(recipe.unwrap(), Recipe::Webm(fitted));
    }

    #[test]
    fn fake_duration_under_spoof_never() {
        let presets = presets(&[
            ("never", "[webm]\nspoof = 'never'\nfake-duration = 1.0\n"),
            ("too long", "[webm]\nspoof = 'never'\nfake-duration = 5.0\n"),
        ]);
        let config = Config::default();
        let never = sticker(Options { spoof: Some(Spoof::Never), ..default() });
        // set aside, without a word (divergence 7)
        let recipe = with_preset("never").recipe(Format::Webm, &presets, &config);
        assert_eq!(recipe.unwrap(), Recipe::Webm(never));
        // though checked first, as the command line checks it
        let too_long = with_preset("too long").recipe(Format::Webm, &presets, &config);
        let said = "preset \"too long\" sets fake-duration, which must be greater than 0 and at \
                    most 3 seconds";
        assert_eq!(too_long.unwrap_err(), said);
    }

    #[test]
    fn integer_scale_without_integer_scaling() {
        let presets = presets(&[
            ("scaled", "[webm]\ninteger-scale = '2'\n"),
            ("cropped", "[webm]\nscaling = 'integer'\ninteger-scale = '2'\nresize = 'crop'\n"),
        ]);
        let config = Config::default();
        // set aside, as only integer scaling scales by it (divergence 2)
        let scaled = |format| with_preset("scaled").recipe(format, &presets, &config).unwrap();
        assert_eq!(scaled(Format::Webm), Recipe::Webm(sticker(default())));
        assert_eq!(scaled(Format::Webp), Recipe::Webp(picture_sticker(default())));
        // and the crop it can't go with set aside instead, for the default
        // (divergence 3)
        let cropped = with_preset("cropped").recipe(Format::Webm, &presets, &config);
        let scaled = Options {
            resize: Some(Resize::Contain),
            scaling: Some(Scaling::Integer),
            integer_scale: "2".parse::<IntegerScale>().ok(),
            ..default()
        };
        assert_eq!(cropped.unwrap(), Recipe::Webm(sticker(scaled)));
    }

    #[test]
    fn refuses_a_preset_setting_one_files_options() {
        let presets = presets(&[
            ("crop", "[webm]\ncrop = { x = 0, y = 0, width = 8, height = 8 }\n"),
            ("webm sheet", "[webm]\nsheet = '4x1'\n"),
            ("tgs sheet", "[tgs]\nsheet = '4x1'\n"),
        ]);
        let config = Config::default();
        for (name, key) in
            [("crop", "webm.crop"), ("webm sheet", "webm.sheet"), ("tgs sheet", "tgs.sheet")]
        {
            // the whole preset, for every format
            for format in [Format::Webm, Format::Webp, Format::Tgs] {
                let said = with_preset(name).recipe(format, &presets, &config).unwrap_err();
                assert!(said.starts_with("invalid preset "), "{said}");
                let per_file =
                    format!(": {key} is a per-file option and cannot be set in a preset");
                assert!(said.ends_with(&per_file), "{said}");
            }
        }
    }

    #[test]
    fn reads_options_by_their_old_names() {
        let renamed = "[webm]\nspeed = 'best'\nscaling = 'pixel-perfect'\nexact-scale = '3'\n\
                       [tgs]\nspeed = 'fast'\n";
        let presets = presets(&[("renamed", renamed)]);
        let config = Config::default();
        let recipe = |format| with_preset("renamed").recipe(format, &presets, &config).unwrap();
        let (scaling, integer_scale) = (Some(Scaling::Integer), "3".parse().ok());
        let webm = Options { scaling, integer_scale, effort: Some(Effort::Best), ..default() };
        assert_eq!(recipe(Format::Webm), Recipe::Webm(sticker(webm)));
        let tgs = tgs::Options { effort: Some(Effort::Fast), ..default() };
        assert_eq!(recipe(Format::Tgs), Recipe::Tgs(tgs_sticker(tgs)));
        let picture = webp::Options { scaling, integer_scale, ..default() };
        assert_eq!(recipe(Format::Webp), Recipe::Webp(picture_sticker(picture)));
    }

    #[test]
    fn makes_a_picture_of_a_presets_webm_options() {
        let presets = presets(&[
            ("picture", "[webm]\nfit = 'off'\ncrf = 20\nscaling = 'sharp'\n"),
            ("fast tempo", "[webm]\ntempo = 50.0\n"),
        ]);
        let config = Config::default();
        // a picture's only (divergence 5), started at its frame rather
        // than the item's start, and as long as one frame
        let mut choices = with_preset("picture");
        (choices.start, choices.length, choices.frame) = (Some(0.5), Some(1.0), Some(1.5));
        let recipe = choices.recipe(Format::Webp, &presets, &config);
        let picture =
            webp::Options { scaling: Some(Scaling::Sharp), start: Some(1.5), ..default() };
        assert_eq!(recipe.unwrap(), Recipe::Webp(picture_sticker(picture)));
        // so a value a picture doesn't use isn't checked, though it is for
        // WebM, as the command line checks it
        let fast = with_preset("fast tempo");
        let picture = fast.recipe(Format::Webp, &presets, &config);
        assert_eq!(picture.unwrap(), Recipe::Webp(picture_sticker(default())));
        let webm = fast.recipe(Format::Webm, &presets, &config);
        let said = "preset \"fast tempo\" sets tempo, which must be from 0.1 to 10";
        assert_eq!(webm.unwrap_err(), said);
    }

    #[test]
    fn bad_values_and_where_they_came_from() {
        let presets = presets(&[("slow", "[webm]\ntempo = 50.0\n[tgs]\ntempo = 50.0\n")]);
        let config = Config::default();
        // checked, naming the preset, as the command line says it; which
        // fails the conversion when it starts
        let said = "preset \"slow\" sets tempo, which must be from 0.1 to 10";
        for format in [Format::Webm, Format::Tgs] {
            let recipe = with_preset("slow").recipe(format, &presets, &config);
            assert_eq!(recipe.unwrap_err(), said, "{format:?}");
        }
    }

    #[test]
    fn sheet_frames_without_a_sheet() {
        let mut choices = Choices::new(Target::Sticker);
        choices.sheet_frames = Some(3);
        let recipe = choices.recipe(Format::Tgs, &Presets::builtin(), &Config::default());
        // left out, as it is kept only for when the sheet is ticked again,
        // where the command line refuses it (divergence 6)
        let best = tgs::Options { effort: Some(Effort::Best), ..default() };
        assert_eq!(recipe.unwrap(), Recipe::Tgs(tgs_sticker(best)));
    }

    #[test]
    fn picks_the_configs_preset_for_each_format() {
        let presets = Presets::builtin();
        let config = |preset: &str| {
            Config::from(tgradish_core::config::Config {
                preset: Some(preset.into()),
                tgs_preset: Some("balanced".into()),
                ..Default::default()
            })
        };
        let fast = config("fast");
        let unnamed = Choices::new(Target::Sticker);
        let recipe = |choices: &Choices, format, config| choices.recipe(format, &presets, config);
        let by_fast =
            sticker(Options { fit: Some(Fit::Bitrate), effort: Some(Effort::Fast), ..default() });
        assert_eq!(recipe(&unnamed, Format::Webm, &fast).unwrap(), Recipe::Webm(by_fast));
        // of which a picture takes none
        assert_eq!(
            recipe(&unnamed, Format::Webp, &fast).unwrap(),
            Recipe::Webp(picture_sticker(default()))
        );
        let balanced = tgs_sticker(tgs::Options { effort: Some(Effort::Balanced), ..default() });
        assert_eq!(recipe(&unnamed, Format::Tgs, &fast).unwrap(), Recipe::Tgs(balanced));
        // unless the item names one
        let best = tgs_sticker(tgs::Options { effort: Some(Effort::Best), ..default() });
        let named = recipe(&with_preset("best"), Format::Tgs, &fast);
        assert_eq!(named.unwrap(), Recipe::Tgs(best));
        // and one there isn't, said as presets say it, wherever it was
        // named
        let nope = config("nope");
        let said = "preset \"nope\" does not exist; available: balanced, best, fast";
        let (named, default_config) = (with_preset("nope"), Config::default());
        for format in [Format::Webm, Format::Webp] {
            assert_eq!(recipe(&unnamed, format, &nope).unwrap_err(), said);
            assert_eq!(recipe(&named, format, &default_config).unwrap_err(), said);
        }
        // which leaves .tgs to its own
        assert!(recipe(&unnamed, Format::Tgs, &nope).is_ok());
    }

    fn default<T: Default>() -> T {
        T::default()
    }
}

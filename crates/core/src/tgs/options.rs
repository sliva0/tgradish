//! Options for `.tgs` stickers, and the settings they come to for the
//! tgs crate.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tgradish_tgs::encode::Effort;
use tgradish_tgs::frames::Sheet;
use tgradish_tgs::normalise;
use tgradish_tgs::reduce::{Compromise, Kind};
use tgradish_tgs::sticker::{self, Fit};

use crate::error::{Error, Result};
use crate::options::{Crop, Resize, Speed, TEMPOS};
use crate::telegram::{self, Target};

/// Options for `.tgs` output. Like the WebM [`Options`](crate::webm::Options),
/// every field is optional so presets and flags can be layered.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
// `describe` titles the schema as it always has
#[schemars(rename = "TgsOptions")]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// What to make. Both are square animations. Default: sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Seconds to skip at the start of the input.
    #[schemars(range(min = 0.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Seconds of the input to use, from `start`. Default: the rest of the
    /// input.
    #[schemars(extend("exclusiveMinimum" = 0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// How many times as fast as the input it plays, 0.1 to 10; what still
    /// lasts longer than 3 seconds is cut. Default: as fast as fits a long
    /// input into 3 seconds.
    #[schemars(range(min = 0.1, max = 10.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<f64>,
    /// How hard to look for a small encoding; best takes a few seconds.
    /// Default: best.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    /// Never change the pixel art to make it fit; a sticker that is too
    /// large is reported instead. Set it to false to let fitting change it.
    /// Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact: Option<bool>,
    /// Reductions fitting may use, least visible first. Default: all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reductions: Option<Vec<Kind>>,
    /// What fitting gives up first: motion (frames merged and dropped,
    /// full detail), detail (smooth motion, a coarser picture), or auto,
    /// whatever changes the pixel art least. Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compromise: Option<Compromise>,
    /// The part of the input to use, in input pixels. Default: all of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// How art that isn't square fills the square sticker: pad (whole,
    /// with transparent margins), crop (its middle) or
    /// stretch (its art pixels no longer square). Default: pad.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// Keep the input's canvas, or the crop, instead of cropping to the
    /// visible pixels. Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_canvas: Option<bool>,
    /// Size of one art pixel in input pixels. Pixels off its grid are moved
    /// onto it. Default: detected.
    #[schemars(range(min = 1))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_scale: Option<u32>,
    /// Aseprite tag to export. Default: all frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Aseprite layers to show, by name, hidden ones included. Default:
    /// those the file shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_layers: Option<Vec<String>>,
    /// Aseprite layers to hide, by name; a hidden group hides the layers in
    /// it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_layers: Option<Vec<String>>,
    /// Read the input as a sprite sheet of COLUMNSxROWS equal cells, frames
    /// row by row, for example 4x2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<crate::options::Sheet>,
    /// How many cells of the sprite sheet hold frames. Default: all but
    /// transparent ones at the end.
    #[schemars(range(min = 1))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet_frames: Option<u32>,
    /// The rate the pictures of a sprite sheet or an image sequence are
    /// read at: how fast they play. Default: 12.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 60))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Name stored in the sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Name the sticker as made by tgradish. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
}

crate::options::layered!(Options {
    target,
    start,
    length,
    tempo,
    speed,
    exact,
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
});

/// The values of options left unset, which converting takes: each says
/// its default once, for core and front-ends alike.
impl Options {
    /// How art that isn't square fills the sticker: padded.
    pub fn resize_or_default(&self) -> Resize {
        self.resize.unwrap_or(Resize::Pad)
    }

    /// How hard to look for a small encoding: the best, which takes
    /// seconds.
    pub fn speed_or_default(&self) -> Speed {
        self.speed.unwrap_or(Speed::Best)
    }

    /// Whether the input's canvas is kept: no, the art is cropped to what
    /// shows.
    pub fn keep_canvas_or_default(&self) -> bool {
        self.keep_canvas.unwrap_or(false)
    }

    /// How many times as fast as the input the sticker plays, for a part
    /// of it `used` seconds long where that is known: as fast as fits a
    /// longer part into Telegram's 3 seconds, as normalising plays it.
    pub fn tempo_or_default(&self, used: Option<f64>) -> f64 {
        let fitting = |used: f64| (used / telegram::MAX_SECONDS).max(1.0);
        self.tempo.unwrap_or_else(|| used.map_or(1.0, fitting))
    }

    /// Whether the art is kept exactly as it is: yes, as changing it to
    /// fit is the user's to choose.
    pub fn exact_or_default(&self) -> bool {
        self.exact.unwrap_or(true)
    }
}

impl Options {
    /// These options with every value `overlay` sets in place of their
    /// own.
    pub fn merged(self, overlay: &Options) -> Options {
        self.overlaid(overlay)
    }

    pub(super) fn sheet_layout(&self) -> Result<Option<Sheet>> {
        let Some(sheet) = self.sheet else { return Ok(None) };
        if sheet.columns == 0 || sheet.rows == 0 {
            return Err(Error::invalid("sheet", format!("{sheet} has no cells")));
        }
        Ok(Some(Sheet {
            frames: self.sheet_frames,
            frame_duration: self.frame_duration()?,
            ..Sheet::new(sheet.columns, sheet.rows)
        }))
    }

    pub(super) fn frame_duration(&self) -> Result<Duration> {
        crate::art::frame_duration(self.fps)
    }

    fn seconds(value: Option<f64>, name: &'static str) -> Result<Option<Duration>> {
        value
            .map(|seconds| {
                Duration::try_from_secs_f64(seconds)
                    .map_err(|_| Error::invalid(name, format!("{seconds} is not valid")))
            })
            .transpose()
    }

    /// The settings for the tgs crate.
    pub fn sticker_options(&self) -> Result<sticker::Options> {
        let name = match (self.watermark.unwrap_or(true), &self.title) {
            (true, Some(title)) => Some(format!("{title}{}", crate::TITLED)),
            (true, None) => Some(format!("{}{}", crate::UNTITLED, crate::TOOL_ID)),
            (false, title) => title.clone(),
        };
        Ok(sticker::Options {
            normalise: normalise::Options {
                keep_canvas: self.keep_canvas_or_default(),
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
                        let reason = format!("{tempo} is not from {low} to {high}");
                        return Err(Error::invalid("tempo", reason));
                    }
                    tempo => tempo,
                },
                square: match self.resize_or_default() {
                    Resize::Pad => normalise::Square::Pad,
                    Resize::Crop => normalise::Square::Crop,
                    Resize::Stretch => normalise::Square::Stretch,
                    Resize::Contain => {
                        let reason = "contain can't make TGS stickers, which are square: use pad, \
                                      crop or stretch";
                        return Err(Error::invalid("resize", reason));
                    }
                },
            },
            effort: match self.speed_or_default() {
                Speed::Fast => Effort::Fast,
                Speed::Balanced => Effort::Balanced,
                Speed::Best => Effort::Best,
            },
            fit: if self.exact_or_default() { Fit::Exact } else { Fit::Auto },
            reductions: self.reductions.clone().unwrap_or_else(|| Kind::ALL.to_vec()),
            compromise: self.compromise.unwrap_or_default(),
            name,
            mark: Some(crate::mark::Mark::current().to_bytes().to_vec()),
            ..sticker::Options::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tgs::DEFAULT_FPS;

    #[test]
    fn keeps_the_art_unless_asked() {
        let fit = |exact| Options { exact, ..Options::default() }.sticker_options();
        assert_eq!(fit(None).unwrap().fit, Fit::Exact);
        assert_eq!(fit(Some(false)).unwrap().fit, Fit::Auto);
    }

    #[test]
    fn says_its_defaults_once() {
        let unset = Options::default();
        assert_eq!(
            (unset.resize_or_default(), unset.speed_or_default()),
            (Resize::Pad, Speed::Best)
        );
        assert!(unset.exact_or_default() && !unset.keep_canvas_or_default());
        let settings = unset.sticker_options().unwrap();
        assert_eq!((settings.effort, settings.fit), (Effort::Best, Fit::Exact));
        // a long part plays as fast as fits it into 3 s, a short one as it is
        assert_eq!(
            (unset.tempo_or_default(Some(6.0)), unset.tempo_or_default(Some(2.0))),
            (2.0, 1.0)
        );
        assert_eq!(unset.tempo_or_default(None), 1.0);
        let set = Options { resize: Some(Resize::Crop), exact: Some(false), ..unset };
        assert_eq!(Options { tempo: Some(0.5), ..set.clone() }.tempo_or_default(Some(6.0)), 0.5);
        assert_eq!((set.resize_or_default(), set.exact_or_default()), (Resize::Crop, false));
    }

    #[test]
    fn refuses_tiny_frame_rates() {
        let slow = Options { fps: Some(1e-100), ..Default::default() };
        assert!(matches!(slow.frame_duration(), Err(Error::InvalidOption { option: "fps", .. })));
    }

    #[test]
    fn layers_options() {
        let preset = Options { speed: Some(Speed::Fast), fps: Some(5.0), ..Default::default() };
        let flags = Options { fps: Some(12.0), exact: Some(true), ..Default::default() };
        let merged = preset.merged(&flags);
        assert_eq!(
            (merged.speed, merged.fps, merged.exact),
            (Some(Speed::Fast), Some(12.0), Some(true))
        );
        let settings = merged.sticker_options().unwrap();
        assert_eq!((settings.effort, settings.fit), (Effort::Fast, Fit::Exact));
        let sheet = Options { sheet: "4x2".parse().ok(), ..Default::default() };
        let layout = sheet.sheet_layout().unwrap().unwrap();
        assert_eq!(
            (layout.columns, layout.rows, layout.frame_duration),
            (4, 2, Duration::from_secs_f64(1.0 / DEFAULT_FPS))
        );
        // made in code, which reading refuses
        let none = crate::options::Sheet { columns: 0, rows: 2 };
        let bad = Options { sheet: Some(none), ..Default::default() };
        assert!(bad.sheet_layout().is_err());
    }
}

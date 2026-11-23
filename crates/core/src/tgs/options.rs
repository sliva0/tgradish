//! `.tgs` conversion options and conversion into
//! `tgradish_tgs::sticker::Options`.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tgradish_tgs::encode;
use tgradish_tgs::frames::Sheet;
use tgradish_tgs::normalise;
use tgradish_tgs::reduce::{Compromise, Kind};
use tgradish_tgs::sticker::{self, Fit};

use crate::error::{Error, Result};
use crate::options::{Crop, Effort, Resize, TEMPOS};
use crate::telegram::{self, Target};

/// Options for `.tgs` output. Like [`crate::webm::Options`], every field is
/// optional so presets and flags can be layered.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
// `describe` titles the schema as it always has
#[schemars(rename = "TgsOptions")]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// Target type: sticker or emoji (both are 512×512). Default: sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Start offset in seconds.
    #[schemars(range(min = 0.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Maximum duration in seconds to read from `start`. Default: full
    /// remaining duration.
    #[schemars(extend("exclusiveMinimum" = 0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Playback speed multiplier (0.1 to 10); anything still over 3 seconds is
    /// trimmed. Default: sped up to fit within 3 seconds.
    #[schemars(range(min = 0.1, max = 10.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<f64>,
    /// Encoder effort (`best` takes a few seconds). Default: best.
    // read as `speed` too, as it was once named
    #[serde(alias = "speed", skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Keep pixel art unchanged and report if the file exceeds 64 KiB. Set to
    /// `false` to allow lossy reductions. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact: Option<bool>,
    /// Reductions allowed when fitting, applied from least visible first.
    /// Default: all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reductions: Option<Vec<Kind>>,
    /// What fitting sacrifices first: `motion` (merge/drop frames, keep
    /// detail), `detail` (coarser image, smooth motion), or `auto` (least
    /// visible change). Default: auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compromise: Option<Compromise>,
    /// Region of the input to keep, in source pixels. Default: full frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// How non-square art fills the square TGS canvas: `pad` (transparent
    /// margins), `crop` (centre crop), `stretch` (non-square pixels), or
    /// `contain` (requires the input or crop to already be square). Default:
    /// pad.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// Keep the full input canvas (or crop) instead of trimming transparent
    /// margins. Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_canvas: Option<bool>,
    /// Size of one art pixel in source pixels; misaligned edges are snapped to
    /// this grid. Default: auto-detected.
    #[schemars(range(min = 1))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_scale: Option<u32>,
    /// Aseprite tag to export. Default: all frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Aseprite layers to show by name (including hidden layers). Default:
    /// layers visible in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_layers: Option<Vec<String>>,
    /// Aseprite layers to hide by name; hiding a group hides its children.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_layers: Option<Vec<String>>,
    /// Treat the input as a sprite sheet of `COLUMNSxROWS` equal cells, read
    /// row by row (for example, `4x2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<crate::options::Sheet>,
    /// Number of frames to read from a sprite sheet. Default: all cells except
    /// trailing empty ones.
    #[schemars(range(min = 1))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet_frames: Option<u32>,
    /// Playback frame rate for sprite sheets and image sequences. Default: 12.
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 60))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Lottie name stored in the sticker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Include tgradish in the sticker's Lottie name. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
}

crate::options::layered!(Options {
    target,
    start,
    length,
    tempo,
    effort,
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

/// Single source of truth for default option values, shared by core and
/// front-ends.
impl Options {
    /// Configured resize mode, or the default (`Resize::Pad`).
    pub fn resize_or_default(&self) -> Resize {
        self.resize.unwrap_or(Resize::Pad)
    }

    /// Configured encoder effort, or the default (`Effort::Best`, which takes a
    /// few seconds).
    pub fn effort_or_default(&self) -> Effort {
        self.effort.unwrap_or(Effort::Best)
    }

    /// Configured canvas behaviour, or `false` by default (trim transparent
    /// margins).
    pub fn keep_canvas_or_default(&self) -> bool {
        self.keep_canvas.unwrap_or(false)
    }

    /// Effective tempo for an input clip of `used` seconds: speeds up clips
    /// longer than 3 seconds to fit within Telegram's limit when unset.
    pub fn tempo_or_default(&self, used: Option<f64>) -> f64 {
        let fitting = |used: f64| (used / telegram::MAX_SECONDS).max(1.0);
        self.tempo.unwrap_or_else(|| used.map_or(1.0, fitting))
    }

    /// Configured exact mode, or `true` by default (keep pixel art unchanged
    /// unless the user opts into lossy fitting).
    pub fn exact_or_default(&self) -> bool {
        self.exact.unwrap_or(true)
    }
}

impl Options {
    /// Overlays all set fields from `overlay` onto `self`.
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
                    .map_err(|_| Error::invalid(name, format!("{seconds} is not a valid duration")))
            })
            .transpose()
    }

    /// The settings for the tgs crate.
    pub fn sticker_options(&self) -> Result<sticker::Options> {
        let watermark = self.watermark.unwrap_or(true);
        let name = match (watermark, &self.title) {
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
                        let reason = format!("{tempo} must be from {low} to {high}");
                        return Err(Error::invalid("tempo", reason));
                    }
                    tempo => tempo,
                },
                square: match self.resize_or_default() {
                    // the part used is square, which converting checks
                    // once it knows the input's size: nothing to fill
                    Resize::Pad | Resize::Contain => normalise::Square::Pad,
                    Resize::Crop => normalise::Square::Crop,
                    Resize::Stretch => normalise::Square::Stretch,
                },
            },
            effort: match self.effort_or_default() {
                Effort::Fast => encode::Effort::Fast,
                Effort::Balanced => encode::Effort::Balanced,
                Effort::Best => encode::Effort::Best,
            },
            fit: if self.exact_or_default() { Fit::Exact } else { Fit::Auto },
            reductions: self.reductions.clone().unwrap_or_else(|| Kind::ALL.to_vec()),
            compromise: self.compromise.unwrap_or_default(),
            name,
            mark: crate::mark::Mark::written(watermark).map(|mark| mark.to_bytes().to_vec()),
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
            (unset.resize_or_default(), unset.effort_or_default()),
            (Resize::Pad, Effort::Best)
        );
        assert!(unset.exact_or_default() && !unset.keep_canvas_or_default());
        let settings = unset.sticker_options().unwrap();
        assert_eq!((settings.effort, settings.fit), (encode::Effort::Best, Fit::Exact));
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
    fn marks_only_a_watermarked_sticker() {
        let mark = |watermark| Options { watermark, ..Options::default() }.sticker_options();
        let current = crate::mark::Mark::current().to_bytes().to_vec();
        assert_eq!(mark(None).unwrap().mark, Some(current.clone()));
        assert_eq!(mark(Some(true)).unwrap().mark, Some(current));
        assert_eq!(mark(Some(false)).unwrap().mark, None);
    }

    #[test]
    fn refuses_tiny_frame_rates() {
        let slow = Options { fps: Some(1e-100), ..Default::default() };
        assert!(matches!(slow.frame_duration(), Err(Error::InvalidOption { option: "fps", .. })));
    }

    #[test]
    fn layers_options() {
        let preset = Options { effort: Some(Effort::Fast), fps: Some(5.0), ..Default::default() };
        let flags = Options { fps: Some(12.0), exact: Some(true), ..Default::default() };
        let merged = preset.merged(&flags);
        assert_eq!(
            (merged.effort, merged.fps, merged.exact),
            (Some(Effort::Fast), Some(12.0), Some(true))
        );
        let settings = merged.sticker_options().unwrap();
        assert_eq!((settings.effort, settings.fit), (encode::Effort::Fast, Fit::Exact));
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

//! Size and quality for WebM: what fitting changes to get under the size
//! limit, and the values the encoder works from. Most of it only matters
//! for some ways of fitting, so the rest is greyed out rather than hidden.

use eframe::egui;
use tgradish_core::options::{
    CRFS, DEFAULT_ATTEMPTS, DEFAULT_CRF, Fit, MAX_ATTEMPTS, Range, Speed,
};

use super::Context;
use crate::item::Item;
use crate::widgets::{
    Choice, auto_number, chosen_hint, flag, preset_segments, row, section, segments,
};

const FITS: [Choice<Fit>; 6] = [
    (
        Fit::Auto,
        "Best looking",
        "Tries a few frame rates, fits the bitrate for each and keeps the one most like the input",
    ),
    (Fit::Bitrate, "Bitrate", "Fits the bitrate at the chosen frame rate"),
    (Fit::Crf, "Quality", "Fits the constant quality value (CRF)"),
    (Fit::Fps, "Frame rate", "Fits the frame rate at constant quality"),
    (Fit::Length, "Length", "Fits the length at constant quality, cutting the end"),
    (Fit::Off, "Off", "Encodes once with the values below, even if it comes out too large"),
];

const WEBM_SPEEDS: [Choice<Speed>; 3] = [
    (Speed::Fast, "Fast", "Seconds; visibly worse at the same size"),
    (Speed::Balanced, "Balanced", ""),
    (Speed::Best, "Best", "Slow: minutes for long videos"),
];

/// The label of a value the preset sets.
const PRESETS: (&str, &str) = ("Preset's", "The value the preset sets");

pub(super) fn webm_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, "Size and quality");
    let base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.webm;
    let fit = options.fit.or(base.fit).unwrap_or(Fit::Auto);
    let hint = "What tgradish changes to land just under the size limit";
    row(ui, "Getting under the limit", hint, Ok(()), |ui| {
        preset_segments(ui, &mut options.fit, base.fit.unwrap_or(Fit::Auto), &FITS);
        chosen_hint(ui, fit, &FITS);
    });
    let hint = "Faster encoding looks worse at the same size";
    row(ui, "VP9 encoder", hint, Ok(()), |ui| {
        let base = base.speed.unwrap_or(Speed::Balanced);
        preset_segments(ui, &mut options.speed, base, &WEBM_SPEEDS);
    });

    // every row stays, greyed out where this way of fitting has no use
    // for it
    let fitting = if fit == Fit::Off { Err("Off encodes once") } else { Ok(()) };
    row(ui, "Encodes at most", "How many times fitting may encode", fitting, |ui| {
        let (automatic, default) = match base.attempts {
            Some(attempts) => (PRESETS, attempts),
            None => (("Default", "As many as land just under the limit"), DEFAULT_ATTEMPTS),
        };
        auto_number(ui, &mut options.attempts, automatic, Some(default), default, |value| {
            value.range(1..=MAX_ATTEMPTS)
        });
    });
    let unit = match fit {
        Fit::Auto | Fit::Bitrate | Fit::Off => " kbit/s",
        Fit::Fps => " fps",
        Fit::Length => " s",
        Fit::Crf => "",
    };
    let hint = "The range fitting searches, in the fitted value's unit";
    row(ui, "Search within", hint, fitting, |ui| {
        ui.horizontal(|ui| {
            let mut range = options.fit_range;
            let start = match fit {
                Fit::Crf => Range { min: 4.0, max: f64::from(*CRFS.end()) },
                Fit::Fps => Range { min: 1.0, max: 30.0 },
                Fit::Length => Range { min: 0.1, max: 3.0 },
                _ => Range { min: 8.0, max: 2000.0 },
            };
            let mut custom = range.is_some();
            let choices = [(false, "Automatic", ""), (true, "Custom", "")];
            if segments(ui, &mut custom, &choices, |_| Ok(())) {
                range = custom.then_some(start);
            }
            let mut shown = range.unwrap_or(start);
            let enabled = range.is_some();
            let pick = "Pick Custom to set it";
            let low = ui
                .add_enabled(
                    enabled,
                    egui::DragValue::new(&mut shown.min).speed(0.5).max_decimals(2),
                )
                .on_disabled_hover_text(pick);
            ui.label("to");
            let high = ui
                .add_enabled(
                    enabled,
                    egui::DragValue::new(&mut shown.max).speed(0.5).max_decimals(2).suffix(unit),
                )
                .on_disabled_hover_text(pick);
            if enabled && (low.changed() || high.changed()) {
                shown.max = shown.max.max(shown.min);
                range = Some(shown);
            }
            options.fit_range = range;
        });
    });
    let quality = match fit {
        Fit::Fps | Fit::Length | Fit::Off => Ok(()),
        Fit::Crf => Err("Fitting finds the quality"),
        Fit::Auto | Fit::Bitrate => Err("Fitting finds a bitrate, not a quality"),
    };
    let hint = "Constant quality from 0, the best, to 63";
    row(ui, "Quality (CRF)", hint, quality, |ui| {
        let (automatic, default) = match base.crf {
            Some(crf) => (PRESETS, crf),
            None => (("Default", "A good quality at sticker sizes"), DEFAULT_CRF),
        };
        auto_number(ui, &mut options.crf, automatic, Some(default), default, |value| {
            value.range(CRFS)
        });
    });
    // ticked from any way of fitting: one that keeps a quality takes over
    let keeps_quality = matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    row(ui, "Lossless", "", Ok(()), |ui| {
        let hint = if keeps_quality {
            "Only small for tiny or still pictures"
        } else {
            "Only small for tiny or still pictures; ticked, fitting goes by frame rate, as \
             lossless keeps no bitrate or quality to fit"
        };
        let preset = base.lossless.unwrap_or(false);
        if flag(ui, &mut options.lossless, preset, "Lossless encoding", hint)
            && options.lossless == Some(true)
            && !keeps_quality
        {
            options.fit = Some(Fit::Fps);
        }
    });
    let bitrate = match fit {
        Fit::Off => Ok(()),
        Fit::Auto | Fit::Bitrate => Err("Fitting finds the bitrate"),
        Fit::Crf | Fit::Fps | Fit::Length => Err("Encoded at a constant quality, not a bitrate"),
    };
    row(ui, "Bitrate", "Used unless only a quality is set", bitrate, |ui| {
        let automatic = ("From the size limit", "As much as the length lets fit");
        auto_number(ui, &mut options.bitrate, automatic, None, 600.0, |value| {
            value.range(1.0..=50_000.0).speed(5.0).max_decimals(0).suffix(" kbit/s")
        });
    });
}

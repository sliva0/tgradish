//! Size and quality for WebM: what fitting changes to get under the size
//! limit, and the values the encoder works from. Most of it only matters
//! for some ways of fitting, so the rest is greyed out rather than hidden.

use eframe::egui;
use tgradish_core::options::{
    CRFS, DEFAULT_ATTEMPTS, DEFAULT_CRF, Fit, MAX_ATTEMPTS, Range, Speed,
};

use tgradish_core::{convert, telegram};

use super::{Context, commands};
use crate::item::Item;
use crate::widgets::{
    Choice, Pick, auto_number, chosen_hint, flag, greyed, preset_picking, preset_segments, row,
    section, segments,
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
    let base_fit = base.fit.unwrap_or(Fit::Auto);
    let options = &mut item.choices.webm;
    let fit = options.fit.unwrap_or(base_fit);
    // lossless keeps no bitrate or quality to fit, so only ways of fitting
    // that keep a quality go with it
    let keeps_quality = |fit: Fit| matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    let lossless = keeps_quality(fit) && options.lossless.or(base.lossless).unwrap_or(false);
    let hint = "What tgradish changes to land just under the size limit";
    row(ui, "Getting under the limit", hint, Ok(()), |ui| {
        let pick = |fit: Fit| match lossless && !keeps_quality(fit) {
            true => Pick::Changes(
                "Lossless keeps no bitrate or quality to fit: picking this unticks Lossless",
            ),
            false => Pick::Yes,
        };
        if preset_picking(ui, &mut options.fit, base_fit, &FITS, pick)
            && lossless
            && !keeps_quality(options.fit.unwrap_or(base_fit))
        {
            options.lossless = (base.lossless == Some(true)).then_some(false);
        }
        chosen_hint(ui, options.fit.unwrap_or(base_fit), &FITS);
    });
    let hint = "Faster encoding looks worse at the same size";
    row(ui, "VP9 encoder", hint, Ok(()), |ui| {
        let base = base.speed.unwrap_or(Speed::Balanced);
        preset_segments(ui, &mut options.speed, base, &WEBM_SPEEDS);
    });

    // every row stays, greyed out where this way of fitting has no use
    // for it
    let fitting = if fit == Fit::Off { Err("Off encodes once") } else { Ok(()) };
    let hint = "How many times fitting may encode, at most";
    row(ui, "Encodes up to", hint, fitting, |ui| {
        let (automatic, default) = match base.attempts {
            Some(attempts) => (PRESETS, attempts),
            None => (("Default", "As many as land just under the limit"), DEFAULT_ATTEMPTS),
        };
        auto_number(ui, &mut options.attempts, automatic, Some(default), default, |value| {
            value
                .range(1..=MAX_ATTEMPTS)
                .custom_formatter(|n, _| format!("{n} {}", if n == 1.0 { "time" } else { "times" }))
                .custom_parser(|text| {
                    text.trim().trim_end_matches(char::is_alphabetic).trim().parse().ok()
                })
        });
    });
    let unit = match fit {
        Fit::Auto | Fit::Bitrate | Fit::Off => " kbit/s",
        Fit::Fps => " fps",
        Fit::Length => " s",
        Fit::Crf => "",
    };
    let hint = "The range fitting searches, in the fitted value's unit";
    let mut asked_custom = false;
    row(ui, "Search within", hint, fitting, |ui| {
        ui.horizontal(|ui| {
            let mut custom = options.fit_range.is_some();
            let choices = [
                (false, "Automatic", "As wide as suits the way of fitting and the input"),
                (true, "Custom", ""),
            ];
            if segments(ui, &mut custom, &choices, |_| Ok(())) {
                if custom {
                    asked_custom = true;
                } else {
                    options.fit_range = None;
                }
            }
            // automatic, the range isn't shown: it is fitting's to choose
            let set = options.fit_range;
            let mut shown = set.unwrap_or(Range { min: 0.0, max: 0.0 });
            let pick = "Pick Custom to set it";
            let low = ui
                .add_enabled(set.is_some(), range_field(&mut shown.min, unit, set.is_some()))
                .on_disabled_hover_text(pick);
            ui.label("to");
            let high = ui
                .add_enabled(set.is_some(), range_field(&mut shown.max, unit, set.is_some()))
                .on_disabled_hover_text(pick);
            if set.is_some() && (low.changed() || high.changed()) {
                shown.max = shown.max.max(shown.min);
                options.fit_range = Some(shown);
            }
        });
    });
    // a custom range starts as the one fitting would search
    if asked_custom {
        let (fps, length) = commands::planned(item, context)
            .map_or((30.0, telegram::MAX_SECONDS), |(_, plan)| (plan.fps, plan.length));
        item.choices.webm.fit_range = Some(convert::default_fit_range(fit, fps, length));
    }
    let options = &mut item.choices.webm;
    let quality = match fit {
        _ if lossless => Err("Lossless encodes without a quality"),
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
    // greyed out where fitting keeps no quality, but live: ticked, fitting
    // goes by frame rate
    row(ui, "Lossless", "", Ok(()), |ui| {
        let hint = if keeps_quality(fit) {
            "Only small for tiny or still pictures"
        } else {
            "Only small for tiny or still pictures. Lossless keeps no bitrate or quality to fit: \
             ticking this switches fitting to Frame rate"
        };
        let preset = base.lossless.unwrap_or(false);
        let changed = greyed(ui, !keeps_quality(fit), |ui| {
            flag(ui, &mut options.lossless, preset, "Lossless encoding", hint)
        });
        if changed && options.lossless.unwrap_or(preset) && !keeps_quality(fit) {
            options.fit = (base_fit != Fit::Fps).then_some(Fit::Fps);
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

/// A field of an end of the range fitting searches, in `unit`; shown as
/// "—" unless `set`, as fitting chooses the range then.
fn range_field<'a>(value: &'a mut f64, unit: &'static str, set: bool) -> egui::DragValue<'a> {
    let field = egui::DragValue::new(value).speed(0.5).max_decimals(2);
    if set { field.suffix(unit) } else { field.custom_formatter(|_, _| "—".to_owned()) }
}

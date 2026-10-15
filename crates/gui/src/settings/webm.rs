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
use crate::l10n::fl;
use crate::widgets::{
    Choice, Pick, auto_number, chosen_hint, flag, greyed, preset_picking, preset_segments, row,
    section, segments,
};

fn fits() -> [Choice<Fit>; 6] {
    [
        (Fit::Auto, fl!("fit-auto"), fl!("fit-auto", "hint")),
        (Fit::Bitrate, fl!("fit-bitrate"), fl!("fit-bitrate", "hint")),
        (Fit::Crf, fl!("fit-crf"), fl!("fit-crf", "hint")),
        (Fit::Fps, fl!("fit-fps"), fl!("fit-fps", "hint")),
        (Fit::Length, fl!("fit-length"), fl!("fit-length", "hint")),
        (Fit::Off, fl!("fit-off"), fl!("fit-off", "hint")),
    ]
}

fn webm_speeds() -> [Choice<Speed>; 3] {
    [
        (Speed::Fast, fl!("encoder-fast"), fl!("encoder-fast", "hint")),
        (Speed::Balanced, fl!("encoder-balanced"), String::new()),
        (Speed::Best, fl!("encoder-best"), fl!("encoder-best", "hint")),
    ]
}

pub(super) fn webm_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, &fl!("size-and-quality"));
    // the label of a value the preset sets
    let presets = (fl!("from-preset-value"), fl!("from-preset-value", "hint"));
    let presets = (presets.0.as_str(), presets.1.as_str());
    let base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let base_fit = base.fit.unwrap_or(Fit::Auto);
    let options = &mut item.choices.webm;
    let fit = options.fit.unwrap_or(base_fit);
    // lossless keeps no bitrate or quality to fit, so only ways of fitting
    // that keep a quality go with it
    let keeps_quality = |fit: Fit| matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    let lossless = keeps_quality(fit) && options.lossless.or(base.lossless).unwrap_or(false);
    row(ui, &fl!("fitting"), &fl!("fitting", "hint"), Ok(()), |ui| {
        let fits = fits();
        let pick = |fit: Fit| match lossless && !keeps_quality(fit) {
            true => Pick::Changes(fl!("fit-off", "lossless")),
            false => Pick::Yes,
        };
        if preset_picking(ui, &mut options.fit, base_fit, &fits, pick)
            && lossless
            && !keeps_quality(options.fit.unwrap_or(base_fit))
        {
            options.lossless = (base.lossless == Some(true)).then_some(false);
        }
        chosen_hint(ui, options.fit.unwrap_or(base_fit), &fits);
    });
    row(ui, &fl!("encoder"), &fl!("encoder", "hint"), Ok(()), |ui| {
        let base = base.speed.unwrap_or(Speed::Balanced);
        preset_segments(ui, &mut options.speed, base, &webm_speeds());
    });

    // every row stays, greyed out where this way of fitting has no use
    // for it
    let off = fl!("fitting-off");
    let fitting = if fit == Fit::Off { Err(off.as_str()) } else { Ok(()) };
    row(ui, &fl!("attempts"), &fl!("attempts", "hint"), fitting, |ui| {
        let defaults = (fl!("attempts-default"), fl!("attempts-default", "hint"));
        let (automatic, default) = match base.attempts {
            Some(attempts) => (presets, attempts),
            None => ((defaults.0.as_str(), defaults.1.as_str()), DEFAULT_ATTEMPTS),
        };
        auto_number(ui, &mut options.attempts, automatic, Some(default), default, |value| {
            value
                .range(1..=MAX_ATTEMPTS)
                .custom_formatter(|n, _| fl!("attempts-times", count = n))
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
    let mut asked_custom = false;
    row(ui, &fl!("search-within"), &fl!("search-within", "hint"), fitting, |ui| {
        ui.horizontal(|ui| {
            let mut custom = options.fit_range.is_some();
            let choices = [
                (false, fl!("search-auto"), fl!("search-auto", "hint")),
                (true, fl!("custom"), String::new()),
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
            let pick = fl!("custom", "hint");
            let low = ui
                .add_enabled(set.is_some(), range_field(&mut shown.min, unit, set.is_some()))
                .on_disabled_hover_text(pick.as_str());
            ui.label(fl!("range-to"));
            let high = ui
                .add_enabled(set.is_some(), range_field(&mut shown.max, unit, set.is_some()))
                .on_disabled_hover_text(pick.as_str());
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
        _ if lossless => Err(fl!("crf", "lossless")),
        Fit::Fps | Fit::Length | Fit::Off => Ok(()),
        Fit::Crf => Err(fl!("crf", "fitted")),
        Fit::Auto | Fit::Bitrate => Err(fl!("crf", "bitrate")),
    };
    row(
        ui,
        &fl!("crf"),
        &fl!("crf", "hint"),
        quality.as_ref().map_err(String::as_str).copied(),
        |ui| {
            let defaults = (fl!("crf-default"), fl!("crf-default", "hint"));
            let (automatic, default) = match base.crf {
                Some(crf) => (presets, crf),
                None => ((defaults.0.as_str(), defaults.1.as_str()), DEFAULT_CRF),
            };
            auto_number(ui, &mut options.crf, automatic, Some(default), default, |value| {
                value.range(CRFS)
            });
        },
    );
    // greyed out where fitting keeps no quality, but live: ticked, fitting
    // goes by frame rate
    row(ui, &fl!("lossless"), "", Ok(()), |ui| {
        let hint = if keeps_quality(fit) {
            fl!("lossless-encoding", "hint")
        } else {
            fl!("lossless-encoding", "switches")
        };
        let preset = base.lossless.unwrap_or(false);
        let text = fl!("lossless-encoding");
        let changed = greyed(ui, !keeps_quality(fit), |ui| {
            flag(ui, &mut options.lossless, preset, &text, &hint)
        });
        if changed && options.lossless.unwrap_or(preset) && !keeps_quality(fit) {
            options.fit = (base_fit != Fit::Fps).then_some(Fit::Fps);
        }
    });
    let bitrate = match fit {
        Fit::Off => Ok(()),
        Fit::Auto | Fit::Bitrate => Err(fl!("bitrate", "fitted")),
        Fit::Crf | Fit::Fps | Fit::Length => Err(fl!("bitrate", "constant")),
    };
    let allowed = bitrate.as_ref().map_err(String::as_str).copied();
    row(ui, &fl!("bitrate"), &fl!("bitrate", "hint"), allowed, |ui| {
        let automatic = (fl!("bitrate-auto"), fl!("bitrate-auto", "hint"));
        let automatic = (automatic.0.as_str(), automatic.1.as_str());
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

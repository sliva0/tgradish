//! Size and quality for WebM: what fitting changes to get under the size
//! limit, and the values the encoder works from. Most of it only matters
//! for some ways of fitting, so the rest is greyed out rather than hidden.

use eframe::egui;
use tgradish_core::options::{Fit, FitUnit, Range, Speed};
use tgradish_core::webm::{
    CRFS, DEFAULT_ATTEMPTS, DEFAULT_CRF, HIGHEST_BITRATE, MAX_ATTEMPTS, Options,
};

use crate::config::Config;
use tgradish_core::presets::Presets;
use tgradish_core::{telegram, webm};

use super::Context;
use crate::item::{Choices, Item};
use crate::l10n::fl;
use crate::widgets::{
    self, Choice, Pick, chosen_hint, flag, greyed, preset_number, preset_picking, preset_segments,
    row, section, segments,
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

/// The way of fitting the item's choices use, its own or its preset's.
pub(crate) fn fit_of(choices: &Choices, presets: &Presets, config: &Config) -> Fit {
    let base = choices.webm_base(presets, config).unwrap_or_default();
    base.merged(&choices.webm).fit_or_default()
}

/// Drops the item's own range to search once fitting goes from `before`
/// to `after`, a way that counts in another unit: a range of bitrates is
/// no range of qualities, and kept it would be cut to the ends of the new
/// unit's, as 63 to 63. Fitting then searches its own for the new way. A
/// preset's range under the item's own way of fitting core drops itself,
/// as it merges them.
pub(crate) fn follow_fit(options: &mut Options, before: Fit, after: Fit) {
    if before.unit() != after.unit() {
        options.fit_range = None;
    }
}

pub(super) fn webm_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, &fl!("size-and-quality"));
    let base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let base_fit = base.fit_or_default();
    let options = &mut item.choices.webm;
    let fit = options.fit.unwrap_or(base_fit);
    // lossless keeps no bitrate or quality to fit, so only ways of fitting
    // that keep a quality go with it
    let keeps_quality = |fit: Fit| matches!(fit, Fit::Fps | Fit::Length | Fit::Off);
    let lossless = keeps_quality(fit) && base.clone().merged(options).lossless_or_default();
    row(ui, &fl!("fitting"), &fl!("fitting", "hint"), Ok(()), |ui| {
        let fits = fits();
        let pick = |fit: Fit| match lossless && !keeps_quality(fit) {
            true => Pick::Changes(fl!("fitting", "lossless")),
            false => Pick::Yes,
        };
        if preset_picking(ui, &mut options.fit, base_fit, &fits, pick)
            && lossless
            && !keeps_quality(options.fit.unwrap_or(base_fit))
        {
            options.lossless = (base.lossless == Some(true)).then_some(false);
        }
        let chosen = options.fit.unwrap_or(base_fit);
        follow_fit(options, fit, chosen);
        chosen_hint(ui, chosen, &fits);
    });
    row(ui, &fl!("encoder"), &fl!("encoder", "hint"), Ok(()), |ui| {
        let base = base.speed_or_default();
        preset_segments(ui, &mut options.speed, base, &webm_speeds());
    });

    // every row stays, greyed out where this way of fitting has no use
    // for it
    let off = fl!("fitting-off");
    let fitting = if fit == Fit::Off { Err(off.as_str()) } else { Ok(()) };
    row(ui, &fl!("attempts"), &fl!("attempts", "hint"), fitting, |ui| {
        let defaults = (fl!("attempts-default"), fl!("attempts-default", "hint"));
        let defaults = (defaults.0.as_str(), defaults.1.as_str());
        // the number alone in its field, which takes it typed, and the
        // word for it beside
        ui.horizontal(|ui| {
            let (value, default) = (&mut options.attempts, DEFAULT_ATTEMPTS);
            widgets::steady_numbers(ui, MAX_ATTEMPTS.to_string().len(), "", |ui| {
                preset_number(ui, value, base.attempts, defaults, Some(default), default, |value| {
                    value.range(1..=MAX_ATTEMPTS)
                })
            });
            let count = options.attempts.or(base.attempts).unwrap_or(default);
            ui.label(fl!("attempts-times", count = count));
        });
    });
    let bounds = RangeBounds::of(fit);
    // the preset's range, unless the item fits in another unit, which
    // drops it
    let inherited = base.clone().merged(&Options { fit: options.fit, ..Options::default() });
    let inherited = inherited.fit_range;
    let mut asked_custom = false;
    row(ui, &fl!("search-within"), &fl!("search-within", "hint"), fitting, |ui| {
        ui.horizontal(|ui| {
            let mut custom = options.fit_range.is_some();
            let automatic = match inherited {
                Some(_) => widgets::from_preset(),
                None => (fl!("search-auto"), fl!("search-auto", "hint")),
            };
            let choices = [(false, automatic.0, automatic.1), (true, fl!("custom"), String::new())];
            if segments(ui, &mut custom, &choices, |_| Ok(())) {
                if custom {
                    asked_custom = true;
                } else {
                    options.fit_range = None;
                }
            }
            // automatic, the range isn't shown: it is fitting's to choose,
            // unless the preset sets one
            let set = options.fit_range;
            let known = set.or(inherited);
            let mut shown = known.unwrap_or(Range { min: 0.0, max: 0.0 });
            let pick = fl!("custom", "hint");
            let (min, max) = (&mut shown.min, &mut shown.max);
            // each as wide as its unit's widest, so "to" and the other
            // stay put as one is dragged
            let widest = bounds.widest();
            let field = bounds.field(min, known.is_some(), set.is_some());
            let low = widgets::steady_width(ui, &widest, |ui| {
                widgets::number_field(ui, set.is_some(), field)
            })
            .on_disabled_hover_text(pick.as_str());
            ui.label(fl!("range-to"));
            let field = bounds.field(max, known.is_some(), set.is_some());
            let high = widgets::steady_width(ui, &widest, |ui| {
                widgets::number_field(ui, set.is_some(), field)
            })
            .on_disabled_hover_text(pick.as_str());
            if set.is_some() && (low.changed() || high.changed()) {
                shown.max = shown.max.max(shown.min);
                options.fit_range = Some(shown);
            }
        });
    });
    // a custom range starts as the preset's, or the one fitting would
    // search
    if asked_custom {
        let searched = || {
            let (fps, length) = planned(item, context)
                .map_or((30.0, telegram::MAX_SECONDS), |(_, plan)| (plan.fps, plan.length));
            webm::default_fit_range(fit, fps, length)
        };
        item.choices.webm.fit_range = Some(inherited.unwrap_or_else(searched));
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
            let defaults = (defaults.0.as_str(), defaults.1.as_str());
            let (value, default) = (&mut options.crf, DEFAULT_CRF);
            widgets::steady_numbers(ui, 2, "", |ui| {
                preset_number(ui, value, base.crf, defaults, Some(default), default, |value| {
                    value.range(CRFS)
                })
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
        let preset = base.lossless_or_default();
        let text = fl!("lossless-encoding");
        let changed = greyed(ui, !keeps_quality(fit), |ui| {
            flag(ui, &mut options.lossless, preset, &text, &hint)
        });
        if changed && options.lossless.unwrap_or(preset) && !keeps_quality(fit) {
            options.fit = (base_fit != Fit::Fps).then_some(Fit::Fps);
            follow_fit(options, fit, Fit::Fps);
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
        let digits = HIGHEST_BITRATE.to_string().len();
        widgets::steady_numbers(ui, digits, " kbit/s", |ui| {
            preset_number(ui, &mut options.bitrate, base.bitrate, automatic, None, 600.0, |value| {
                value.range(1.0..=HIGHEST_BITRATE).speed(5.0).max_decimals(0).suffix(" kbit/s")
            })
        });
    });
}

/// What the ends of the range fitting searches can be, as planning checks
/// them: whole frame rates up to Telegram's, libvpx's CRFs, and bitrates as
/// the bitrate field takes them. Lengths beyond the video's are cut to it.
struct RangeBounds {
    range: std::ops::RangeInclusive<f64>,
    /// What planning takes, which a preset or an earlier session may have
    /// set beyond what the field offers: kept until it is edited.
    takes: std::ops::RangeInclusive<f64>,
    decimals: usize,
    speed: f64,
    unit: &'static str,
}

impl RangeBounds {
    fn of(fit: Fit) -> Self {
        let takes = fit.range_limits();
        let (range, decimals, speed, unit) = match fit.unit() {
            FitUnit::Kbps => (1.0..=HIGHEST_BITRATE, 0, 5.0, " kbit/s"),
            FitUnit::Fps => (takes.clone(), 0, 0.1, " fps"),
            FitUnit::Seconds => (0.01..=3600.0, 2, 0.01, " s"),
            FitUnit::Crf => (takes.clone(), 0, 0.1, ""),
        };
        Self { range, takes, decimals, speed, unit }
    }

    /// The widest text a field of an end shows.
    fn widest(&self) -> String {
        let most = widgets::fixed(*self.range.end(), self.decimals);
        widgets::zeros(&most) + self.unit
    }

    /// A field of one end, shown as "—" unless it is `known`, as fitting
    /// chooses the range then; whole numbers without a decimal point, and
    /// seconds to the hundredth. Only a `live` one, the user's own, is
    /// brought into what planning takes: a preset's shows as it is.
    fn field<'a>(&self, value: &'a mut f64, known: bool, live: bool) -> egui::DragValue<'a> {
        let refused = live && !self.takes.contains(value);
        let field = egui::DragValue::new(value)
            .range(self.range.clone())
            .clamp_existing_to_range(refused)
            .speed(self.speed)
            .max_decimals(self.decimals);
        if known {
            let decimals = self.decimals;
            field
                .suffix(self.unit)
                .custom_formatter(move |value, _| widgets::fixed(value, decimals))
        } else {
            field.custom_formatter(|_, _| "—".to_owned())
        }
    }
}

/// What converting the item to WebM would ask for, and plans to make.
pub(super) fn planned(
    item: &Item,
    context: &Context,
) -> Result<(webm::Request, webm::Plan), String> {
    let video = item.video.ready().ok_or_else(|| fl!("no-ffmpeg-commands", "unread"))?;
    let options = item.choices.webm_options(context.presets, context.config)?;
    let request = webm::Request {
        input: item.input(),
        output: context.output.clone(),
        options,
        overwrite: true,
        keep_temp: false,
    };
    let (plan, _) = webm::plan(&request, video.probe.clone()).map_err(|err| err.to_string())?;
    Ok((request, plan))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What showing a range's end leaves of it.
    fn shown(fit: Fit, mut value: f64) -> f64 {
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            ui.add(RangeBounds::of(fit).field(&mut value, true, true));
        });
        harness.run();
        drop(harness);
        value
    }

    #[test]
    fn drops_a_custom_range_once_fitting_counts_another_way() {
        use crate::config::Config;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        use tgradish_core::presets::Presets;
        use tgradish_core::telegram::Target;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("short.toml"), "[webm]\nfit = 'length'\n").unwrap();
        let (presets, config) = (Presets::load(Some(dir.path())).unwrap(), Config::default());
        let context = Context {
            presets: &presets,
            config: &config,
            ffmpeg: config.ffmpeg.clone(),
            ffmpeg_args: true,
            output: None,
            config_path: None,
            presets_dir: None,
            in_frames: false,
        };
        let mut item = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        item.choices.webm.fit = Some(Fit::Bitrate);
        let bitrates = Some(Range { min: 200.0, max: 1500.0 });
        item.choices.webm.fit_range = bitrates;
        let mut harness = Harness::new_ui_state(
            |ui, item: &mut Item| {
                super::super::make(ui, item, &context);
                webm_quality(ui, item, &context);
            },
            item,
        );
        harness.run_steps(2);
        let range = |harness: &Harness<Item>| harness.state().choices.webm.fit_range;
        // a way that counts bitrates too keeps it
        harness.get_by_label(&fl!("fit-auto")).click();
        harness.run_steps(2);
        assert_eq!(range(&harness), bitrates);
        // qualities don't: none of its own, rather than 63 to 63
        harness.get_by_label(&fl!("fit-crf")).click();
        harness.run_steps(2);
        assert_eq!(harness.state().choices.webm.fit, Some(Fit::Crf));
        assert_eq!(range(&harness), None);
        // nor does a preset fitting the length, once the item's way is gone
        harness.state_mut().choices.webm = Options { fit_range: bitrates, ..Options::default() };
        harness.run_steps(2);
        harness.get_by_label("short").click();
        harness.run_steps(2);
        assert_eq!(harness.state().choices.preset.as_deref(), Some("short"));
        assert_eq!(range(&harness), None);
    }

    #[test]
    fn showing_a_range_keeps_what_planning_takes() {
        // beyond what the fields offer, yet fine to convert with
        assert_eq!(shown(Fit::Length, 0.005), 0.005);
        assert_eq!(shown(Fit::Bitrate, 60_000.0), 60_000.0);
        assert_eq!(shown(Fit::Fps, 24.0), 24.0);
        // what planning would refuse comes into range
        assert_eq!(shown(Fit::Fps, -2257.0), 1.0);
        assert_eq!(shown(Fit::Fps, 507.0), telegram::MAX_FPS);
        assert_eq!(shown(Fit::Length, -1.0), 0.01);
    }
}

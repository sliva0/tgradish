//! Time, for videos and animations: how fast and how smoothly the result
//! plays, and for WebM what Telegram is told of its length. A picture
//! made from a video has only the frame it is made of.

use eframe::egui;
use tgradish_core::options::{Spoof, TEMPOS};
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs;
use tgradish_core::webm::DEFAULT_FAKE_DURATION;

use super::Context;
use crate::item::{Item, Kind};
use crate::l10n::fl;
use crate::widgets::{self, Choice, chosen_hint, preset_number, preset_segments, row, section};
use crate::{cuts, timeline};

fn spoofs() -> [Choice<Spoof>; 3] {
    [
        (Spoof::Auto, fl!("spoof-auto"), fl!("spoof-auto", "hint")),
        (Spoof::Always, fl!("spoof-always"), fl!("spoof-always", "hint")),
        (Spoof::Never, fl!("spoof-never"), fl!("spoof-never", "hint")),
    ]
}

/// The time of a video or a `.tgs` animation. A picture has only its
/// frame, which [`frame`] shows instead.
pub(super) fn time(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let tgs = item.format == Format::Tgs;
    let animated = item.input_length().is_some();
    // what the preset sets, which the item's own values go over
    let webm_base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let tgs_base = item.choices.tgs_base(context.presets, context.config).unwrap_or_default();
    let sheet = item.choices.tgs.sheet.is_some();
    let sequence = tgs && (item.sequence || sheet);
    if !animated && !sequence {
        return;
    }
    section(ui, &fl!("time"));
    // how long the used part lasts, and plays
    let used = item.range().map(|(start, end)| end - start);
    let tempo = item.tempo(context.presets, context.config);
    let played = used.map(|used| used / tempo);
    // the average rate, and the most a video of variable rate reaches
    let input_fps =
        item.video.ready().filter(|_| !tgs).map(|video| (video.probe.fps, video.probe.base_fps));
    let spoof = webm_base.clone().merged(&item.choices.webm).spoof_or_default();
    // a preset's tempo is what the automatic choice stands for
    let preset_tempo = if tgs { tgs_base.tempo } else { webm_base.tempo };
    if let Some(length) = item.input_length() {
        row(ui, &fl!("used-part"), &fl!("used-part", "hint"), Ok(()), |ui| {
            used_part(ui, item, length, context.in_frames)
        });
    }
    if animated {
        row(ui, &fl!("speed"), &fl!("speed", "hint"), Ok(()), |ui| {
            ui.horizontal(|ui| {
                // a .tgs plays a longer part fast enough to fit 3 s
                let fitted = used.is_some_and(telegram::too_long);
                let input = (fl!("from-input"), fl!("from-input", "speed"));
                let fits = (fl!("fits-3-s"), fl!("fits-3-s", "hint"));
                let (value, automatic) = match tgs {
                    false => (&mut item.choices.webm.tempo, input),
                    true if fitted => (&mut item.choices.tgs.tempo, fits),
                    true => (&mut item.choices.tgs.tempo, input),
                };
                let automatic = (automatic.0.as_str(), automatic.1.as_str());
                speed_field(ui, value, preset_tempo, automatic, tempo);
                if !tgs
                    && let Some(used) = used
                    && ui.button(fl!("fit-3-s")).on_hover_text(fl!("fit-3-s", "hint")).clicked()
                {
                    item.choices.webm.tempo =
                        Some((used / telegram::MAX_SECONDS).clamp(*TEMPOS.start(), *TEMPOS.end()));
                }
            });
            // .tgs results, and honest WebM ones, last 3 seconds at most;
            // their lines as they are as the speed is dragged, with room for
            // as long as the slowest makes it
            let cut = tgs || spoof == Spoof::Never;
            let used = used.unwrap_or_default();
            let longest = widgets::widest_seconds(used / TEMPOS.start());
            match played {
                Some(played) if cut && telegram::too_long(played) => {
                    let kept = widgets::widest_seconds(telegram::MAX_SECONDS * TEMPOS.end());
                    let used_text = widgets::seconds(used);
                    let widest: Vec<String> = kept
                        .iter()
                        .map(|kept| {
                            fl!("plays-cut", kept = kept.as_str(), used = used_text.as_str())
                        })
                        .collect();
                    let kept = widgets::seconds(telegram::MAX_SECONDS * tempo);
                    let text = fl!("plays-cut", kept = kept, used = used_text);
                    widgets::steady_hint(ui, text, &widest);
                }
                Some(played) => {
                    let widest: Vec<String> =
                        longest.iter().map(|most| fl!("plays", played = most.as_str())).collect();
                    let text = fl!("plays", played = widgets::seconds(played));
                    widgets::steady_hint(ui, text, &widest);
                }
                None => {}
            }
        });
    }
    match item.format {
        Format::Webm => {
            let options = &mut item.choices.webm;
            // as converting chooses it, and more would repeat the input's
            // frames
            let (average, base) = input_fps.unwrap_or_default();
            let chosen = average.map(|fps| tgradish_core::webm::played_fps(fps, tempo));
            let peak = match (average, base) {
                (Some(average), Some(base)) => Some(average.max(base)),
                (average, base) => average.or(base),
            };
            let most = peak.map(|fps| tgradish_core::webm::played_fps(fps, tempo));
            if item.kind == Kind::Frames {
                // how fast its frames play, which the result's follows
                row(ui, &fl!("frame-rate"), &fl!("frame-rate", "sequence"), Ok(()), |ui| {
                    sequence_rate(ui, &mut options.fps, webm_base.fps);
                });
            } else {
                let highest = most.unwrap_or(telegram::MAX_FPS);
                row(ui, &fl!("frame-rate"), &fl!("frame-rate", "hint"), Ok(()), |ui| {
                    let most = highest.min(30.0);
                    let automatic = (fl!("from-input"), fl!("from-input", "rate"));
                    let automatic = (automatic.0.as_str(), automatic.1.as_str());
                    let (value, preset) = (&mut options.fps, webm_base.fps);
                    rate_field(ui, value, preset, automatic, chosen, (most, 1.0..=highest));
                });
            }
            row(ui, &fl!("over-3-s"), &fl!("over-3-s", "hint"), Ok(()), |ui| {
                let spoofs = spoofs();
                let preset = webm_base.spoof_or_default();
                preset_segments(ui, &mut options.spoof, preset, &spoofs);
                let spoof = options.spoof.unwrap_or(preset);
                chosen_hint(ui, spoof, &spoofs);
                if let Some(played) = played
                    && spoof == Spoof::Auto
                    && telegram::too_long(played)
                {
                    let most = used.map_or(played, |used| used / TEMPOS.start());
                    let widest: Vec<String> = widgets::widest_seconds(most)
                        .iter()
                        .map(|most| fl!("plays-spoofed", played = most.as_str()))
                        .collect();
                    let played = widgets::seconds(played);
                    widgets::steady_hint(ui, fl!("plays-spoofed", played = played), &widest);
                }
            });
            let honest = options.spoof.or(webm_base.spoof) == Some(Spoof::Never);
            let why = fl!("spoof-to", "honest");
            let allowed = if honest { Err(why.as_str()) } else { Ok(()) };
            row(ui, &fl!("spoof-to"), &fl!("spoof-to", "hint"), allowed, |ui| {
                let custom = options.fake_duration.is_some();
                let automatic = (fl!("default-duration"), fl!("default-duration", "hint"));
                let automatic = (automatic.0.as_str(), automatic.1.as_str());
                let changed = widgets::steady_width(ui, "0.00 s", |ui| {
                    preset_number(
                        ui,
                        &mut options.fake_duration,
                        webm_base.fake_duration,
                        automatic,
                        Some(DEFAULT_FAKE_DURATION),
                        telegram::MAX_SECONDS,
                        |value| {
                            value
                                .range(0.01..=telegram::MAX_SECONDS)
                                .speed(0.01)
                                .max_decimals(2)
                                .suffix(" s")
                                .custom_formatter(|n, _| widgets::fixed(n, 2))
                        },
                    )
                });
                // one of your own starts at the most Telegram takes, rather
                // than at the default's odd number
                if changed && !custom && options.fake_duration.is_some() {
                    options.fake_duration = Some(telegram::MAX_SECONDS);
                }
            });
        }
        Format::Tgs if sequence => {
            row(ui, &fl!("frame-rate"), &fl!("frame-rate", "sheet"), Ok(()), |ui| {
                sequence_rate(ui, &mut item.choices.tgs.fps, tgs_base.fps);
            });
        }
        // pictures never come here
        Format::Tgs | Format::Webp => {}
    }
}

/// Where the used part starts and ends, typed: as the timeline counts,
/// frames or seconds, with the other count under them.
fn used_part(ui: &mut egui::Ui, item: &mut Item, length: f64, in_frames: bool) {
    let frames = item.input_frames();
    let choices = &mut item.choices;
    let size = egui::vec2(72.0, ui.spacing().interact_size.y);
    let mut counted = None;
    ui.horizontal(|ui| {
        match frames.as_deref() {
            Some(frames) => {
                let (mut first, mut after) = frames.part(length, choices.start, choices.length);
                ui.label(fl!("part-from"));
                let field =
                    timeline::boundary_field(&mut first, 0..=after - 1, frames, in_frames, false);
                let start = ui.add_sized(size, field).on_hover_text(fl!("part-first")).changed();
                ui.label(fl!("part-to"));
                let range = first + 1..=frames.count();
                let field = timeline::boundary_field(&mut after, range, frames, in_frames, true);
                let end = ui.add_sized(size, field).on_hover_text(fl!("part-last")).changed();
                if start || end {
                    frames.set_part((first, after), &mut choices.start, &mut choices.length);
                }
                let (first, after) = frames.part(length, choices.start, choices.length);
                let count = frames.count();
                // with room for the widest numbers the part may have, so
                // its lines stay as they are as it is dragged
                let most = widgets::widest_seconds(length).swap_remove(0);
                let longest = widgets::zeros(&count.to_string());
                counted = Some(if in_frames {
                    let text = fl!(
                        "part-seconds",
                        from = widgets::seconds(frames.times[first]),
                        to = widgets::seconds(frames.times[after]),
                        length = widgets::seconds(length)
                    );
                    let widest = fl!(
                        "part-seconds",
                        from = most.as_str(),
                        to = most.as_str(),
                        length = widgets::seconds(length)
                    );
                    (text, widest)
                } else if after - first == 1 {
                    let number = first + 1;
                    let widest = fl!("part-frame", number = longest.as_str(), count = count);
                    (fl!("part-frame", number = number, count = count), widest)
                } else {
                    let first = first + 1;
                    let widest = fl!(
                        "part-frames",
                        first = longest.as_str(),
                        last = longest.as_str(),
                        count = count
                    );
                    (fl!("part-frames", first = first, last = after, count = count), widest)
                });
            }
            None => {
                let (mut from, mut to) = cuts::part(length, choices.start, choices.length);
                ui.label(fl!("part-from"));
                let start = ui.add_sized(size, timeline::seconds(&mut from, length)).changed();
                ui.label(fl!("part-to"));
                let end = ui.add_sized(size, timeline::seconds(&mut to, length)).changed();
                if start || end {
                    let from = from.min(to - 0.01).max(0.0);
                    let to = to.max(from + 0.01).min(length);
                    cuts::set_part(length, (from, to), &mut choices.start, &mut choices.length);
                }
            }
        }
        let cut = choices.start.is_some() || choices.length.is_some();
        if widgets::reset_to(ui, cut, &fl!("all-of-input")) {
            (choices.start, choices.length) = (None, None);
        }
    });
    if let Some((counted, widest)) = counted {
        widgets::steady_hint(ui, counted, &[widest]);
    }
}

/// Which frame of a video an image sticker is made of, as the timeline
/// counts: by its number, or where it starts, with the other under it.
pub(super) fn frame(ui: &mut egui::Ui, item: &mut Item, in_frames: bool) {
    let Some(length) = item.input_length() else { return };
    section(ui, &fl!("frame"));
    row(ui, &fl!("frame-at"), &fl!("frame-at", "hint"), Ok(()), |ui| {
        let Some(frames) = item.input_frames() else { return };
        let mut index = frames.at(item.choices.frame.unwrap_or(0.0));
        let size = egui::vec2(72.0, ui.spacing().interact_size.y);
        let field = timeline::frame_field(&mut index, &frames, in_frames);
        if ui.add_sized(size, field).changed() {
            // where its frame starts, as the picture is made of it
            item.choices.frame = (index > 0).then_some(frames.times[index]);
        }
        let index = frames.at(item.choices.frame.unwrap_or(0.0));
        // as it is dragged, its lines as they are
        let (text, widest) = if in_frames {
            let most = widgets::widest_seconds(length).swap_remove(0);
            let length = widgets::seconds(length);
            let text = fl!(
                "part-seconds",
                from = widgets::seconds(frames.times[index]),
                to = widgets::seconds(frames.times[index + 1]),
                length = length.as_str()
            );
            let widest =
                fl!("part-seconds", from = most.as_str(), to = most.as_str(), length = length);
            (text, widest)
        } else {
            let (number, count) = (index + 1, frames.count());
            let longest = widgets::zeros(&count.to_string());
            let widest = fl!("part-frame", number = longest, count = count);
            (fl!("part-frame", number = number, count = count), widest)
        };
        widgets::steady_hint(ui, text, &[widest]);
    });
}

/// How fast frames that don't say it play: the `preset`'s rate, or else
/// the default one, unless set.
fn sequence_rate(ui: &mut egui::Ui, fps: &mut Option<f64>, preset: Option<f64>) {
    let default = tgs::DEFAULT_FPS;
    let hint = fl!("default-rate", "hint", fps = widgets::trimmed(default));
    let automatic = (fl!("default-rate"), hint);
    let automatic = (automatic.0.as_str(), automatic.1.as_str());
    let range = 0.1..=telegram::MAX_FPS;
    rate_field(ui, fps, preset, automatic, Some(default), (default, range));
}

/// The widest a speed's field shows, at most ×10.
const WIDEST_SPEED: &str = "×00.00";
/// The widest a frame rate's field shows, at most 60 fps.
const WIDEST_RATE: &str = "00.00 fps";

/// A speed, ×1.50, as automatic `shown` or the `preset`'s unless set: its
/// field as wide as the widest speed, so what follows stays put as it is
/// dragged.
pub(crate) fn speed_field(
    ui: &mut egui::Ui,
    value: &mut Option<f64>,
    preset: Option<f64>,
    automatic: (&str, &str),
    shown: f64,
) -> bool {
    widgets::steady_width(ui, WIDEST_SPEED, |ui| {
        preset_number(ui, value, preset, automatic, Some(shown), 1.0, |value| {
            value
                .range(TEMPOS)
                .speed(0.01)
                .max_decimals(widgets::SPEED_DECIMALS)
                .prefix("×")
                .custom_formatter(|n, _| widgets::speed(n))
        })
    })
}

/// A frame rate, 29.97 fps, within `range`, as automatic `shown` or the
/// `preset`'s unless set; one of the user's own starts at `start`. Its
/// field as wide as the widest rate.
fn rate_field(
    ui: &mut egui::Ui,
    value: &mut Option<f64>,
    preset: Option<f64>,
    automatic: (&str, &str),
    shown: Option<f64>,
    (start, range): (f64, std::ops::RangeInclusive<f64>),
) -> bool {
    widgets::steady_width(ui, WIDEST_RATE, |ui| {
        preset_number(ui, value, preset, automatic, shown, start, |value| {
            value
                .range(range)
                .speed(0.1)
                .max_decimals(widgets::FPS_DECIMALS)
                .suffix(" fps")
                .custom_formatter(|v, _| widgets::fps(v))
        })
    })
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::kittest::Queryable;
    use tgradish_core::ffmpeg::FfmpegChoice;

    use crate::testing::{finish, harness, video_of};

    #[test]
    fn keeps_what_follows_changing_numbers_in_place() {
        use eframe::egui;
        use egui_kittest::Harness;
        // a speed of the user's own, then the rate it plays at, as the time
        // section has them, with a button after each, and a hint under them
        // whose seconds follow the speed
        type State = (Option<f64>, Option<f64>, Option<egui::Rect>, Option<egui::Rect>, f32);
        let show = |ui: &mut egui::Ui, state: &mut State| {
            ui.set_width(260.0);
            ui.horizontal(|ui| {
                super::speed_field(ui, &mut state.0, None, ("Input's", ""), 1.0);
                state.2 = Some(ui.button("Fit 3 s").rect);
            });
            ui.horizontal(|ui| {
                let chosen = state.1;
                super::rate_field(
                    ui,
                    &mut state.1,
                    None,
                    ("Input's", ""),
                    chosen,
                    (30.0, 1.0..=60.0),
                );
                state.3 = Some(ui.button("after").rect);
            });
            let played = 14.55 / state.0.unwrap_or(1.0);
            let widest: Vec<String> = crate::widgets::widest_seconds(14.55 / 0.1)
                .iter()
                .map(|most| format!("Plays all {most}: Telegram is told it is shorter than it is, so it takes it"))
                .collect();
            let text = format!(
                "Plays all {}: Telegram is told it is shorter than it is, so it takes it",
                crate::widgets::seconds(played)
            );
            let before = ui.cursor().top();
            crate::widgets::steady_hint(ui, text, &widest);
            state.4 = ui.cursor().top() - before;
        };
        let mut harness = Harness::new_ui_state(show, (Some(1.0), None, None, None, 0.0));
        crate::widgets::test_fonts(&harness);
        harness.run_steps(2);
        let mut seen = None;
        // as many digits before the point as after, and more and fewer
        for (speed, rate) in [(1.0, 30.0), (1.73, 51.9), (0.5, 9.97), (10.0, 60.0), (0.1, 1.0)] {
            (harness.state_mut().0, harness.state_mut().1) = (Some(speed), Some(rate));
            harness.run_steps(2);
            let state = harness.state();
            let placed = (state.2.unwrap(), state.3.unwrap(), state.4);
            assert_eq!(*seen.get_or_insert(placed), placed, "×{speed}, {rate} fps");
        }
    }

    #[test]
    fn warns_before_spoofing() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video_of(dir.path(), "64x48", 4) else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        harness.get_by_label_contains("Telegram is told it is shorter");
        harness.get_by_label("Spoof to");
        // cut to 3 seconds, or not spoofed at all: no warning
        harness.state_mut().items[0].choices.length = Some(3.0);
        harness.run_steps(2);
        assert!(harness.query_by_label_contains("Telegram is told it is shorter").is_none());
        harness.state_mut().items[0].choices.length = None;
        harness.state_mut().items[0].choices.webm.spoof =
            Some(tgradish_core::options::Spoof::Never);
        harness.run_steps(2);
        assert!(harness.query_by_label_contains("Telegram is told it is shorter").is_none());
    }
}

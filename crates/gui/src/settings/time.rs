//! Time, for videos and animations: how fast and how smoothly the result
//! plays, and for WebM what Telegram is told of its length. A picture
//! made from a video has only the frame it is made of.

use eframe::egui;
use tgradish_core::options::{DEFAULT_FAKE_DURATION, Spoof, TEMPOS};
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs;

use super::Context;
use crate::item::{Item, Kind};
use crate::l10n::fl;
use crate::timeline;
use crate::widgets::{
    self, Choice, chosen_hint, note, preset_number, preset_segments, row, section,
};

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
    let sheet = item.choices.tgs.sheet.is_some() || tgs_base.sheet.is_some();
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
    let spoof = item.choices.webm.spoof.or(webm_base.spoof).unwrap_or(Spoof::Auto);
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
                preset_number(ui, value, preset_tempo, automatic, Some(tempo), 1.0, |value| {
                    value.range(TEMPOS).speed(0.01).max_decimals(2).prefix("×")
                });
                if !tgs
                    && let Some(used) = used
                    && ui.button(fl!("fit-3-s")).on_hover_text(fl!("fit-3-s", "hint")).clicked()
                {
                    item.choices.webm.tempo =
                        Some((used / telegram::MAX_SECONDS).clamp(*TEMPOS.start(), *TEMPOS.end()));
                }
            });
            // .tgs results, and honest WebM ones, last 3 seconds at most
            let cut = tgs || spoof == Spoof::Never;
            match played {
                Some(played) if cut && telegram::too_long(played) => note(
                    ui,
                    fl!(
                        "plays-cut",
                        kept = widgets::seconds(telegram::MAX_SECONDS * tempo),
                        used = widgets::seconds(used.unwrap_or_default())
                    ),
                ),
                Some(played) => note(ui, fl!("plays", played = widgets::seconds(played))),
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
            let chosen = average.map(|fps| (fps * tempo).clamp(1.0, telegram::MAX_FPS));
            let peak = match (average, base) {
                (Some(average), Some(base)) => Some(average.max(base)),
                (average, base) => average.or(base),
            };
            let most = peak.map(|fps| (fps * tempo).clamp(1.0, telegram::MAX_FPS));
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
                    preset_number(ui, value, preset, automatic, chosen, most, |value| {
                        value
                            .range(1.0..=highest)
                            .speed(0.1)
                            .max_decimals(2)
                            .suffix(" fps")
                            .custom_formatter(|v, _| widgets::trimmed(v))
                    });
                });
            }
            row(ui, &fl!("over-3-s"), &fl!("over-3-s", "hint"), Ok(()), |ui| {
                let spoofs = spoofs();
                let preset = webm_base.spoof.unwrap_or(Spoof::Auto);
                preset_segments(ui, &mut options.spoof, preset, &spoofs);
                let spoof = options.spoof.unwrap_or(preset);
                chosen_hint(ui, spoof, &spoofs);
                if let Some(played) = played
                    && spoof == Spoof::Auto
                    && telegram::too_long(played)
                {
                    let played = widgets::seconds(played);
                    note(ui, fl!("plays-spoofed", played = played));
                }
            });
            let honest = options.spoof.or(webm_base.spoof) == Some(Spoof::Never);
            let why = fl!("spoof-to", "honest");
            let allowed = if honest { Err(why.as_str()) } else { Ok(()) };
            row(ui, &fl!("spoof-to"), &fl!("spoof-to", "hint"), allowed, |ui| {
                let custom = options.fake_duration.is_some();
                let automatic = (fl!("default-duration"), fl!("default-duration", "hint"));
                let automatic = (automatic.0.as_str(), automatic.1.as_str());
                let changed = preset_number(
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
                    },
                );
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
                counted = Some(if in_frames {
                    fl!(
                        "part-seconds",
                        from = widgets::seconds(frames.times[first]),
                        to = widgets::seconds(frames.times[after]),
                        length = widgets::seconds(length)
                    )
                } else if after - first == 1 {
                    let number = first + 1;
                    fl!("part-frame", number = number, count = count)
                } else {
                    let first = first + 1;
                    fl!("part-frames", first = first, last = after, count = count)
                });
            }
            None => {
                let (mut from, mut to) = timeline::part(length, choices.start, choices.length);
                ui.label(fl!("part-from"));
                let start = ui.add_sized(size, timeline::seconds(&mut from, length)).changed();
                ui.label(fl!("part-to"));
                let end = ui.add_sized(size, timeline::seconds(&mut to, length)).changed();
                if start || end {
                    let from = from.min(to - 0.01).max(0.0);
                    let to = to.max(from + 0.01).min(length);
                    timeline::set_part(length, (from, to), &mut choices.start, &mut choices.length);
                }
            }
        }
        if (choices.start.is_some() || choices.length.is_some())
            && widgets::reset_to(ui, &fl!("all-of-input"))
        {
            (choices.start, choices.length) = (None, None);
        }
    });
    if let Some(counted) = counted {
        note(ui, counted);
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
        note(
            ui,
            if in_frames {
                fl!(
                    "part-seconds",
                    from = widgets::seconds(frames.times[index]),
                    to = widgets::seconds(frames.times[index + 1]),
                    length = widgets::seconds(length)
                )
            } else {
                let (number, count) = (index + 1, frames.count());
                fl!("part-frame", number = number, count = count)
            },
        );
    });
}

/// How fast frames that don't say it play: the `preset`'s rate, or else
/// the default one, unless set.
fn sequence_rate(ui: &mut egui::Ui, fps: &mut Option<f64>, preset: Option<f64>) {
    let default = tgs::DEFAULT_FPS;
    let hint = fl!("default-rate", "hint", fps = widgets::trimmed(default));
    let automatic = (fl!("default-rate"), hint);
    let automatic = (automatic.0.as_str(), automatic.1.as_str());
    preset_number(ui, fps, preset, automatic, Some(default), default, |value| {
        value
            .range(0.1..=60.0)
            .speed(0.1)
            .max_decimals(2)
            .suffix(" fps")
            .custom_formatter(|v, _| widgets::trimmed(v))
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tgradish_core::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;

    use crate::testing::{finish, harness, video_of};

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

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
use crate::widgets::{self, Choice, auto_number, chosen_hint, note, preset_segments, row, section};

fn spoofs() -> [Choice<Spoof>; 3] {
    [
        (Spoof::Auto, fl!("spoof-auto"), fl!("spoof-auto", "hint")),
        (Spoof::Always, fl!("spoof-always"), fl!("spoof-always", "hint")),
        (Spoof::Never, fl!("spoof-never"), fl!("spoof-never", "hint")),
    ]
}

pub(super) fn time(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let animated = item.input_length().is_some();
    let sequence =
        item.format == Format::Tgs && (item.sequence || item.choices.tgs.sheet.is_some());
    if !animated && !sequence {
        return;
    }
    section(ui, &fl!("time"));
    // how long the used part lasts, and plays
    let used = item.range().map(|(start, end)| end - start);
    let tempo = item.tempo(context.presets, context.config);
    let played = used.map(|used| used / tempo);
    // the average rate, and the most a video of variable rate reaches
    let input_fps = match item.format {
        Format::Webm | Format::Image => {
            item.video.ready().map(|video| (video.probe.fps, video.probe.base_fps))
        }
        Format::Tgs => None,
    };
    // what the preset sets, which the item's own values go over
    let webm_base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let spoof = item.choices.webm.spoof.or(webm_base.spoof).unwrap_or(Spoof::Auto);
    // a preset's tempo is what the automatic choice stands for
    let preset_tempo = match item.format {
        Format::Webm | Format::Image => webm_base.tempo,
        Format::Tgs => {
            item.choices.tgs_base(context.presets, context.config).ok().and_then(|base| base.tempo)
        }
    };
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
                let preset = (fl!("from-preset"), fl!("from-preset", "speed"));
                let input = (fl!("from-input"), fl!("from-input", "speed"));
                let fits = (fl!("fits-3-s"), fl!("fits-3-s", "hint"));
                let (value, automatic) = match (item.format, preset_tempo) {
                    (Format::Tgs, Some(_)) => (&mut item.choices.tgs.tempo, preset),
                    (Format::Webm | Format::Image, Some(_)) => {
                        (&mut item.choices.webm.tempo, preset)
                    }
                    (Format::Webm | Format::Image, None) => (&mut item.choices.webm.tempo, input),
                    (Format::Tgs, None) if fitted => (&mut item.choices.tgs.tempo, fits),
                    (Format::Tgs, None) => (&mut item.choices.tgs.tempo, input),
                };
                let automatic = (automatic.0.as_str(), automatic.1.as_str());
                auto_number(ui, value, automatic, Some(tempo), 1.0, |value| {
                    value.range(TEMPOS).speed(0.01).max_decimals(2).prefix("×")
                });
                if item.format == Format::Webm
                    && let Some(used) = used
                    && ui.button(fl!("fit-3-s")).on_hover_text(fl!("fit-3-s", "hint")).clicked()
                {
                    item.choices.webm.tempo =
                        Some((used / telegram::MAX_SECONDS).clamp(*TEMPOS.start(), *TEMPOS.end()));
                }
            });
            // .tgs results, and honest WebM ones, last 3 seconds at most
            let cut = item.format == Format::Tgs || spoof == Spoof::Never;
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
        Format::Webm | Format::Image => {
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
                    let default = Some(tgs::DEFAULT_FPS);
                    let automatic = (fl!("default-rate"), fl!("default-rate", "hint"));
                    auto_number(
                        ui,
                        &mut options.fps,
                        (&automatic.0, &automatic.1),
                        default,
                        tgs::DEFAULT_FPS,
                        |value| value.range(0.1..=60.0).speed(0.1).max_decimals(2).suffix(" fps"),
                    );
                });
            } else {
                let highest = most.unwrap_or(telegram::MAX_FPS);
                row(ui, &fl!("frame-rate"), &fl!("frame-rate", "hint"), Ok(()), |ui| {
                    let most = highest.min(30.0);
                    let automatic = (fl!("from-input"), fl!("from-input", "rate"));
                    let automatic = (automatic.0.as_str(), automatic.1.as_str());
                    auto_number(ui, &mut options.fps, automatic, chosen, most, |value| {
                        value.range(1.0..=highest).speed(0.1).max_decimals(2).suffix(" fps")
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
                let (automatic, shown) = match webm_base.fake_duration {
                    Some(duration) => {
                        ((fl!("from-preset"), fl!("from-preset", "duration")), duration)
                    }
                    None => (
                        (fl!("default-duration"), fl!("default-duration", "hint")),
                        DEFAULT_FAKE_DURATION,
                    ),
                };
                let automatic = (automatic.0.as_str(), automatic.1.as_str());
                let changed = auto_number(
                    ui,
                    &mut options.fake_duration,
                    automatic,
                    Some(shown),
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
                let default = Some(tgs::DEFAULT_FPS);
                let automatic = (fl!("default-rate"), fl!("default-rate", "hint"));
                auto_number(
                    ui,
                    &mut item.choices.tgs.fps,
                    (&automatic.0, &automatic.1),
                    default,
                    tgs::DEFAULT_FPS,
                    |value| value.range(0.1..=60.0).speed(0.1).max_decimals(2).suffix(" fps"),
                );
            });
        }
        Format::Tgs => {}
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

/// Which frame of a video an image sticker is made of.
pub(super) fn frame(ui: &mut egui::Ui, item: &mut Item) {
    let Some(length) = item.input_length() else { return };
    section(ui, &fl!("frame"));
    row(ui, &fl!("frame-at"), &fl!("frame-at", "hint"), Ok(()), |ui| {
        let Some(frames) = item.input_frames() else { return };
        let mut at = item.choices.frame.unwrap_or(0.0);
        let field = egui::DragValue::new(&mut at).range(0.0..=length).speed(0.01).max_decimals(3);
        if ui.add(field.suffix(" s")).changed() {
            // where its frame starts, as the picture is made of it
            let index = frames.at(at);
            item.choices.frame = (index > 0).then_some(frames.times[index]);
        }
        let index = frames.at(item.choices.frame.unwrap_or(0.0));
        let (number, count) = (index + 1, frames.count());
        note(ui, fl!("part-frame", number = number, count = count));
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

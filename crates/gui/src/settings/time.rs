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
use crate::timeline;
use crate::widgets::{self, Choice, auto_number, chosen_hint, note, preset_segments, row, section};

const SPOOFS: [Choice<Spoof>; 3] = [
    (
        Spoof::Auto,
        "Spoof when longer",
        "Longer stickers get a short duration written into them: Telegram accepts them and plays all of it",
    ),
    (Spoof::Always, "Always spoof", "Even short stickers get the short duration"),
    (Spoof::Never, "Cut at 3 s", "Keep the file honest and cut it at 3 seconds"),
];

pub(super) fn time(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let animated = item.input_length().is_some();
    let sequence =
        item.format == Format::Tgs && (item.sequence || item.choices.tgs.sheet.is_some());
    if !animated && !sequence {
        return;
    }
    section(ui, "Time");
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
        let hint = "Where the part of the input used starts and ends; the bar under the preview \
                    sets it too";
        row(ui, "Used part", hint, Ok(()), |ui| used_part(ui, item, length, context.in_frames));
    }
    if animated {
        let hint = "How many times as fast as the input the result plays";
        row(ui, "Speed", hint, Ok(()), |ui| {
            ui.horizontal(|ui| {
                // a .tgs plays a longer part fast enough to fit 3 s
                let fitted = used.is_some_and(telegram::too_long);
                let (value, automatic) = match (item.format, preset_tempo) {
                    (Format::Tgs, Some(_)) => {
                        (&mut item.choices.tgs.tempo, ("Preset's", "The speed the preset sets"))
                    }
                    (Format::Webm | Format::Image, Some(_)) => {
                        (&mut item.choices.webm.tempo, ("Preset's", "The speed the preset sets"))
                    }
                    (Format::Webm | Format::Image, None) => {
                        (&mut item.choices.webm.tempo, ("Input's", "As fast as the input plays"))
                    }
                    (Format::Tgs, None) if fitted => (
                        &mut item.choices.tgs.tempo,
                        ("Fits 3 s", "As fast as fits the used part into 3 seconds"),
                    ),
                    (Format::Tgs, None) => {
                        (&mut item.choices.tgs.tempo, ("Input's", "As fast as the input plays"))
                    }
                };
                auto_number(ui, value, automatic, Some(tempo), 1.0, |value| {
                    value.range(TEMPOS).speed(0.01).max_decimals(2).prefix("×")
                });
                if item.format == Format::Webm
                    && let Some(used) = used
                    && ui
                        .button("Fit 3 s")
                        .on_hover_text("Plays the used part in 3 seconds")
                        .clicked()
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
                    format!(
                        "Plays {} of the {}: the rest is cut off",
                        widgets::seconds(telegram::MAX_SECONDS * tempo),
                        widgets::seconds(used.unwrap_or_default())
                    ),
                ),
                Some(played) => note(ui, format!("Plays {}", widgets::seconds(played))),
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
                let hint = "How fast the frames of the sequence play";
                row(ui, "Frame rate", hint, Ok(()), |ui| {
                    let default = Some(tgs::DEFAULT_FPS);
                    auto_number(
                        ui,
                        &mut options.fps,
                        ("Default", "Ten frames a second"),
                        default,
                        tgs::DEFAULT_FPS,
                        |value| value.range(0.1..=60.0).speed(0.1).max_decimals(2).suffix(" fps"),
                    );
                });
            } else {
                let hint = "Fewer frames a second leave more bytes for each; more than the \
                            input's would only repeat them. Automatic is the input's, at most 60";
                let highest = most.unwrap_or(telegram::MAX_FPS);
                row(ui, "Frame rate", hint, Ok(()), |ui| {
                    let most = highest.min(30.0);
                    let automatic = ("Input's", "The input's rate at the speed, at most 60 fps");
                    auto_number(ui, &mut options.fps, automatic, chosen, most, |value| {
                        value.range(1.0..=highest).speed(0.1).max_decimals(2).suffix(" fps")
                    });
                });
            }
            let hint = "Telegram refuses stickers that say they last longer than 3 seconds";
            row(ui, "Over 3 seconds", hint, Ok(()), |ui| {
                let preset = webm_base.spoof.unwrap_or(Spoof::Auto);
                preset_segments(ui, &mut options.spoof, preset, &SPOOFS);
                let spoof = options.spoof.unwrap_or(preset);
                chosen_hint(ui, spoof, &SPOOFS);
                if let Some(played) = played
                    && spoof == Spoof::Auto
                    && telegram::too_long(played)
                {
                    let played = widgets::seconds(played);
                    note(ui, format!("Plays all {played}: Telegram is told it is shorter"));
                }
            });
            let honest = options.spoof.or(webm_base.spoof) == Some(Spoof::Never);
            let allowed = if honest { Err("Nothing is spoofed when cut at 3 s") } else { Ok(()) };
            let hint = "The duration written into the file, which Telegram reads";
            row(ui, "Spoof to", hint, allowed, |ui| {
                let custom = options.fake_duration.is_some();
                let (automatic, shown) = match webm_base.fake_duration {
                    Some(duration) => (("Preset's", "The duration the preset sets"), duration),
                    None => {
                        (("Default", "A short duration Telegram accepts"), DEFAULT_FAKE_DURATION)
                    }
                };
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
            let hint = "How fast the frames of a sequence or sprite sheet play";
            row(ui, "Frame rate", hint, Ok(()), |ui| {
                let default = Some(tgs::DEFAULT_FPS);
                auto_number(
                    ui,
                    &mut item.choices.tgs.fps,
                    ("Default", "Ten frames a second"),
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
                ui.label("From");
                let field =
                    timeline::boundary_field(&mut first, 0..=after - 1, frames, in_frames, false);
                let start = ui.add_sized(size, field).on_hover_text("Its first frame").changed();
                ui.label("to");
                let range = first + 1..=frames.count();
                let field = timeline::boundary_field(&mut after, range, frames, in_frames, true);
                let end = ui.add_sized(size, field).on_hover_text("Its last frame").changed();
                if start || end {
                    frames.set_part((first, after), &mut choices.start, &mut choices.length);
                }
                let (first, after) = frames.part(length, choices.start, choices.length);
                counted = Some(if in_frames {
                    format!(
                        "{} to {} of {}",
                        widgets::seconds(frames.times[first]),
                        widgets::seconds(frames.times[after]),
                        widgets::seconds(length)
                    )
                } else if after - first == 1 {
                    format!("Frame {} of {}", first + 1, frames.count())
                } else {
                    format!("Frames {} to {} of {}", first + 1, after, frames.count())
                });
            }
            None => {
                let (mut from, mut to) = timeline::part(length, choices.start, choices.length);
                ui.label("From");
                let start = ui.add_sized(size, timeline::seconds(&mut from, length)).changed();
                ui.label("to");
                let end = ui.add_sized(size, timeline::seconds(&mut to, length)).changed();
                if start || end {
                    let from = from.min(to - 0.01).max(0.0);
                    let to = to.max(from + 0.01).min(length);
                    timeline::set_part(length, (from, to), &mut choices.start, &mut choices.length);
                }
            }
        }
        if (choices.start.is_some() || choices.length.is_some())
            && widgets::reset_to(ui, "All of the input")
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
    section(ui, "Frame");
    let hint =
        "The time of the frame the picture is made of; the bar under the preview picks it too";
    row(ui, "Frame at", hint, Ok(()), |ui| {
        let Some(frames) = item.input_frames() else { return };
        let mut at = item.choices.frame.unwrap_or(0.0);
        let field = egui::DragValue::new(&mut at).range(0.0..=length).speed(0.01).max_decimals(3);
        if ui.add(field.suffix(" s")).changed() {
            // where its frame starts, as the picture is made of it
            let index = frames.at(at);
            item.choices.frame = (index > 0).then_some(frames.times[index]);
        }
        let index = frames.at(item.choices.frame.unwrap_or(0.0));
        note(ui, format!("Frame {} of {}", index + 1, frames.count()));
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

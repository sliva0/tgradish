//! The settings of one item: what to make, beside the preview, and how,
//! under it.

use eframe::egui::{self, RichText};
use tgradish_core::config::Config;
use tgradish_core::options::{Crop, Fit, Options, Range, Resize, Speed, Spoof};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::{self, Target};
use tgradish_core::tgs;
use tgradish_tgs::normalise::Long;
use tgradish_tgs::reduce::Kind as Reduction;

use crate::item::{Aspect, Item, Kind};
use crate::widgets::{
    self, Choice, auto_number, chosen_hint, flag, label, note, preset_segments, section, segments,
};

/// What the settings depend on besides the item.
pub struct Context<'a> {
    pub presets: &'a Presets,
    pub config: &'a Config,
    /// ffmpeg is a separate program, so raw arguments can be passed.
    pub extra_args: bool,
}

const LABELS: f32 = 128.0;

fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).min_col_width(LABELS).spacing([14.0, 10.0]).show(ui, rows);
}

/// The built-in presets in the order of their speed, then the user's own.
pub fn preset_names(presets: &Presets) -> Vec<(String, Result<String, String>)> {
    const BUILTIN: [&str; 3] = ["fast", "balanced", "best"];
    let mut names: Vec<(String, Result<String, String>)> = presets
        .iter()
        .map(|(name, preset)| {
            (name.to_owned(), preset.map(|p| p.description.clone()).map_err(str::to_owned))
        })
        .collect();
    names.sort_by_key(|(name, _)| {
        (BUILTIN.iter().position(|b| b == name).unwrap_or(BUILTIN.len()), name.clone())
    });
    names
}

/// What a built-in preset does for `format`.
fn builtin(name: &str, format: Format) -> Option<&'static str> {
    Some(match (name, format) {
        ("fast", Format::Webm) => {
            "Fits only the bitrate, with the fast encoder: seconds, but blurrier"
        }
        ("balanced", Format::Webm) => "Tries a few frame rates and keeps the one that looks best",
        ("best", Format::Webm) => {
            "Like balanced, with the slowest encoder: minutes for long videos"
        }
        ("fast", Format::Tgs) => "Under a second, a few percent larger",
        ("balanced", Format::Tgs) => "Between fast and best",
        ("best", Format::Tgs) => "The smallest file, in a few seconds",
        _ => return None,
    })
}

pub fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// What to make: format, target and preset. Narrow, for beside the
/// preview.
pub fn make(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.label(RichText::new("Make").strong().size(15.5));
    ui.add_space(4.0);
    let formats = [
        (Format::Webm, "Video sticker (WebM)", "A VP9 video, from any video or image"),
        (
            Format::Tgs,
            "Vector animation (TGS)",
            "Pixel art drawn as shapes: sharp at any size, and small",
        ),
    ];
    for (format, text, hint) in formats {
        let allowed = item.kind.allows(format);
        let response = ui
            .add_enabled(allowed, egui::RadioButton::new(item.format == format, text))
            .on_hover_text(hint)
            .on_disabled_hover_text(match item.kind {
                Kind::Video => "Only pixel art can become a TGS animation",
                _ => "ffmpeg can't read Aseprite files or folders of frames",
            });
        if response.clicked() {
            item.format = format;
            item.format_guessed = false;
        }
    }
    ui.add_space(8.0);
    let sticker = match item.format {
        Format::Webm => "512 px on the longer side, up to 256 KB",
        Format::Tgs => "512 × 512, up to 64 KB",
    };
    let targets = [
        (Target::Sticker, "Sticker", sticker),
        (Target::Emoji, "Custom emoji", "100 × 100 px, up to 64 KB"),
    ];
    segments(ui, &mut item.choices.target, &targets, |_| Ok(()));
    chosen_hint(ui, item.choices.target, &targets);

    ui.add_space(8.0);
    ui.label("Preset")
        .on_hover_text("How hard to work on the result; your own values below go on top");
    let current = item.choices.preset_name(item.format, context.config).to_owned();
    let default = context.config.preset_for(item.format).to_owned();
    let names = preset_names(context.presets);
    let mut picked = current.clone();
    let labels: Vec<String> = names.iter().map(|(name, _)| capitalised(name)).collect();
    let choices: Vec<Choice<&str>> = names
        .iter()
        .zip(&labels)
        .map(|((name, description), label)| {
            (name.as_str(), label.as_str(), description.as_deref().unwrap_or(""))
        })
        .collect();
    let broken = |name: &str| {
        names.iter().find(|(n, _)| n == name).and_then(|(_, d)| d.as_ref().err()).is_some()
    };
    let mut chosen = picked.as_str();
    if segments(ui, &mut chosen, &choices, |name| {
        if broken(name) { Err("This preset's file is broken") } else { Ok(()) }
    }) {
        picked = chosen.to_owned();
        item.choices.preset = (picked != default).then(|| picked.clone());
    }
    match (builtin(&picked, item.format), context.presets.get(&picked)) {
        (Some(text), Ok(preset)) if preset.path.is_none() => note(ui, text),
        (_, Ok(preset)) => note(ui, preset.description.clone()),
        _ => {}
    }
}

/// The ratios the crop can keep.
const ASPECTS: [(Aspect, &str, &str); 7] = [
    (Aspect::Free, "Free", ""),
    (Aspect::Input, "Input's", "The input's own shape"),
    (Aspect::Ratio(1.0, 1.0), "1:1", ""),
    (Aspect::Ratio(4.0, 3.0), "4:3", ""),
    (Aspect::Ratio(3.0, 4.0), "3:4", ""),
    (Aspect::Ratio(16.0, 9.0), "16:9", ""),
    (Aspect::Ratio(9.0, 16.0), "9:16", ""),
];

/// The width to height ratio `aspect` keeps for a `size` input.
pub fn ratio(aspect: Aspect, size: (u32, u32)) -> Option<f64> {
    match aspect {
        Aspect::Free => None,
        Aspect::Input => Some(f64::from(size.0) / f64::from(size.1.max(1))),
        Aspect::Ratio(width, height) => Some(width / height),
    }
}

/// `crop` cut to `ratio` around its middle.
pub fn to_ratio(crop: Crop, ratio: f64) -> Crop {
    let (width, height) = (f64::from(crop.width), f64::from(crop.height));
    let (new_width, new_height) =
        if width / height > ratio { (height * ratio, height) } else { (width, width / ratio) };
    let new_width = new_width.round().max(1.0) as u32;
    let new_height = new_height.round().max(1.0) as u32;
    Crop {
        x: crop.x + (crop.width - new_width) / 2,
        y: crop.y + (crop.height - new_height) / 2,
        width: new_width,
        height: new_height,
    }
}

/// How WebM results fill their box.
const RESIZES: [Choice<Resize>; 4] = [
    (Resize::Contain, "Whole", "All of the picture, 512 px on the longer side"),
    (Resize::Pad, "Whole, padded", "All of the picture, with transparent bars to fill the box"),
    (Resize::Crop, "Fill", "Fills the box, cutting off what sticks out: the preview dims it"),
    (Resize::Stretch, "Stretch", "Fills the box, stretching the picture"),
];

/// Shows the settings under the preview.
pub fn show(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    picture(ui, item);
    time(ui, item);
    match item.format {
        Format::Webm => webm_quality(ui, item, context),
        Format::Tgs => tgs_quality(ui, item, context),
    }
    if item.format == Format::Tgs {
        reading(ui, item);
    }
    metadata(ui, item);
    if item.format == Format::Webm {
        advanced(ui, item, context);
    }
}

fn picture(ui: &mut egui::Ui, item: &mut Item) {
    ui.add_space(4.0);
    ui.label(RichText::new("Picture").strong().size(15.5));
    ui.add_space(4.0);
    let size = item.input_size();
    grid(ui, "picture", |ui| {
        label(
            ui,
            "Crop",
            "The part of the input to use, in its pixels. Drag on the preview to change it",
        );
        ui.horizontal_wrapped(|ui| {
            let Some((width, height)) = size else {
                note(ui, "once the input is read");
                return;
            };
            let full = Crop { x: 0, y: 0, width, height };
            let mut crop = item.choices.crop.unwrap_or(full);
            let mut edited = false;
            ui.label("x");
            edited |= ui.add(egui::DragValue::new(&mut crop.x).range(0..=width - 1)).changed();
            ui.label("y");
            edited |= ui.add(egui::DragValue::new(&mut crop.y).range(0..=height - 1)).changed();
            ui.label("  width");
            edited |= ui.add(egui::DragValue::new(&mut crop.width).range(1..=width)).changed();
            ui.label("height");
            edited |= ui.add(egui::DragValue::new(&mut crop.height).range(1..=height)).changed();
            if edited {
                crop.width = crop.width.min(width - crop.x);
                crop.height = crop.height.min(height - crop.y);
                item.choices.crop = (crop != full).then_some(crop);
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(item.choices.crop.is_some(), egui::Button::new("Whole picture"))
                .clicked()
            {
                item.choices.crop = None;
            }
        });
        ui.end_row();

        label(ui, "Crop shape", "The width to height ratio the crop keeps while you drag it");
        let mut aspect = item.view.aspect;
        if segments(ui, &mut aspect, &ASPECTS, |_| Ok(())) {
            item.view.aspect = aspect;
            if let Some(size) = size
                && let Some(ratio) = ratio(aspect, size)
            {
                let full = Crop { x: 0, y: 0, width: size.0, height: size.1 };
                let cut = to_ratio(item.choices.crop.unwrap_or(full), ratio);
                item.choices.crop = (cut != full).then_some(cut);
            }
        }
        ui.end_row();

        match item.format {
            Format::Webm => {
                let target = item.choices.target;
                label(
                    ui,
                    "Fit into the box",
                    "How the picture is scaled into the sticker's or emoji's size",
                );
                let base = if target.requires_exact_size() { Resize::Pad } else { Resize::Contain };
                ui.vertical(|ui| {
                    let mut resize = item.choices.webm.resize.unwrap_or(base);
                    ui.horizontal(|ui| {
                        let allowed = |option| {
                            if option == Resize::Contain && target.requires_exact_size() {
                                Err("Emoji must be square: pad, fill or stretch them")
                            } else {
                                Ok(())
                            }
                        };
                        if segments(ui, &mut resize, &RESIZES, allowed) {
                            item.choices.webm.resize = (resize != base).then_some(resize);
                        }
                        if item.choices.webm.resize.is_some() && widgets::reset(ui) {
                            item.choices.webm.resize = None;
                            resize = base;
                        }
                    });
                    if let Some((width, height)) = size {
                        let (w, h) =
                            item.choices.crop.map_or((width, height), |c| (c.width, c.height));
                        let (out_w, out_h) = output_size(target, resize, (w, h));
                        let hint = RESIZES
                            .iter()
                            .find(|(option, ..)| *option == resize)
                            .map_or("", |c| c.2);
                        note(ui, format!("{hint}. The result is {out_w} × {out_h} px"));
                    }
                });
                ui.end_row();
            }
            Format::Tgs => {
                label(ui, "Canvas", "");
                flag(
                    ui,
                    &mut item.choices.tgs.keep_canvas,
                    false,
                    "Keep transparent margins",
                    "Keep the whole canvas or crop instead of cutting it to the visible pixels",
                );
                ui.end_row();
                label(
                    ui,
                    "Art pixel size",
                    "How many input pixels one pixel of the art takes. Pixels off its grid are moved onto it",
                );
                auto_number(ui, &mut item.choices.tgs.pixel_scale, "Detected", 2, |value| {
                    value.range(1..=64).suffix(" px")
                });
                ui.end_row();
            }
        }
    });
}

/// The size of a WebM result: `target`'s box, filled from a `size` input
/// the way `resize` does.
pub fn output_size(target: Target, resize: Resize, (width, height): (u32, u32)) -> (u32, u32) {
    let (box_w, box_h) = target.box_size();
    match resize {
        Resize::Contain => {
            let scale =
                (f64::from(box_w) / f64::from(width)).min(f64::from(box_h) / f64::from(height));
            let even = |side: u32| (((f64::from(side) * scale / 2.0).round() as u32) * 2).max(2);
            (even(width).min(box_w), even(height).min(box_h))
        }
        Resize::Pad | Resize::Crop | Resize::Stretch => (box_w, box_h),
    }
}

const SPOOFS: [Choice<Spoof>; 3] = [
    (
        Spoof::Auto,
        "Spoof when longer",
        "Longer stickers get a short duration written into them: Telegram accepts them and plays all of it",
    ),
    (Spoof::Always, "Always spoof", "Even short stickers get the short duration"),
    (Spoof::Never, "Cut at 3 s", "Keep the file honest and cut it at 3 seconds"),
];

const LONGS: [Choice<Long>; 2] = [
    (Long::SpeedUp, "Play faster", "Longer animations are sped up to last 3 seconds"),
    (Long::Trim, "Cut at 3 s", "Longer animations keep their speed and are cut"),
];

fn time(ui: &mut egui::Ui, item: &mut Item) {
    let animated = item.input_length().is_some();
    let sequence =
        item.format == Format::Tgs && (item.sequence || item.choices.tgs.sheet.is_some());
    if !animated && !sequence {
        return;
    }
    section(ui, "Time");
    grid(ui, "time", |ui| match item.format {
        Format::Webm => {
            let options = &mut item.choices.webm;
            label(ui, "Frame rate", "Fewer frames a second leave more bytes for each");
            auto_number(ui, &mut options.fps, "Input's, at most 30", 30.0, |value| {
                value.range(1.0..=telegram::MAX_FPS).speed(0.1).max_decimals(2).suffix(" fps")
            });
            ui.end_row();
            label(
                ui,
                "Over 3 seconds",
                "Telegram refuses stickers that say they last longer than 3 seconds",
            );
            ui.vertical(|ui| {
                preset_segments(ui, &mut options.spoof, Spoof::Auto, &SPOOFS);
                chosen_hint(ui, options.spoof.unwrap_or(Spoof::Auto), &SPOOFS);
                if options.spoof != Some(Spoof::Never) {
                    ui.horizontal(|ui| {
                        ui.label("Duration written:");
                        auto_number(ui, &mut options.fake_duration, "0.42069 s", 1.0, |value| {
                            value
                                .range(0.01..=telegram::MAX_SECONDS)
                                .speed(0.01)
                                .max_decimals(5)
                                .suffix(" s")
                        });
                    });
                }
            });
            ui.end_row();
        }
        Format::Tgs => {
            let options = &mut item.choices.tgs;
            if animated {
                label(ui, "Over 3 seconds", "Animated stickers can last at most 3 seconds");
                ui.vertical(|ui| {
                    preset_segments(ui, &mut options.long, Long::SpeedUp, &LONGS);
                    chosen_hint(ui, options.long.unwrap_or_default(), &LONGS);
                });
                ui.end_row();
            }
            if sequence {
                label(ui, "Frame rate", "How fast the frames of a sequence or sprite sheet play");
                auto_number(ui, &mut options.fps, "10 fps", tgs::DEFAULT_FPS, |value| {
                    value.range(0.1..=60.0).speed(0.1).max_decimals(2).suffix(" fps")
                });
                ui.end_row();
            }
        }
    });
}

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

fn webm_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, "Size and quality");
    let base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.webm;
    let fit = options.fit.or(base.fit).unwrap_or(Fit::Auto);
    grid(ui, "webm-quality", |ui| {
        label(
            ui,
            "Getting under the limit",
            "What tgradish changes to land just under the size limit",
        );
        ui.vertical(|ui| {
            preset_segments(ui, &mut options.fit, base.fit.unwrap_or(Fit::Auto), &FITS);
            chosen_hint(ui, fit, &FITS);
        });
        ui.end_row();

        label(ui, "VP9 encoder", "Faster encoding looks worse at the same size");
        preset_segments(
            ui,
            &mut options.speed,
            base.speed.unwrap_or(Speed::Balanced),
            &WEBM_SPEEDS,
        );
        ui.end_row();

        if fit != Fit::Off {
            label(ui, "Encodes at most", "How many times fitting may encode");
            auto_number(ui, &mut options.attempts, "8", 8, |value| value.range(1..=50));
            ui.end_row();
            let unit = match fit {
                Fit::Auto | Fit::Bitrate => " kbit/s",
                Fit::Fps => " fps",
                Fit::Length => " s",
                Fit::Crf | Fit::Off => "",
            };
            label(ui, "Search within", "The range fitting searches, in the fitted value's unit");
            ui.horizontal(|ui| {
                let mut range = options.fit_range;
                if ui.radio(range.is_none(), "Automatic").clicked() {
                    range = None;
                }
                if ui.radio(range.is_some(), "").clicked() && range.is_none() {
                    range = Some(match fit {
                        Fit::Crf => Range { min: 4.0, max: 63.0 },
                        Fit::Fps => Range { min: 1.0, max: 30.0 },
                        Fit::Length => Range { min: 0.1, max: 3.0 },
                        _ => Range { min: 8.0, max: 2000.0 },
                    });
                }
                let mut shown = range.unwrap_or(Range { min: 0.0, max: 0.0 });
                let enabled = range.is_some();
                let low = ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut shown.min).speed(0.5).max_decimals(2),
                );
                ui.label("to");
                let high = ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut shown.max).speed(0.5).max_decimals(2).suffix(unit),
                );
                if low.changed() || high.changed() {
                    shown.max = shown.max.max(shown.min);
                    range = Some(shown);
                }
                options.fit_range = range;
            });
            ui.end_row();
        }

        if matches!(fit, Fit::Fps | Fit::Length | Fit::Off) {
            label(ui, "Quality (CRF)", "Constant quality from 0, the best, to 63");
            auto_number(ui, &mut options.crf, "32", 32, |value| value.range(0..=63));
            ui.end_row();
            label(ui, "Lossless", "");
            flag(
                ui,
                &mut options.lossless,
                false,
                "Lossless encoding",
                "Only small for tiny or still pictures",
            );
            ui.end_row();
        }
        if fit == Fit::Off {
            label(ui, "Bitrate", "Used unless only a quality is set");
            auto_number(ui, &mut options.bitrate, "From the size limit", 600.0, |value| {
                value.range(1.0..=50_000.0).speed(5.0).max_decimals(0).suffix(" kbit/s")
            });
            ui.end_row();
        }
    });
}

const TGS_SPEEDS: [Choice<Speed>; 3] = [
    (Speed::Fast, "Fast", "Under a second; a few percent larger"),
    (Speed::Balanced, "Balanced", ""),
    (Speed::Best, "Best", "A few seconds; the smallest"),
];

const REDUCTIONS: [(Reduction, &str, &str); 6] = [
    (Reduction::SnapToGrid, "Snap to the pixel grid", "Move stray pixels onto the art's grid"),
    (Reduction::MergeColours, "Merge similar colours", ""),
    (Reduction::Despeckle, "Remove specks", "Single pixels that differ from all around them"),
    (Reduction::MergeFrames, "Merge similar frames", "Frames that hardly differ show one picture"),
    (Reduction::DropFrames, "Drop frames", "Show every other frame, or fewer"),
    (Reduction::Downscale, "Lower the resolution", "Fewer, larger art pixels"),
];

fn tgs_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, "Size and quality");
    let base = item.choices.tgs_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.tgs;
    grid(ui, "tgs-quality", |ui| {
        label(ui, "TGS encoder", "How hard to look for a small encoding");
        ui.vertical(|ui| {
            preset_segments(ui, &mut options.speed, base.speed.unwrap_or(Speed::Best), &TGS_SPEEDS);
            chosen_hint(ui, options.speed.or(base.speed).unwrap_or(Speed::Best), &TGS_SPEEDS);
        });
        ui.end_row();

        label(ui, "Too large", "What happens when the art doesn't fit into 64 KB as it is");
        flag(
            ui,
            &mut options.lossless,
            base.lossless.unwrap_or(false),
            "Never change the art; report it instead",
            "",
        );
        ui.end_row();

        let lossless = options.lossless.or(base.lossless).unwrap_or(false);
        label(ui, "May change", "What fitting may give up to make it fit, least visible first");
        ui.add_enabled_ui(!lossless, |ui| {
            let all = base.reductions.clone().unwrap_or_else(|| Reduction::ALL.to_vec());
            let mut chosen = options.reductions.clone().unwrap_or_else(|| all.clone());
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (kind, text, hint) in REDUCTIONS {
                    let mut on = chosen.contains(&kind);
                    let response = ui.checkbox(&mut on, text);
                    let response =
                        if hint.is_empty() { response } else { response.on_hover_text(hint) };
                    if response.changed() {
                        if on {
                            chosen.push(kind);
                        } else {
                            chosen.retain(|&other| other != kind);
                        }
                        changed = true;
                    }
                }
                if options.reductions.is_some() && widgets::reset(ui) {
                    options.reductions = None;
                }
            });
            if changed {
                // in fitting's order
                chosen.sort_by_key(|kind| Reduction::ALL.iter().position(|other| other == kind));
                options.reductions = (chosen != all).then_some(chosen);
            }
        });
        ui.end_row();
    });
}

fn reading(ui: &mut egui::Ui, item: &mut Item) {
    let tags = item.art.ready().map(|art| art.tags.clone()).unwrap_or_default();
    let single = item.inputs.len() == 1 && !item.sequence;
    if tags.is_empty() && !single {
        return;
    }
    section(ui, "Reading the art");
    let options = &mut item.choices.tgs;
    grid(ui, "reading", |ui| {
        if !tags.is_empty() {
            label(ui, "Aseprite tag", "Only the frames of one tag, in its direction");
            ui.horizontal_wrapped(|ui| {
                if ui.radio(options.tag.is_none(), "All frames").clicked() {
                    options.tag = None;
                }
                for tag in &tags {
                    if ui.radio(options.tag.as_ref() == Some(tag), tag).clicked() {
                        options.tag = Some(tag.clone());
                    }
                }
            });
            ui.end_row();
        }
        if single && tags.is_empty() {
            label(ui, "Sprite sheet", "Frames laid out in a grid of equal cells, row by row");
            ui.horizontal_wrapped(|ui| {
                let mut sheet = options.sheet.is_some();
                if ui.checkbox(&mut sheet, "Read as a sprite sheet of").changed() {
                    options.sheet = sheet.then(|| "4x1".to_owned());
                }
                let (mut columns, mut rows) =
                    parse_sheet(options.sheet.as_deref()).unwrap_or((4, 1));
                let a = ui.add_enabled(sheet, egui::DragValue::new(&mut columns).range(1..=256));
                ui.label("×");
                let b = ui.add_enabled(sheet, egui::DragValue::new(&mut rows).range(1..=256));
                ui.label("cells");
                if sheet && (a.changed() || b.changed()) {
                    options.sheet = Some(format!("{columns}x{rows}"));
                }
                if sheet {
                    ui.label(", frames:");
                    auto_number(ui, &mut options.sheet_frames, "All", columns * rows, |value| {
                        value.range(1..=columns * rows)
                    });
                }
            });
            ui.end_row();
        }
    });
}

fn parse_sheet(sheet: Option<&str>) -> Option<(u32, u32)> {
    let (columns, rows) = sheet?.split_once('x')?;
    Some((columns.parse().ok()?, rows.parse().ok()?))
}

fn metadata(ui: &mut egui::Ui, item: &mut Item) {
    section(ui, "In the file");
    let (title, watermark) = match item.format {
        Format::Webm => (&mut item.choices.webm.title, &mut item.choices.webm.watermark),
        Format::Tgs => (&mut item.choices.tgs.title, &mut item.choices.tgs.watermark),
    };
    grid(ui, "metadata", |ui| {
        label(ui, "Title", "A name stored in the file");
        let mut text = title.clone().unwrap_or_default();
        if ui
            .add(egui::TextEdit::singleline(&mut text).hint_text("none").desired_width(260.0))
            .changed()
        {
            *title = (!text.is_empty()).then_some(text);
        }
        ui.end_row();
        label(ui, "Watermark", "");
        flag(
            ui,
            watermark,
            true,
            "Mark it as made by tgradish",
            "In the file's metadata, never in the picture",
        );
        ui.end_row();
    });
}

fn advanced(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new("Encoder options").strong()).id_salt("advanced").show(ui, |ui| {
        let options: &mut Options = &mut item.choices.webm;
        grid(ui, "advanced", |ui| {
            label(ui, "libvpx-vp9", "Encoder options by name, like tune-content = screen for flat graphics. ffmpeg -h encoder=libvpx-vp9 lists them");
            ui.vertical(|ui| {
                let mut pairs: Vec<(String, String)> = options.encoder_options.clone().unwrap_or_default().into_iter().collect();
                let mut changed = false;
                let mut remove = None;
                for (index, (name, value)) in pairs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(name).hint_text("name").desired_width(130.0)).changed();
                        ui.label("=");
                        changed |= ui.add(egui::TextEdit::singleline(value).hint_text("value").desired_width(100.0)).changed();
                        if ui.small_button("✖").on_hover_text("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    pairs.remove(index);
                    changed = true;
                }
                if ui.small_button("Add an option").clicked() {
                    pairs.push((String::new(), String::new()));
                    changed = true;
                }
                if changed {
                    options.encoder_options = (!pairs.is_empty()).then(|| pairs.into_iter().collect());
                }
            });
            ui.end_row();

            label(ui, "ffmpeg arguments", "Raw arguments added before the output, split like a shell would");
            ui.vertical(|ui| {
                let mut text = options.extra_args.as_ref().map(|words| shlex::try_join(words.iter().map(String::as_str)).unwrap_or_default()).unwrap_or_default();
                let edit = ui.add_enabled(context.extra_args, egui::TextEdit::singleline(&mut text).hint_text("none").desired_width(300.0));
                if edit.changed() {
                    options.extra_args = shlex::split(&text).filter(|words| !words.is_empty());
                }
                if !context.extra_args {
                    note(ui, "Only with the system's ffmpeg, see Settings");
                }
            });
            ui.end_row();
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_crops_to_a_ratio_around_their_middle() {
        let crop = Crop { x: 10, y: 10, width: 200, height: 100 };
        assert_eq!(to_ratio(crop, 1.0), Crop { x: 60, y: 10, width: 100, height: 100 });
        assert_eq!(to_ratio(crop, 4.0), Crop { x: 10, y: 35, width: 200, height: 50 });
    }

    #[test]
    fn sizes_results_like_planning() {
        assert_eq!(output_size(Target::Sticker, Resize::Contain, (640, 360)), (512, 288));
        assert_eq!(output_size(Target::Sticker, Resize::Contain, (300, 100)), (512, 170));
        assert_eq!(output_size(Target::Emoji, Resize::Pad, (300, 100)), (100, 100));
    }

    #[test]
    fn orders_presets_by_speed() {
        let names: Vec<String> =
            preset_names(&Presets::builtin()).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["fast", "balanced", "best"]);
    }
}

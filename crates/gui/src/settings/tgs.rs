//! Size and quality for TGS animations, and how their art is read. To
//! fit into 64 KiB the art itself may have to change, so what it may give
//! up is the user's to choose.

use eframe::egui;
use tgradish_core::options::Speed;
use tgradish_core::tgs;
use tgradish_tgs::reduce::Kind as Reduction;

use super::Context;
use crate::item::{Item, Kind};
use crate::widgets::{self, Choice, auto_number, chosen_hint, flag, preset_segments, row, section};

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

const COMPROMISES: [Choice<tgs::Compromise>; 3] = [
    (tgs::Compromise::Auto, "Either", "Whatever changes the art least for the bytes it saves"),
    (
        tgs::Compromise::Motion,
        "Motion",
        "Frames merge and drop first: full detail, choppier motion",
    ),
    (tgs::Compromise::Detail, "Detail", "The picture coarsens first: smooth motion, less detail"),
];

pub(super) fn tgs_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, "Size and quality");
    let base = item.choices.tgs_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.tgs;
    row(ui, "TGS encoder", "How hard to look for a small encoding", Ok(()), |ui| {
        preset_segments(ui, &mut options.speed, base.speed.unwrap_or(Speed::Best), &TGS_SPEEDS);
        chosen_hint(ui, options.speed.or(base.speed).unwrap_or(Speed::Best), &TGS_SPEEDS);
    });
    let hint = "What happens when the art doesn't fit into 64 KiB as it is";
    row(ui, "If output is too large", hint, Ok(()), |ui| {
        let text = "Never change the art; report it instead";
        flag(ui, &mut options.lossless, base.lossless.unwrap_or(false), text, "");
    });

    let lossless = options.lossless.or(base.lossless).unwrap_or(false);
    let changes = if lossless { Err("The art is never changed, as set above") } else { Ok(()) };
    let hint = "Art that is mostly motion can lose frames or detail";
    row(ui, "Give up first", hint, changes, |ui| {
        let base = base.compromise.unwrap_or_default();
        preset_segments(ui, &mut options.compromise, base, &COMPROMISES);
        chosen_hint(ui, options.compromise.unwrap_or(base), &COMPROMISES);
    });
    let hint = "What fitting may give up to make it fit, least visible first";
    row(ui, "May change", hint, changes, |ui| {
        let all = base.reductions.clone().unwrap_or_else(|| Reduction::ALL.to_vec());
        let mut chosen = options.reductions.clone().unwrap_or_else(|| all.clone());
        let mut changed = false;
        // a list, closer than rows of settings
        ui.spacing_mut().item_spacing.y = 2.0;
        for (index, (kind, text, hint)) in REDUCTIONS.into_iter().enumerate() {
            ui.horizontal(|ui| {
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
                if index == 0 && options.reductions.is_some() && widgets::reset(ui) {
                    options.reductions = None;
                }
            });
        }
        if changed {
            // in fitting's order
            chosen.sort_by_key(|kind| Reduction::ALL.iter().position(|other| other == kind));
            options.reductions = (chosen != all).then_some(chosen);
        }
    });
}

/// How a picture is read: as a sprite sheet, or as it is.
pub(super) fn reading(ui: &mut egui::Ui, item: &mut Item) {
    if item.kind != Kind::Image || item.inputs.len() != 1 || item.sequence {
        return;
    }
    section(ui, "Reading the art");
    let options = &mut item.choices.tgs;
    let hint = "Frames laid out in a grid of equal cells, row by row";
    row(ui, "Sprite sheet", hint, Ok(()), |ui| {
        let mut sheet = options.sheet.is_some();
        if ui.checkbox(&mut sheet, "Read as a sprite sheet").changed() {
            options.sheet = sheet.then(|| "4x1".to_owned());
        }
        let (mut columns, mut rows) = parse_sheet(options.sheet.as_deref()).unwrap_or((4, 1));
        ui.add_enabled_ui(sheet, |ui| {
            ui.horizontal(|ui| {
                ui.label("Cells");
                let a = ui.add(egui::DragValue::new(&mut columns).range(1..=256));
                ui.label("×");
                let b = ui.add(egui::DragValue::new(&mut rows).range(1..=256));
                if sheet && (a.changed() || b.changed()) {
                    options.sheet = Some(format!("{columns}x{rows}"));
                }
                ui.add_space(14.0);
                ui.label("Frames");
                let all = columns * rows;
                let automatic = ("All", "Every cell but transparent ones at the end");
                auto_number(ui, &mut options.sheet_frames, automatic, Some(all), all, |value| {
                    value.range(1..=all)
                });
            });
        });
    });
}

fn parse_sheet(sheet: Option<&str>) -> Option<(u32, u32)> {
    let (columns, rows) = sheet?.split_once('x')?;
    Some((columns.parse().ok()?, rows.parse().ok()?))
}

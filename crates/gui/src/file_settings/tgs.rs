//! Size and quality for TGS animations, and how their art is read. To
//! fit into 64 KiB the art itself may have to change, so what it may give
//! up is the user's to choose.

use eframe::egui;
use tgradish_core::options::{Sheet, Speed};
use tgradish_core::tgradish_tgs::reduce::{self, Kind as Reduction};

use super::Context;
use crate::item::{Item, Kind};
use crate::l10n::fl;
use crate::widgets::{self, Choice, chosen_hint, flag, preset_segments, row, section};

fn tgs_speeds() -> [Choice<Speed>; 3] {
    [
        (Speed::Fast, fl!("tgs-fast"), fl!("tgs-fast", "hint")),
        (Speed::Balanced, fl!("tgs-balanced"), String::new()),
        (Speed::Best, fl!("tgs-best"), fl!("tgs-best", "hint")),
    ]
}

fn reductions() -> [Choice<Reduction>; 6] {
    [
        (Reduction::SnapToGrid, fl!("reduce-snap"), fl!("reduce-snap", "hint")),
        (Reduction::MergeColours, fl!("reduce-colours"), fl!("reduce-colours", "hint")),
        (Reduction::Despeckle, fl!("reduce-specks"), fl!("reduce-specks", "hint")),
        (Reduction::MergeFrames, fl!("reduce-frames"), fl!("reduce-frames", "hint")),
        (Reduction::DropFrames, fl!("reduce-drop"), fl!("reduce-drop", "hint")),
        (Reduction::Downscale, fl!("reduce-resolution"), fl!("reduce-resolution", "hint")),
    ]
}

fn compromises() -> [Choice<reduce::Compromise>; 3] {
    [
        (reduce::Compromise::Auto, fl!("give-up-either"), fl!("give-up-either", "hint")),
        (reduce::Compromise::Motion, fl!("give-up-motion"), fl!("give-up-motion", "hint")),
        (reduce::Compromise::Detail, fl!("give-up-detail"), fl!("give-up-detail", "hint")),
    ]
}

pub(super) fn tgs_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, &fl!("size-and-quality"));
    let base = item.choices.tgs_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.tgs;
    row(ui, &fl!("tgs-encoder"), &fl!("tgs-encoder", "hint"), Ok(()), |ui| {
        let speeds = tgs_speeds();
        preset_segments(ui, &mut options.speed, base.speed_or_default(), &speeds);
        chosen_hint(ui, base.clone().merged(options).speed_or_default(), &speeds);
    });
    row(ui, &fl!("too-large"), &fl!("too-large", "hint"), Ok(()), |ui| {
        let text = fl!("never-change");
        flag(ui, &mut options.exact, base.exact_or_default(), &text, "");
    });

    let exact = base.clone().merged(options).exact_or_default();
    let never = fl!("never-change", "set");
    let changes = if exact { Err(never.as_str()) } else { Ok(()) };
    row(ui, &fl!("give-up"), &fl!("give-up", "hint"), changes, |ui| {
        let compromises = compromises();
        let base = base.compromise.unwrap_or_default();
        preset_segments(ui, &mut options.compromise, base, &compromises);
        chosen_hint(ui, options.compromise.unwrap_or(base), &compromises);
    });
    row(ui, &fl!("ways-to-fit"), &fl!("ways-to-fit", "hint"), changes, |ui| {
        let all = base.reductions.clone().unwrap_or_else(|| Reduction::ALL.to_vec());
        let mut chosen = options.reductions.clone().unwrap_or_else(|| all.clone());
        let mut changed = false;
        // a list, closer than rows of settings
        ui.spacing_mut().item_spacing.y = 2.0;
        for (index, (kind, text, hint)) in reductions().into_iter().enumerate() {
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
                if index == 0 && widgets::reset(ui, options.reductions.is_some()) {
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

/// The layout a picture first gets once it is read as a sprite sheet: a
/// strip, as sheets most often are.
const FIRST_SHEET: Sheet = Sheet { columns: 4, rows: 1 };

/// How a picture is read: as a sprite sheet, or as it is. Only the item
/// says, as a sheet's layout is one file's, which no preset holds.
pub(super) fn reading(ui: &mut egui::Ui, item: &mut Item) {
    if item.kind != Kind::Image || item.inputs.len() != 1 || item.sequence {
        return;
    }
    section(ui, &fl!("reading-art"));
    let options = &mut item.choices.tgs;
    row(ui, &fl!("sprite-sheet"), &fl!("sprite-sheet", "hint"), Ok(()), |ui| {
        let mut sheet = options.sheet.is_some();
        if ui.checkbox(&mut sheet, fl!("read-as-sheet")).changed() {
            options.sheet = sheet.then_some(FIRST_SHEET);
        }
        let Sheet { mut columns, mut rows } = options.sheet.unwrap_or(FIRST_SHEET);
        ui.add_enabled_ui(sheet, |ui| {
            ui.horizontal(|ui| {
                ui.label(fl!("sheet-cells"));
                let a = ui.add(egui::DragValue::new(&mut columns).range(1..=256));
                ui.label("×");
                let b = ui.add(egui::DragValue::new(&mut rows).range(1..=256));
                if sheet && (a.changed() || b.changed()) {
                    options.sheet = Some(Sheet { columns, rows });
                }
                ui.add_space(14.0);
                ui.label(fl!("sheet-frames"));
                let all = columns * rows;
                let automatic = (fl!("sheet-all"), fl!("sheet-all", "hint"));
                let automatic = (automatic.0.as_str(), automatic.1.as_str());
                widgets::auto_number(
                    ui,
                    &mut options.sheet_frames,
                    automatic,
                    Some(all),
                    all,
                    |value| value.range(1..=all),
                );
            });
        });
    });
}

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
    // a file of frames of its own is no sheet, which is one picture: as
    // last read, so it stays as it is while read again, and unknown, not
    // ruled out, until first read
    let frames = item.art.ready().map(|art| art.file_frames(&item.art_reading));
    let refused =
        frames.filter(|&frames| frames > 1).map(|frames| fl!("sheet-refused", frames = frames));
    let allowed = refused.as_deref().map_or(Ok(()), Err);
    let options = &mut item.choices.tgs;
    row(ui, &fl!("sprite-sheet"), &fl!("sprite-sheet", "hint"), allowed, |ui| {
        // greyed out unticked, though a session may keep it ticked, which
        // reading and converting leave aside for this file
        let mut sheet = options.sheet.is_some() && refused.is_none();
        if ui.checkbox(&mut sheet, fl!("read-as-sheet")).changed() {
            options.sheet = sheet.then_some(FIRST_SHEET);
        }
        if let Some(refused) = &refused {
            widgets::note(ui, refused);
            return;
        }
        let Sheet { mut columns, mut rows } = options.sheet.unwrap_or(FIRST_SHEET);
        ui.add_enabled_ui(sheet, |ui| {
            ui.horizontal(|ui| {
                ui.label(fl!("sheet-cells"));
                // as wide as any count, so what follows stays put
                let (a, b) = widgets::steady_numbers(ui, 3, "", |ui| {
                    let a = ui.add(egui::DragValue::new(&mut columns).range(1..=256));
                    ui.label("×");
                    (a, ui.add(egui::DragValue::new(&mut rows).range(1..=256)))
                });
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

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::{NodeT, Queryable};
    use tgradish_core::presets::Format;

    use super::*;
    use crate::config::Config;
    use crate::testing::{finish, harness, square};

    /// The corpus's walker, an animated GIF of 8 frames, in `dir`.
    fn walker(dir: &std::path::Path) -> std::path::PathBuf {
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let path = dir.join("walker.gif");
        std::fs::write(&path, &walker.bytes).unwrap();
        path
    }

    #[test]
    fn offers_no_sheet_of_a_file_of_frames() {
        let dir = tempfile::tempdir().unwrap();
        let picture = dir.path().join("sheet.png");
        square(&picture, [255, 0, 0, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![picture, walker(dir.path())]);
        finish(&mut harness);
        for item in &mut harness.state_mut().items {
            item.format = Format::Tgs;
        }
        let select = |harness: &mut egui_kittest::Harness<crate::app::App>, index: usize| {
            let id = harness.state().items[index].id;
            harness.state_mut().select(id);
            finish(harness);
            harness.run_steps(3);
        };
        let label = fl!("read-as-sheet");
        let ticked = |harness: &egui_kittest::Harness<crate::app::App>| {
            let node = harness.get_by_label(&label);
            let toggled = node.accesskit_node().toggled();
            (node.accesskit_node().is_disabled(), toggled == Some(egui::accesskit::Toggled::True))
        };
        // a picture can be read as one, and is, ticked
        select(&mut harness, 0);
        assert_eq!(ticked(&harness), (false, false));
        harness.get_by_label(&label).click_accesskit();
        finish(&mut harness);
        harness.run_steps(3);
        assert_eq!(ticked(&harness), (false, true));
        // the GIF can't, and says why
        select(&mut harness, 1);
        assert_eq!(ticked(&harness), (true, false));
        harness.get_by_label(&fl!("sheet-refused", frames = 8));
        // each keeps its own
        select(&mut harness, 0);
        assert_eq!(ticked(&harness), (false, true));
        assert!(harness.state().items[0].choices.tgs.sheet.is_some());
        assert!(harness.state().items[1].choices.tgs.sheet.is_none());
    }

    #[test]
    fn reads_and_converts_a_file_of_frames_kept_as_a_sheet_as_it_is() {
        // as a reopened session has it: a GIF ticked to be read as a sheet
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![walker(dir.path())]);
        let item = &mut harness.state_mut().items[0];
        (item.format, item.format_guessed) = (Format::Tgs, false);
        item.choices.tgs.sheet = Some(FIRST_SHEET);
        finish(&mut harness);
        harness.run_steps(3);
        // read as it is, rather than not at all, and said so
        let item = &harness.state().items[0];
        assert_eq!(item.input_error(), None);
        assert_eq!(item.art.ready().map(|art| art.clip.frames.len()), Some(8));
        harness.get_by_label(&fl!("sheet-refused", frames = 8));
        // and converted so, from Convert all as from its own button
        let all = fl!("convert-all", count = 1);
        harness.get_by_label(&all).click();
        harness.run_steps(2);
        finish(&mut harness);
        let made = harness.state().items[0].made.as_ref().expect("made");
        let crate::jobs::Status::Done(_) = &made.job.status else {
            panic!("{:?}", made.job.status)
        };
        let warned = &made.job.progress.warnings;
        let said = fl!("sheet-refused", "converted", frames = 8);
        assert!(warned.contains(&said), "{warned:?}");
    }
}

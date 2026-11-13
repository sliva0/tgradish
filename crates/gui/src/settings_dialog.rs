//! The Settings dialog, of the window's own settings: ffmpeg, where
//! results go, and what new files start with. Changes apply at once; Save
//! keeps them in `config.toml`.

use std::path::{Path, PathBuf};

use crate::config::Config;
use eframe::egui::{self, Vec2};
use tgradish_core::CancelToken;
use tgradish_core::backend::Backend;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::presets::{Format, Presets};

use crate::app::App;
use crate::file_settings;
use crate::l10n::{self, fl};
use crate::media::Task;
use crate::widgets::{self, note, row, segments};

#[derive(Default)]
pub(crate) struct SettingsDialog {
    open: bool,
    /// Whether the window of tgradish's version and licences is open.
    pub(crate) about: bool,
    /// What checking ffmpeg found: whether it is ready, and what to say.
    ffmpeg_status: Option<(bool, String)>,
    /// Checking ffmpeg, which runs it, on a thread of its own: stopped if
    /// the dialog is opened again or the window closes.
    checking: Option<Task<(bool, String)>>,
    /// What Save did: where the settings went, or why they didn't.
    saved: Option<Result<String, String>>,
    /// What the fields show while empty, found once the dialog opens: the
    /// ffmpeg on PATH, and a folder results could go to.
    placeholders: (String, String),
    /// A file dialog asked for, which the window opens.
    pub(crate) pick: Option<crate::files::Pick>,
    /// Results are to go in a folder not chosen yet: kept here, as the
    /// settings never hold a folder without a name.
    folder_unchosen: bool,
    /// A default preset picked for a format, which the window takes up, as
    /// the items that follow it may then fit another way.
    pub(crate) default_preset: Option<(Format, String)>,
}

impl SettingsDialog {
    pub(crate) fn open(&mut self) {
        let ffmpeg = match tgradish_core::ffmpeg::Ffmpeg::on_path() {
            Ok(found) => fl!("ffmpeg-found", path = found.ffmpeg.display().to_string()),
            Err(_) if cfg!(windows) => fl!("for-example", example = r"C:\ffmpeg\bin"),
            Err(_) => fl!("for-example", example = "/usr/local/bin/ffmpeg"),
        };
        let folder = tgradish_core::paths::pictures_dir()
            .map(|dir| fl!("for-example", example = dir.join("Stickers").display().to_string()))
            .unwrap_or_default();
        *self = SettingsDialog {
            open: true,
            about: self.about,
            placeholders: (ffmpeg, folder),
            ..SettingsDialog::default()
        };
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

/// Shows the dialog if it is open. `ffmpeg_given` says the command line
/// that started the window chose its ffmpeg, over the settings'.
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut SettingsDialog,
    config: &mut Config,
    ffmpeg_given: bool,
    config_path: Option<&Path>,
    presets: &Presets,
) {
    if !dialog.open {
        return;
    }
    if let Some(found) = dialog.checking.as_ref().and_then(Task::poll) {
        dialog.ffmpeg_status = Some(found.unwrap_or_else(|message| (false, message)));
        dialog.checking = None;
    }
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
        ui.set_width(620.0);
        ui.horizontal(|ui| {
            ui.heading(fl!("settings"));
            // in the corner, square, level with the heading
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let side = ui.spacing().interact_size.y;
                let cross = egui::Button::new("✖").frame(false).min_size(Vec2::splat(side));
                close = ui.add(cross).on_hover_text(fl!("close")).clicked();
            });
        });
        // the dialog fits the window, its settings scrolling within
        let room = (ctx.content_rect().height() - 150.0).max(200.0);
        egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
            sections(ui, dialog, config, ffmpeg_given, presets);
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(fl!("save")).on_hover_text(fl!("save", "hint")).clicked() {
                dialog.saved = Some(match config_path {
                    Some(path) => match config.save(path) {
                        Ok(()) => Ok(fl!("saved-to", path = path.display().to_string())),
                        Err(err) => Err(fl!("not-saved", error = err.to_string())),
                    },
                    None => Err(fl!("no-home")),
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(fl!("about-and-licences")).clicked() {
                    dialog.about = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // cut short where it is long, as a path can be
                    let weak = |text: &str| egui::RichText::new(text).weak().size(12.5);
                    let text = match &dialog.saved {
                        Some(Ok(saved)) => weak(saved),
                        Some(Err(error)) => egui::RichText::new(error).color(widgets::BAD),
                        None => weak(&fl!("settings-note")),
                    };
                    ui.add(egui::Label::new(text).truncate());
                });
            });
        });
    });
    // changes stay either way, until tgradish closes unless saved
    if close || modal.should_close() {
        dialog.open = false;
    }
}

fn sections(
    ui: &mut egui::Ui,
    dialog: &mut SettingsDialog,
    config: &mut Config,
    ffmpeg_given: bool,
    presets: &Presets,
) {
    widgets::section(ui, "ffmpeg");
    if ffmpeg_given {
        note(ui, fl!("ffmpeg-given"));
    }
    let builtin = Backend::select(FfmpegChoice::Builtin, None).is_ok();
    row(ui, &fl!("ffmpeg-use"), &fl!("ffmpeg-use", "hint"), Ok(()), |ui| {
        let choices = [
            (FfmpegChoice::Auto, fl!("ffmpeg-auto"), fl!("ffmpeg-auto", "hint")),
            (FfmpegChoice::Builtin, fl!("ffmpeg-builtin"), fl!("ffmpeg-builtin", "hint")),
            (FfmpegChoice::System, fl!("ffmpeg-system"), fl!("ffmpeg-system", "hint")),
        ];
        segments(ui, &mut config.ffmpeg.choice, &choices, |choice| match choice {
            FfmpegChoice::Builtin if !builtin => Err(fl!("ffmpeg-builtin", "missing")),
            _ => Ok(()),
        });
    });
    row(ui, &fl!("ffmpeg-program"), &fl!("ffmpeg-program", "hint"), Ok(()), |ui| {
        let ffmpeg = &mut config.ffmpeg;
        ui.horizontal(|ui| {
            let mut text =
                ffmpeg.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            let edit = egui::TextEdit::singleline(&mut text)
                .hint_text(dialog.placeholders.0.as_str())
                .desired_width(280.0);
            if ui.add(edit).changed() {
                ffmpeg.path = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
            }
            if ui.button(fl!("choose")).clicked() {
                dialog.pick = Some(crate::files::Pick::Ffmpeg);
            }
        });
        ui.horizontal(|ui| {
            let check = egui::Button::new(fl!("check"));
            let clicked = ui
                .add_enabled(dialog.checking.is_none(), check)
                .on_hover_text(fl!("check", "hint"))
                .clicked();
            if clicked {
                // the settings being edited, even where the command line
                // chose another ffmpeg for this run: Check sits beside
                // them, and Save keeps them
                let ffmpeg = config.ffmpeg.clone();
                dialog.checking = Some(Task::spawn(ui.ctx(), move |cancel| {
                    Ok(check_ffmpeg(ffmpeg.choice, ffmpeg.path.as_deref(), cancel))
                }));
                dialog.ffmpeg_status = None;
            }
            if dialog.checking.is_some() {
                ui.spinner();
                note(ui, fl!("checking-ffmpeg"));
            } else if let Some((ok, status)) = &dialog.ffmpeg_status {
                let colour = if *ok { widgets::GOOD } else { widgets::BAD };
                ui.add(egui::Label::new(egui::RichText::new(status).color(colour)).truncate());
            }
        });
    });

    widgets::section(ui, &fl!("results"));
    let gui = &mut config.gui;
    let placeholder = &dialog.placeholders.1;
    row(ui, &fl!("save-results"), "", Ok(()), |ui| {
        let mut elsewhere = gui.output_dir.is_some() || dialog.folder_unchosen;
        let choices = [
            (false, fl!("next-to-input"), String::new()),
            (true, fl!("in-a-folder"), String::new()),
        ];
        if segments(ui, &mut elsewhere, &choices, |_| Ok(())) {
            gui.output_dir = None;
            dialog.folder_unchosen = elsewhere;
        }
        ui.add_enabled_ui(elsewhere, |ui| {
            ui.horizontal(|ui| {
                let mut text =
                    gui.output_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                let edit = egui::TextEdit::singleline(&mut text)
                    .hint_text(placeholder.as_str())
                    .desired_width(280.0);
                if ui.add(edit).changed() {
                    let dir = PathBuf::from(text.trim());
                    gui.output_dir = (!dir.as_os_str().is_empty()).then_some(dir);
                    dialog.folder_unchosen = gui.output_dir.is_none();
                }
                if ui.button(fl!("choose")).clicked() {
                    dialog.pick = Some(crate::files::Pick::OutputDir);
                }
            });
        });
        if elsewhere && gui.output_dir.is_none() {
            note(ui, fl!("in-a-folder", "unchosen"));
        }
    });
    row(ui, &fl!("existing-files"), &fl!("existing-files", "hint"), Ok(()), |ui| {
        ui.checkbox(&mut gui.overwrite, fl!("replace-in-the-way"));
    });

    widgets::section(ui, &fl!("window"));
    // only once there is more than English to pick
    let languages = l10n::languages();
    if languages.len() > 1 {
        row(ui, &fl!("language"), &fl!("language", "hint"), Ok(()), |ui| {
            // the system's, then each by its own name
            let tags: Vec<Option<String>> = std::iter::once(None)
                .chain(languages.iter().map(|(language, _)| Some(language.to_string())))
                .collect();
            let names = std::iter::once(fl!("as-the-system"))
                .chain(languages.iter().map(|(_, name)| name.clone()));
            let choices: Vec<widgets::Choice<usize>> =
                names.enumerate().map(|(index, name)| (index, name, String::new())).collect();
            let mut index = tags.iter().position(|tag| *tag == gui.language).unwrap_or(0);
            if segments(ui, &mut index, &choices, |_| Ok(())) {
                gui.language = tags[index].clone();
                l10n::choose(gui.language.as_deref());
            }
        });
    }
    row(ui, &fl!("theme"), &fl!("theme", "hint"), Ok(()), |ui| {
        use crate::config::Theme;
        let themes = [
            (Theme::System, fl!("as-the-system"), String::new()),
            (Theme::Light, fl!("theme-light"), String::new()),
            (Theme::Dark, fl!("theme-dark"), String::new()),
        ];
        segments(ui, &mut gui.theme, &themes, |_| Ok(()));
    });
    row(ui, &fl!("files"), "", Ok(()), |ui| {
        ui.checkbox(&mut gui.reopen_files, fl!("reopen-files"));
    });
    row(ui, &fl!("scrolling"), "", Ok(()), |ui| {
        ui.checkbox(&mut gui.smooth_scrolling, fl!("smooth-scrolling"));
    });

    widgets::section(ui, &fl!("new-files"));
    row(ui, &fl!("make"), "", Ok(()), |ui| {
        let mut target = gui.target.unwrap_or_default();
        if segments(ui, &mut target, &file_settings::targets(), |_| Ok(())) {
            gui.target = Some(target);
        }
    });
    for (format, text) in
        [(Format::Webm, fl!("preset-for-webm")), (Format::Tgs, fl!("preset-for-tgs"))]
    {
        let names = file_settings::preset_names(presets);
        // what built-in presets do differs by format
        let choices: Vec<widgets::Choice<&str>> = names
            .iter()
            .map(|(name, description)| {
                let builtin = presets.get(name).is_ok_and(|preset| preset.path.is_none());
                let hint = file_settings::builtin_description(name, format).filter(|_| builtin);
                let description = description.as_deref().unwrap_or("").to_owned();
                (name.as_str(), file_settings::preset_label(name), hint.unwrap_or(description))
            })
            .collect();
        let current = config.preset_for(format).to_owned();
        let mut chosen = current.as_str();
        let broken = |name: &str| names.iter().any(|(n, d)| n == name && d.is_err());
        row(ui, &text, "", Ok(()), |ui| {
            segments(ui, &mut chosen, &choices, |name| {
                if broken(name) { Err(fl!("preset", "broken")) } else { Ok(()) }
            });
            widgets::chosen_hint(ui, chosen, &choices);
        });
        let picked = chosen.to_owned();
        if picked != current {
            dialog.default_preset = Some((format, picked));
        }
    }
}

impl App {
    /// Makes `name` the default preset for `format`. The items without a
    /// preset of their own follow it, and may then fit another way, as
    /// when they pick another preset: their own range to search goes
    /// where it counts in another unit.
    pub(crate) fn set_default_preset(&mut self, format: Format, name: String) {
        let fit = |choices: &_, config: &_| file_settings::fit_of(choices, &self.presets, config);
        let before: Vec<_> =
            self.items.iter().map(|item| fit(&item.choices, &self.config)).collect();
        match format {
            Format::Webm | Format::Webp => self.config.preset = Some(name),
            Format::Tgs => self.config.tgs_preset = Some(name),
        }
        for (item, before) in self.items.iter_mut().zip(before) {
            let after = fit(&item.choices, &self.config);
            file_settings::follow_fit(&mut item.choices.webm, before, after);
        }
    }
}

/// Whether the ffmpeg chosen, or at `path`, can make stickers, and what to
/// say of it. Runs it, so it may take a while.
fn check_ffmpeg(choice: FfmpegChoice, path: Option<&Path>, cancel: &CancelToken) -> (bool, String) {
    let backend = match Backend::select(choice, path) {
        Ok(backend) => backend,
        Err(err) => return (false, crate::jobs::failure(&err)),
    };
    match backend.capabilities(cancel) {
        Ok(caps) => {
            // "ffmpeg version 8.0 Copyright (c) …", or "… (built in)", where
            // builds from ffmpeg's git tags say "n8.0"
            let version = caps.version.trim_start_matches("ffmpeg version ");
            let version = version.split(" Copyright").next().unwrap_or(version).trim();
            let version = match version.strip_prefix('n') {
                Some(number) if number.starts_with(|c: char| c.is_ascii_digit()) => number,
                _ => version,
            };
            match caps.libvpx_vp9 {
                true => (true, fl!("ffmpeg-ready", version = version)),
                false => (false, fl!("ffmpeg-no-vp9", version = version)),
            }
        }
        Err(err) => (false, crate::jobs::failure(&err)),
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    #[test]
    fn saves_settings_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let at = path.clone();
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).build_eframe(move |_| {
            App::with(Config::default(), Some(at.clone()), Presets::builtin(), None)
        });
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        // a change applies at once, and stays in the file once saved
        harness.state_mut().config.gui.overwrite = true;
        harness.get_by_label("Save").click_accesskit();
        harness.run_steps(2);
        assert!(Config::load(&path).unwrap().gui.overwrite);
        harness.get_by_label_contains("Saved to");
        // closing keeps what changed
        harness.state_mut().config.gui.reopen_files = false;
        harness.get_by_label("✖").click_accesskit();
        harness.run_steps(2);
        assert!(
            !harness.state().settings_dialog.is_open() && !harness.state().config.gui.reopen_files
        );
        assert!(Config::load(&path).unwrap().gui.reopen_files);
    }

    #[test]
    fn drops_a_range_a_new_default_preset_fits_by_another_unit() {
        use tgradish_core::options::{Fit, Range};
        use tgradish_core::tgs;
        use tgradish_core::webm::Options;

        let dir = tempfile::tempdir().unwrap();
        let webm = Options { fit: Some(Fit::Crf), ..Options::default() };
        let (tgs, presets_dir) = (tgs::Options::default(), dir.path());
        tgradish_core::presets::save(presets_dir, "by quality", "", (&webm, &tgs), false).unwrap();
        let mut harness = crate::testing::harness(Config::default());
        let bitrates = Some(Range { min: 100.0, max: 400.0 });
        {
            let app = harness.state_mut();
            app.presets = Presets::load(Some(presets_dir)).unwrap();
            for (id, preset) in [(1, None), (2, Some("balanced"))] {
                let mut item = crate::item::Item::new(
                    id,
                    vec!["pig.mp4".into()],
                    false,
                    tgradish_core::telegram::Target::Sticker,
                );
                item.choices.preset = preset.map(str::to_owned);
                item.choices.webm.fit_range = bitrates;
                app.items.push(item);
            }
            app.settings_dialog.open();
        }
        harness.run_steps(2);
        // the WebM row's, the first
        harness.get_all_by_label("by quality").next().unwrap().click_accesskit();
        harness.run_steps(2);
        let app = harness.state();
        assert_eq!(app.config.preset.as_deref(), Some("by quality"));
        // the first follows the default, by quality now; the second has a
        // preset of its own, by bitrate still
        let ranges: Vec<_> = app.items.iter().map(|item| item.choices.webm.fit_range).collect();
        assert_eq!(ranges, [None, bitrates]);
    }

    #[test]
    fn keeps_no_folder_without_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let at = path.clone();
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).build_eframe(move |_| {
            App::with(Config::default(), Some(at.clone()), Presets::builtin(), None)
        });
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label(&fl!("in-a-folder")).click_accesskit();
        harness.run_steps(2);
        // in a folder, said to be unchosen, yet none is kept
        harness.get_by_label(&fl!("in-a-folder", "unchosen"));
        assert_eq!(harness.state().config.gui.output_dir, None);
        harness.get_by_label("Save").click_accesskit();
        harness.run_steps(2);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("output-dir"));
    }

    #[test]
    fn checks_ffmpeg_while_the_window_draws() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.ffmpeg.path = Some(dir.path().join("no-ffmpeg-here"));
        let mut harness = crate::testing::harness(config);
        // the command line's choice is used, and said to be, but the
        // setting is what is checked
        harness.state_mut().ffmpeg_override = (Some(FfmpegChoice::System), None);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label_contains("ffmpeg its command line gave");
        harness.get_by_label("Check").click_accesskit();
        harness.run_steps(2);
        // what it finds is taken in as the window draws
        for _ in 0..500 {
            if harness.state().settings_dialog.ffmpeg_status.is_some() {
                break;
            }
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // in the window's words, as reading a file without it would say
        let status = harness.state().settings_dialog.ffmpeg_status.clone();
        assert!(
            matches!(&status, Some((false, said)) if said.starts_with("ffmpeg wasn't found")),
            "{status:?}"
        );
    }
}

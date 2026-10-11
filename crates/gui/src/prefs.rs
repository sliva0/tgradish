//! The settings dialog: ffmpeg, where results go, and what new files start
//! with. Changes apply at once; OK keeps them in `config.toml`, and Cancel
//! goes back to how they were.

use std::path::{Path, PathBuf};

use eframe::egui;
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::{CancelToken, FfmpegChoice};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;

use crate::settings;
use crate::widgets::{self, note, row, segments};

#[derive(Default)]
pub struct Prefs {
    /// The settings as they were when the dialog opened, for Cancel; the
    /// dialog is open while they are kept.
    before: Option<Config>,
    /// Whether the window of tgradish's version and licences is open.
    pub about: bool,
    ffmpeg_status: Option<(bool, String)>,
    /// Why OK couldn't save the settings.
    error: Option<String>,
}

impl Prefs {
    pub fn open(&mut self, config: &Config) {
        *self = Prefs { before: Some(config.clone()), about: self.about, ..Prefs::default() };
    }
}

/// Shows the dialog if it is open. Returns what to tell the user once it
/// closed with settings that couldn't be kept.
pub fn show(
    ctx: &egui::Context,
    prefs: &mut Prefs,
    config: &mut Config,
    config_path: Option<&Path>,
    presets: &Presets,
) -> Option<String> {
    let before = prefs.before.clone()?;
    let mut ok = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
        ui.set_width(620.0);
        ui.heading("Settings");
        // the dialog fits the window, its settings scrolling within
        let room = (ctx.content_rect().height() - 150.0).max(200.0);
        egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
            sections(ui, prefs, config, presets);
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("About and licenses…").clicked() {
                prefs.about = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Breeze's order: OK, then Cancel
                cancel = ui.button("Cancel").on_hover_text("Back to how they were").clicked();
                ok = ui.button("OK").on_hover_text("Keep these settings").clicked();
                if let Some(error) = &prefs.error {
                    let text = egui::RichText::new(error).color(widgets::BAD);
                    ui.add(egui::Label::new(text).truncate());
                }
            });
        });
    });
    // Escape cancels; a click beside the dialog doesn't, so changes aren't
    // lost to a stray one
    let escape = modal.is_top_modal
        && !modal.any_popup_open
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if cancel || escape {
        *config = before;
        prefs.before = None;
        return None;
    }
    if !ok {
        return None;
    }
    match config_path.map(|path| config.save(path)) {
        Some(Ok(())) => {
            prefs.before = None;
            None
        }
        Some(Err(err)) => {
            prefs.error = Some(format!("Couldn't save: {err}"));
            None
        }
        None => {
            prefs.before = None;
            Some(
                "There is no home folder to keep settings in: they last until tgradish closes"
                    .into(),
            )
        }
    }
}

fn sections(ui: &mut egui::Ui, prefs: &mut Prefs, config: &mut Config, presets: &Presets) {
    widgets::section(ui, "ffmpeg");
    let builtin = Backend::select(FfmpegChoice::Builtin, None).is_ok();
    row(ui, "Use", "WebM stickers are made with ffmpeg", Ok(()), |ui| {
        let choices = [
            (
                FfmpegChoice::Auto,
                "Automatic",
                "The ffmpeg built in if this build has one, else the system's",
            ),
            (FfmpegChoice::Builtin, "Built in", "The ffmpeg inside tgradish"),
            (
                FfmpegChoice::System,
                "The system's",
                "ffmpeg and ffprobe on PATH; allows raw ffmpeg arguments",
            ),
        ];
        segments(ui, &mut config.ffmpeg.choice, &choices, |choice| match choice {
            FfmpegChoice::Builtin if !builtin => Err("This build of tgradish has no ffmpeg in it"),
            _ => Ok(()),
        });
    });
    let hint =
        "An ffmpeg executable, or a folder with ffmpeg and ffprobe; overrides the choice above";
    row(ui, "Program", hint, Ok(()), |ui| {
        let ffmpeg = &mut config.ffmpeg;
        ui.horizontal(|ui| {
            let mut text =
                ffmpeg.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            let edit = egui::TextEdit::singleline(&mut text)
                .hint_text("found on PATH")
                .desired_width(280.0);
            if ui.add(edit).changed() {
                ffmpeg.path = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
            }
            if ui.button("Choose…").clicked()
                && let Some(file) = rfd::FileDialog::new().pick_file()
            {
                ffmpeg.path = Some(file);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Check").on_hover_text("Whether this ffmpeg can make stickers").clicked() {
                prefs.ffmpeg_status = Some(check_ffmpeg(config));
            }
            if let Some((ok, status)) = &prefs.ffmpeg_status {
                ui.colored_label(if *ok { widgets::GOOD } else { widgets::BAD }, status);
            }
        });
    });

    widgets::section(ui, "Results");
    let gui = &mut config.gui;
    row(ui, "Save them", "", Ok(()), |ui| {
        let mut elsewhere = gui.output_dir.is_some();
        let choices = [(false, "Next to each input", ""), (true, "In a folder", "")];
        if segments(ui, &mut elsewhere, &choices, |_| Ok(())) {
            gui.output_dir = elsewhere.then(PathBuf::new);
        }
        ui.add_enabled_ui(elsewhere, |ui| {
            ui.horizontal(|ui| {
                let mut text =
                    gui.output_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                let edit = egui::TextEdit::singleline(&mut text)
                    .hint_text("a folder")
                    .desired_width(280.0);
                if ui.add(edit).changed() {
                    gui.output_dir = Some(PathBuf::from(text.trim()));
                }
                if ui.button("Choose…").clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    gui.output_dir = Some(dir);
                }
            });
        });
        if gui.output_dir.as_ref().is_some_and(|dir| dir.as_os_str().is_empty()) {
            note(ui, "Until a folder is chosen, results are saved next to each input");
        }
    });
    let hint = "Results tgradish made are always replaced when made again";
    row(ui, "Existing files", hint, Ok(()), |ui| {
        ui.checkbox(&mut gui.overwrite, "Replace any file that is in the way");
    });

    widgets::section(ui, "Window");
    let hint = "The button beside ⚙ switches between light and dark too";
    row(ui, "Theme", hint, Ok(()), |ui| {
        use tgradish_core::config::Theme;
        let themes = [
            (Theme::System, "As the system", ""),
            (Theme::Light, "Light", ""),
            (Theme::Dark, "Dark", ""),
        ];
        segments(ui, &mut gui.theme, &themes, |_| Ok(()));
    });
    row(ui, "Files", "", Ok(()), |ui| {
        let mut reopen = !gui.forget_files;
        let text = "Reopen the files of last time, with their settings";
        if ui.checkbox(&mut reopen, text).changed() {
            gui.forget_files = !reopen;
        }
    });
    row(ui, "Scrolling", "", Ok(()), |ui| {
        ui.checkbox(&mut gui.smooth_scrolling, "Smooth: ease it over a few frames");
    });

    widgets::section(ui, "New files");
    row(ui, "Make", "", Ok(()), |ui| {
        let mut target = gui.target.unwrap_or_default();
        let targets = [(Target::Sticker, "Sticker", ""), (Target::Emoji, "Custom emoji", "")];
        if segments(ui, &mut target, &targets, |_| Ok(())) {
            gui.target = Some(target);
        }
    });
    for (format, text) in [(Format::Webm, "Preset for WebM"), (Format::Tgs, "Preset for TGS")] {
        let names = settings::preset_names(presets);
        let labels: Vec<String> =
            names.iter().map(|(name, _)| settings::capitalised(name)).collect();
        // what built-in presets do differs by format
        let choices: Vec<widgets::Choice<&str>> = names
            .iter()
            .zip(&labels)
            .map(|((name, description), label)| {
                let builtin = presets.get(name).is_ok_and(|preset| preset.path.is_none());
                let hint = settings::builtin(name, format).filter(|_| builtin);
                (
                    name.as_str(),
                    label.as_str(),
                    hint.unwrap_or(description.as_deref().unwrap_or("")),
                )
            })
            .collect();
        let current = config.preset_for(format).to_owned();
        let mut chosen = current.as_str();
        let broken = |name: &str| names.iter().any(|(n, d)| n == name && d.is_err());
        row(ui, text, "", Ok(()), |ui| {
            segments(ui, &mut chosen, &choices, |name| {
                if broken(name) { Err("This preset's file is broken") } else { Ok(()) }
            });
            widgets::chosen_hint(ui, chosen, &choices);
        });
        let picked = chosen.to_owned();
        if picked != current {
            match format {
                Format::Webm | Format::Image => config.preset = Some(picked),
                Format::Tgs => config.tgs_preset = Some(picked),
            }
        }
    }
}

fn check_ffmpeg(config: &Config) -> (bool, String) {
    let backend = match Backend::select(config.ffmpeg.choice, config.ffmpeg.path.as_deref()) {
        Ok(backend) => backend,
        Err(err) => return (false, err.to_string()),
    };
    match backend.capabilities(&CancelToken::new()) {
        Ok(caps) if caps.libvpx_vp9 => (true, format!("ffmpeg {} with VP9: ready", caps.version)),
        Ok(caps) => (false, format!("ffmpeg {} can't encode VP9 with libvpx", caps.version)),
        Err(err) => (false, err.to_string()),
    }
}

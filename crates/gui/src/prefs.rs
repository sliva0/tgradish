//! The settings window: ffmpeg, where results go, and what new files start
//! with. Saved to `config.toml`.

use std::path::{Path, PathBuf};

use eframe::egui;
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::{CancelToken, FfmpegChoice};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;

use crate::settings;
use crate::widgets::{self, label, note, radios, segments};

#[derive(Default)]
pub struct Prefs {
    pub open: bool,
    ffmpeg_status: Option<(bool, String)>,
    saved: Option<String>,
}

fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).min_col_width(120.0).spacing([12.0, 10.0]).show(ui, rows);
}

/// Shows the window if it is open.
pub fn show(
    ctx: &egui::Context,
    prefs: &mut Prefs,
    config: &mut Config,
    config_path: Option<&Path>,
    presets: &Presets,
) {
    let mut open = prefs.open;
    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            widgets::section(ui, "ffmpeg");
            grid(ui, "prefs-ffmpeg", |ui| {
                label(ui, "Use", "WebM stickers are made with ffmpeg");
                let ffmpeg = &mut config.ffmpeg;
                radios(
                    ui,
                    &mut ffmpeg.choice,
                    &[
                        (FfmpegChoice::Auto, "Built in if there is one", "Else the one on the system"),
                        (FfmpegChoice::Builtin, "Built in", "The ffmpeg inside tgradish"),
                        (FfmpegChoice::System, "The system's", "ffmpeg and ffprobe on PATH; allows raw ffmpeg arguments"),
                    ],
                );
                ui.end_row();
                label(ui, "Program", "An ffmpeg executable, or a folder with ffmpeg and ffprobe; overrides the choice above");
                ui.horizontal(|ui| {
                    let mut text = ffmpeg.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                    let edit = egui::TextEdit::singleline(&mut text).hint_text("found on PATH").desired_width(260.0);
                    if ui.add(edit).changed() {
                        ffmpeg.path = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
                    }
                    if ui.button("Choose…").clicked()
                        && let Some(file) = rfd::FileDialog::new().pick_file()
                    {
                        ffmpeg.path = Some(file);
                    }
                });
                ui.end_row();
                ui.label("");
                ui.horizontal(|ui| {
                    if ui.button("Check").clicked() {
                        prefs.ffmpeg_status = Some(check_ffmpeg(config));
                    }
                    if let Some((ok, status)) = &prefs.ffmpeg_status {
                        ui.colored_label(if *ok { widgets::good(ui) } else { widgets::bad(ui) }, status);
                    }
                });
                ui.end_row();
            });

            widgets::section(ui, "Results");
            grid(ui, "prefs-results", |ui| {
                let gui = &mut config.gui;
                label(ui, "Save them", "");
                ui.vertical(|ui| {
                    let mut elsewhere = gui.output_dir.is_some();
                    ui.radio_value(&mut elsewhere, false, "Next to each input");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut elsewhere, true, "In");
                        let mut text = gui.output_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut text).desired_width(240.0)).changed() {
                            gui.output_dir = Some(PathBuf::from(text.trim()));
                            elsewhere = true;
                        }
                        if ui.button("Choose…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            gui.output_dir = Some(dir);
                            elsewhere = true;
                        }
                    });
                    if !elsewhere {
                        gui.output_dir = None;
                    } else if gui.output_dir.is_none() {
                        gui.output_dir = Some(PathBuf::new());
                    }
                });
                ui.end_row();
                label(ui, "Existing files", "Results tgradish made are always replaced when made again");
                ui.checkbox(&mut gui.overwrite, "Replace any file that is in the way");
                ui.end_row();
            });

            widgets::section(ui, "Window");
            grid(ui, "prefs-window", |ui| {
                label(ui, "Theme", "The button beside ⚙ switches between light and dark too");
                use tgradish_core::config::Theme;
                let themes = [
                    (Theme::System, "As the system", ""),
                    (Theme::Light, "Light", ""),
                    (Theme::Dark, "Dark", ""),
                ];
                segments(ui, &mut config.gui.theme, &themes, |_| Ok(()));
                ui.end_row();
                label(ui, "Files", "");
                let mut reopen = !config.gui.forget_files;
                if ui.checkbox(&mut reopen, "Reopen the files of last time, with their settings").changed() {
                    config.gui.forget_files = !reopen;
                }
                ui.end_row();
                label(ui, "Scrolling", "");
                ui.checkbox(&mut config.gui.smooth_scrolling, "Smooth: ease it over a few frames");
                ui.end_row();
            });

            widgets::section(ui, "New files");
            grid(ui, "prefs-new", |ui| {
                label(ui, "Make", "");
                let mut target = config.gui.target.unwrap_or_default();
                let targets = [(Target::Sticker, "Stickers", ""), (Target::Emoji, "Custom emoji", "")];
                if segments(ui, &mut target, &targets, |_| Ok(())) {
                    config.gui.target = Some(target);
                }
                ui.end_row();
                for (format, text) in [(Format::Webm, "Preset for WebM"), (Format::Tgs, "Preset for TGS")] {
                    label(ui, text, "");
                    let names = settings::preset_names(presets);
                    let labels: Vec<String> = names.iter().map(|(name, _)| settings::capitalised(name)).collect();
                    // what built-in presets do differs by format
                    let choices: Vec<widgets::Choice<&str>> = names
                        .iter()
                        .zip(&labels)
                        .map(|((name, description), label)| {
                            let builtin = presets.get(name).is_ok_and(|preset| preset.path.is_none());
                            let hint = settings::builtin(name, format).filter(|_| builtin);
                            (name.as_str(), label.as_str(), hint.unwrap_or(description.as_deref().unwrap_or("")))
                        })
                        .collect();
                    let current = config.preset_for(format).to_owned();
                    let mut chosen = current.as_str();
                    let broken = |name: &str| names.iter().any(|(n, d)| n == name && d.is_err());
                    ui.vertical(|ui| {
                        segments(ui, &mut chosen, &choices, |name| if broken(name) { Err("This preset's file is broken") } else { Ok(()) });
                        widgets::chosen_hint(ui, chosen, &choices);
                    });
                    let picked = chosen.to_owned();
                    if picked != current {
                        match format {
                            Format::Webm => config.preset = Some(picked),
                            Format::Tgs => config.tgs_preset = Some(picked),
                        }
                    }
                    ui.end_row();
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save").on_hover_text("Keep these settings for next time").clicked() {
                    prefs.saved = Some(match config_path {
                        Some(path) => match config.save(path) {
                            Ok(()) => format!("Saved to {}", path.display()),
                            Err(err) => format!("Couldn't save: {err}"),
                        },
                        None => "No place to save settings: no home directory".to_owned(),
                    });
                }
                match &prefs.saved {
                    Some(saved) => note(ui, saved.clone()),
                    None => note(ui, "Changes apply now; Save keeps them"),
                }
            });
        });
    prefs.open = open;
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

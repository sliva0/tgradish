//! The settings dialog: ffmpeg, where results go, and what new files start
//! with. Changes apply at once; Save keeps them in `config.toml`.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Vec2};
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::{CancelToken, FfmpegChoice};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;

use crate::settings;
use crate::widgets::{self, note, row, segments};

#[derive(Default)]
pub struct Prefs {
    open: bool,
    /// Whether the window of tgradish's version and licences is open.
    pub about: bool,
    ffmpeg_status: Option<(bool, String)>,
    /// What Save did: where the settings went, or why they didn't.
    saved: Option<Result<String, String>>,
    /// What the fields show while empty, found once the dialog opens: the
    /// ffmpeg on PATH, and a folder results could go to.
    placeholders: (String, String),
}

impl Prefs {
    pub fn open(&mut self) {
        let ffmpeg = match tgradish_core::ffmpeg::locate(FfmpegChoice::System, None) {
            Ok(found) => format!("{}, found on PATH", found.ffmpeg.display()),
            Err(_) if cfg!(windows) => r"For example: C:\ffmpeg\bin".to_owned(),
            Err(_) => "For example: /usr/local/bin/ffmpeg".to_owned(),
        };
        let folder = tgradish_core::paths::pictures_dir()
            .map(|dir| format!("For example: {}", dir.join("Stickers").display()))
            .unwrap_or_default();
        *self = Prefs {
            open: true,
            about: self.about,
            placeholders: (ffmpeg, folder),
            ..Prefs::default()
        };
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// Shows the dialog if it is open.
pub fn show(
    ctx: &egui::Context,
    prefs: &mut Prefs,
    config: &mut Config,
    config_path: Option<&Path>,
    presets: &Presets,
) {
    if !prefs.open {
        return;
    }
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
        ui.set_width(620.0);
        ui.horizontal(|ui| {
            ui.heading("Settings");
            // in the corner, square, level with the heading
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let side = ui.spacing().interact_size.y;
                let cross = egui::Button::new("✖").frame(false).min_size(Vec2::splat(side));
                close = ui.add(cross).on_hover_text("Close").clicked();
            });
        });
        // the dialog fits the window, its settings scrolling within
        let room = (ctx.content_rect().height() - 150.0).max(200.0);
        egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
            sections(ui, prefs, config, presets);
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Save").on_hover_text("Keep these settings for next time").clicked() {
                prefs.saved = Some(match config_path {
                    Some(path) => match config.save(path) {
                        Ok(()) => Ok(format!("Saved to {}", path.display())),
                        Err(err) => Err(format!("Couldn't save: {err}")),
                    },
                    None => Err("There is no home folder to keep settings in".to_owned()),
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("About and licenses…").clicked() {
                    prefs.about = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // cut short where it is long, as a path can be
                    let weak = |text: &str| egui::RichText::new(text).weak().size(12.5);
                    let text = match &prefs.saved {
                        Some(Ok(saved)) => weak(saved),
                        Some(Err(error)) => egui::RichText::new(error).color(widgets::BAD),
                        None => weak("Changes apply now; Save keeps them between restarts"),
                    };
                    ui.add(egui::Label::new(text).truncate());
                });
            });
        });
    });
    // changes stay either way, until tgradish closes unless saved
    if close || modal.should_close() {
        prefs.open = false;
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
                .hint_text(prefs.placeholders.0.as_str())
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
                let colour = if *ok { widgets::GOOD } else { widgets::BAD };
                ui.add(egui::Label::new(egui::RichText::new(status).color(colour)).truncate());
            }
        });
    });

    widgets::section(ui, "Results");
    let gui = &mut config.gui;
    let placeholder = &prefs.placeholders.1;
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
                    .hint_text(placeholder.as_str())
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
        Ok(caps) => {
            // "ffmpeg version 8.0 Copyright (c) …", or "… (built in)"
            let version = caps.version.trim_start_matches("ffmpeg version ");
            let version = version.split(" Copyright").next().unwrap_or(version).trim();
            match caps.libvpx_vp9 {
                true => (true, format!("ffmpeg {version} with VP9: ready")),
                false => (false, format!("ffmpeg {version} can't encode VP9 with libvpx")),
            }
        }
        Err(err) => (false, err.to_string()),
    }
}

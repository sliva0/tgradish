//! The tgradish window: a list of files, each with its own settings, a
//! preview to crop and trim it, and its result. It converts with
//! `tgradish-core` directly, like the CLI.

// What the crate offers is what this file exports; everything else is
// the window's own, which the lint keeps from being marked public.
#![warn(unreachable_pub)]

mod about;
mod accent;
mod actions;
mod app;
mod canvas;
mod config;
mod cuts;
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
mod dbus;
#[cfg(all(unix, not(target_os = "macos")))]
mod dnd;
mod fallback;
mod file_settings;
mod files;
mod fonts;
mod icon;
mod inset;
mod inspect;
mod item;
mod jobs;
mod keys;
mod l10n;
mod media;
mod outputs;
mod predict;
mod preset_dialogs;
mod preview;
mod result;
#[cfg(test)]
mod screenshots;
mod session;
mod settings_dialog;
#[cfg(test)]
mod strings;
mod system;
#[cfg(test)]
pub(crate) mod testing;
mod theme;
mod timeline;
#[cfg(all(unix, not(target_os = "macos")))]
mod toplevel_icon;
mod widgets;
mod work;

use std::path::PathBuf;

use eframe::egui;
use tgradish_core::ffmpeg::FfmpegChoice;

pub use crate::about::notices;
use crate::app::App;

/// The narrowest the window goes, which holds the list of files, the preview
/// and the column beside it.
pub(crate) const MIN_WIDTH: f32 = 920.0;

/// How the window starts, from the command line: by default with the
/// user's own config and the ffmpeg it chooses.
#[derive(Debug, Default)]
pub struct Launch {
    /// The `config.toml` to read and save; the user's own when `None`.
    pub(crate) config_path: Option<PathBuf>,
    /// Which ffmpeg to use instead of the config's.
    pub(crate) ffmpeg_choice: Option<FfmpegChoice>,
    /// An ffmpeg executable or directory to use instead of the config's.
    pub(crate) ffmpeg_path: Option<PathBuf>,
}

impl Launch {
    /// Reads and saves the `config.toml` at `path`, or with `None` the
    /// user's own.
    pub fn config_path(self, path: Option<PathBuf>) -> Launch {
        Launch { config_path: path, ..self }
    }

    /// Uses the ffmpeg `choice` says, and the executable or directory at
    /// `path`, over the config's where either is given.
    pub fn ffmpeg(self, choice: Option<FfmpegChoice>, path: Option<PathBuf>) -> Launch {
        Launch { ffmpeg_choice: choice, ffmpeg_path: path, ..self }
    }
}

/// Why the window couldn't open, or stopped.
#[derive(Debug)]
pub struct Error(eframe::Error);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {
    // its own message is eframe's, which is said once
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.0)
    }
}

/// Opens the window and runs until it is closed.
pub fn run(launch: Launch) -> Result<(), Error> {
    tgradish_core::mark::set_client(tgradish_core::mark::Client::Window);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tgradish")
            .with_app_id("tgradish")
            .with_icon(std::sync::Arc::new(icon::window()))
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([MIN_WIDTH, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "tgradish",
        options,
        Box::new(|cc| {
            let app = App { accent: Some(accent::Accent::watch(&cc.egui_ctx)), ..App::new(launch) };
            app.style(&cc.egui_ctx);
            #[cfg(all(unix, not(target_os = "macos")))]
            let app = App {
                drops: dnd::Drops::start(cc, &cc.egui_ctx),
                _toplevel_icon: cc.winit_window().and_then(|window| toplevel_icon::set(window)),
                ..app
            };
            // the icon applies with the window's next frame
            cc.egui_ctx.request_repaint();
            Ok(Box::new(app))
        }),
    )
    .map_err(Error)
}

/// Shows an error in a dialog, for when there is no terminal to print to.
/// Printed as well: on Linux the dialog is zenity's, which may be missing,
/// and a log may still be kept of what is printed.
pub fn show_error(message: &str) {
    eprintln!("{message}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("tgradish")
        .set_description(message)
        .show();
}

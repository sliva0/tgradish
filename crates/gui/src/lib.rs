//! The tgradish window: a list of files, each with its own settings, a
//! preview to crop and trim it, and its result. It converts with
//! `tgradish-core` directly, like the CLI.

mod about;
mod accent;
mod actions;
mod app;
mod canvas;
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
mod dbus;
#[cfg(all(unix, not(target_os = "macos")))]
mod dnd;
mod fallback;
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
mod prefs;
mod preset_dialogs;
mod preview;
mod result;
#[cfg(test)]
mod screenshots;
mod session;
mod settings;
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

/// How the window starts, from the command line.
#[derive(Debug, Default)]
pub struct Launch {
    /// The `config.toml` to read and save; the user's own when `None`.
    pub config_path: Option<PathBuf>,
    /// Which ffmpeg to use instead of the config's.
    pub ffmpeg_choice: Option<FfmpegChoice>,
    /// An ffmpeg executable or directory to use instead of the config's.
    pub ffmpeg_path: Option<PathBuf>,
}

/// Opens the window and runs until it is closed.
pub fn run(launch: Launch) -> eframe::Result<()> {
    tgradish_core::mark::set_client(tgradish_core::mark::Client::Window);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tgradish")
            .with_app_id("tgradish")
            .with_icon(std::sync::Arc::new(icon::window()))
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0])
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
}

/// Shows an error in a dialog, for when there is no terminal to print to.
pub fn show_error(message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("tgradish")
        .set_description(message)
        .show();
}

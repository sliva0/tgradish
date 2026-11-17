//! The tgradish GUI: a list of files, each with its own settings, a
//! preview to crop and trim it, and its result. It converts with
//! `tgradish-core` directly, like the CLI.

// What the crate offers is what this file exports; everything else is
// the crate's own, which the lint keeps from being marked public.
#![warn(unreachable_pub)]

mod about;
mod accent;
mod actions;
#[cfg(all(unix, not(target_os = "macos")))]
mod activation;
mod app;
mod banner;
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
mod instance;
mod item;
mod jobs;
mod keys;
mod l10n;
mod media;
mod motion;
mod outputs;
mod predict;
mod preset_dialogs;
mod preview;
mod result;
mod scaling;
#[cfg(test)]
mod scenes;
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
/// The lowest it goes.
pub(crate) const MIN_HEIGHT: f32 = 560.0;

/// How the GUI starts, from the command line: by default with the
/// user's own config and the ffmpeg it chooses.
#[derive(Debug, Default)]
pub struct Launch {
    /// The `config.toml` to read and save; the user's own when `None`.
    pub(crate) config_path: Option<PathBuf>,
    /// Which ffmpeg to use instead of the config's.
    pub(crate) ffmpeg_choice: Option<FfmpegChoice>,
    /// An ffmpeg executable or directory to use instead of the config's.
    pub(crate) ffmpeg_path: Option<PathBuf>,
    /// Files to open as the GUI starts.
    pub(crate) files: Vec<PathBuf>,
}

impl Launch {
    /// Reads and saves the `config.toml` at `path`, or with `None` the
    /// user's own.
    pub fn config_path(self, path: Option<PathBuf>) -> Launch {
        Launch { config_path: path, ..self }
    }

    /// Uses the ffmpeg `choice` says, and the executable or directory at
    /// `path`, over the config's where either is given, for this run
    /// alone: as the command line or `TGRADISH_FFMPEG` gives them.
    pub fn ffmpeg(self, choice: Option<FfmpegChoice>, path: Option<PathBuf>) -> Launch {
        Launch { ffmpeg_choice: choice, ffmpeg_path: path, ..self }
    }

    /// Opens `files` as the GUI starts, each as a dropped one
    /// is, after those of last time: pictures, videos, or folders of
    /// frames.
    pub fn files(self, files: Vec<PathBuf>) -> Launch {
        Launch { files, ..self }
    }
}

/// Why the GUI couldn't start, or stopped.
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

/// What came of [`run`], for whoever started it to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// Its window opened, and has been closed since.
    Opened,
    /// A GUI ran already: it took this many files to add, maybe none, and
    /// was brought forward.
    HandedOver { files: usize },
}

/// Asked to close, by [`close`].
static CLOSING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The window's, once [`run`] has opened it.
static OPEN: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

/// Closes the window, from any thread, as closing it does: conversions
/// stop, taking their temporary files and ffmpeg with them, the session
/// is kept, and [`run`] returns. For a signal to end, as SIGTERM when the
/// session ends, SIGHUP when the terminal it was started from closes, or
/// Ctrl-C there.
/// Asked before the window opens, it closes as it opens.
pub fn close() {
    CLOSING.store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(ctx) = OPEN.get() {
        close_now(ctx);
    }
}

fn close_now(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    ctx.request_repaint();
}

/// Runs the GUI until its window is closed, or where one runs already,
/// hands it the files to add and brings it forward instead.
pub fn run(launch: Launch) -> Result<Started, Error> {
    tgradish_core::mark::set_client(tgradish_core::mark::Client::Gui);
    // as the command line names them, from where it was started
    let files = launch.files.iter().map(|file| std::path::absolute(file).unwrap_or(file.clone()));
    let launch = Launch { files: files.collect(), ..launch };
    let request = instance::Request::from_env(launch.files.clone());
    let (store, listener) = match app::session_path().map(|path| instance::start(path, &request)) {
        Some(instance::Start::HandedOver) => {
            return Ok(Started::HandedOver { files: launch.files.len() });
        }
        Some(instance::Start::First { store, listener }) => (Some(store), listener),
        // one that couldn't be reached keeps the session, and this one none
        Some(instance::Start::Second) | None => (None, None),
    };
    // as it was last time, where the session says
    let window = app::session_path().and_then(|path| session::window(&path));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("tgradish")
        .with_app_id("tgradish")
        .with_icon(std::sync::Arc::new(icon::window()))
        .with_inner_size(window.map_or([1280.0, 820.0], |window| window.size))
        .with_min_inner_size([MIN_WIDTH, MIN_HEIGHT])
        .with_maximized(window.is_some_and(|window| window.maximized))
        .with_drag_and_drop(true);
    // where the system lets a window say where it goes, as Wayland doesn't
    if let Some(position) = window.and_then(|window| window.position) {
        viewport = viewport.with_position(position);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "tgradish",
        options,
        Box::new(|cc| {
            let app = App {
                accent: Some(accent::Accent::watch(&cc.egui_ctx)),
                system_motion: Some(motion::System::watch(&cc.egui_ctx)),
                ..App::new(launch, store, listener)
            };
            app.style(&cc.egui_ctx);
            if let Some(listener) = &app.instance {
                listener.attach(&cc.egui_ctx);
                #[cfg(all(unix, not(target_os = "macos")))]
                if let Some(window) = cc.winit_window() {
                    listener.raise(window);
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            let app = App {
                drops: dnd::Drops::start(cc, &cc.egui_ctx),
                _toplevel_icon: cc.winit_window().and_then(|window| toplevel_icon::set(window)),
                ..app
            };
            // the icon applies with the next frame
            cc.egui_ctx.request_repaint();
            // asked to close as it opened: after, so either sees the other
            let _ = OPEN.set(cc.egui_ctx.clone());
            if CLOSING.load(std::sync::atomic::Ordering::SeqCst) {
                close_now(&cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
    .map(|()| Started::Opened)
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

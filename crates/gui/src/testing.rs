//! Shared test helpers for the GUI: test harness setup, settling and
//! conversion helpers, and synthetic test media. Test videos are generated
//! with `ffmpeg` from `PATH`; if `ffmpeg` is missing, video tests are skipped
//! (or fail when `TGRADISH_REQUIRE_FFMPEG` is set).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::Config;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use tgradish_core::presets::Presets;

use crate::app::App;
use crate::item::Item;
use crate::jobs::Job;
use crate::l10n::fl;

/// Writes a 4x4 PNG with a 2x2 square of `colour` in the centre.
pub(crate) fn square(path: &Path, colour: [u8; 4]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, 4, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let inside = |i: usize| (1..3).contains(&(i % 4)) && (1..3).contains(&(i / 4));
    let pixels: Vec<u8> = (0..16).flat_map(|i| if inside(i) { colour } else { [0; 4] }).collect();
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

/// Creates a temporary pasted image and its directory, matching what the
/// clipboard passes to `App::add_pasted`.
pub(crate) fn pasted_image() -> (Vec<PathBuf>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join(tgradish_core::clipboard::PASTED_IMAGE_NAME);
    square(&image, [255, 0, 0, 255]);
    (vec![image], dir)
}

pub(crate) fn harness(config: Config) -> Harness<'static, App> {
    Harness::builder()
        .with_size([1280.0, 820.0])
        .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin()))
}

/// Simulates eframe and `egui-winit` driving the GUI on a fixed-pixel screen:
/// keeps the window size constant in physical pixels as `zoom_factor` changes,
/// runs `raw_input_hook` before each frame, and sends pointer button events at
/// the point coordinates recorded on the last pointer move (matching
/// `egui-winit`, which reuses the last point position even if `zoom_factor`
/// changed in between).
pub(crate) struct Screen {
    pub(crate) harness: Harness<'static, App>,
    /// Window size in physical pixels.
    size: egui::Vec2,
    /// Last pointer position in points as cached by `egui-winit`.
    said: Option<egui::Pos2>,
}

impl Screen {
    /// Builds a GUI window of `size` pixels from `config`, optionally opens
    /// Settings, and runs 16 frames so layouts settle.
    pub(crate) fn new(config: Config, size: [f32; 2], settings: bool) -> Self {
        let harness = Harness::builder()
            .with_size(size)
            .with_step_dt(1.0 / 60.0)
            .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin()));
        let mut screen = Screen { harness, size: size.into(), said: None };
        if settings {
            screen.harness.state_mut().settings_dialog.open();
        }
        screen.frames(16);
        screen
    }

    /// Runs a single frame with `events`, calling `raw_input_hook` like eframe.
    pub(crate) fn frame(&mut self, events: Vec<egui::Event>) {
        let zoom = self.harness.ctx.zoom_factor();
        self.harness.set_size(self.size / zoom);
        let ctx = self.harness.ctx.clone();
        let mut input = self.harness.input().clone();
        input.events.extend(events);
        eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut input);
        *self.harness.input_mut() = input;
        self.harness.step();
    }

    pub(crate) fn frames(&mut self, frames: usize) {
        for _ in 0..frames {
            self.frame(Vec::new());
        }
    }

    /// Moves the pointer to `pixels` and advances one frame.
    pub(crate) fn move_to(&mut self, pixels: egui::Pos2) {
        let at = pixels / self.harness.ctx.zoom_factor();
        self.said = Some(at);
        self.frame(vec![egui::Event::PointerMoved(at)]);
    }

    /// Presses and releases the primary button at the current pointer position
    /// across two frames.
    pub(crate) fn click(&mut self) {
        for pressed in [true, false] {
            self.press(pressed);
        }
    }

    /// Sends a primary button press or release at the last pointer position
    /// and advances one frame.
    pub(crate) fn press(&mut self, pressed: bool) {
        let pos = self.said.expect("the pointer is on the window");
        let (button, modifiers) = (egui::PointerButton::Primary, egui::Modifiers::NONE);
        self.frame(vec![egui::Event::PointerButton { pos, button, pressed, modifiers }]);
    }

    /// Presses `key` with the command modifier and advances one frame.
    pub(crate) fn command(&mut self, key: egui::Key) {
        let modifiers = egui::Modifiers::COMMAND;
        let (repeat, physical_key) = (false, None);
        let pressed = egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers };
        self.frame(vec![pressed]);
    }

    /// Returns the centre of widget `label` in physical pixels.
    pub(crate) fn pixels(&self, label: &str) -> egui::Pos2 {
        self.harness.get_by_label(label).rect().center() * self.harness.ctx.zoom_factor()
    }

    /// Returns egui's current hover position in points.
    pub(crate) fn pointer(&self) -> Option<egui::Pos2> {
        self.harness.ctx.input(|input| input.pointer.hover_pos())
    }

    /// Returns true if egui's pointer is inside widget `label`.
    pub(crate) fn on(&self, label: &str) -> bool {
        let rect = self.harness.get_by_label(label).rect();
        self.pointer().is_some_and(|at| rect.contains(at))
    }
}

/// Joins all active banner messages with newlines for test assertions.
pub(crate) fn said(app: &App) -> String {
    let rows = app.banner.rows.iter();
    rows.flat_map(|said| said.message.texts()).collect::<Vec<_>>().join("\n")
}

/// Clicks the selected item's Convert button by matching its keyboard shortcut
/// (which is independent of the active UI language).
pub(crate) fn convert(harness: &Harness<App>) {
    let keys = crate::keys::keys_text(crate::keys::Command::Convert);
    harness.get(egui_kittest::kittest::by().label_contains(&keys)).click();
}

/// Finds the Convert all button regardless of the file count in its label.
pub(crate) fn convert_all<'a>(harness: &'a Harness<App>) -> egui_kittest::Node<'a> {
    let counts = 0..=harness.state().items.len();
    let labels: Vec<String> = counts.map(|count| fl!("convert-all", count = count)).collect();
    let by = egui_kittest::kittest::by();
    harness.get(by.predicate(move |node| node.label().is_some_and(|label| labels.contains(&label))))
}

/// Returns the longest constant substring of `message` (formatted with [`ANY`]
/// for each argument), trimmed of surrounding whitespace and ASCII punctuation
/// where line wraps could occur.
pub(crate) fn fixed(message: &str) -> &str {
    let part = message.split(ANY).max_by_key(|part| part.len()).unwrap_or_default();
    part.trim_matches(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
}

/// Placeholder argument passed to `fl!` so [`fixed`] can split around it.
pub(crate) const ANY: &str = "\u{1}";

/// Steps the harness until all background loads, conversions, and file dialogs
/// have been idle for two consecutive frames (work triggered by a click on one
/// frame is only spawned at the start of the next frame).
pub(crate) fn finish(harness: &mut Harness<App>) {
    let mut settled = false;
    for _ in 0..6000 {
        harness.step();
        let state = harness.state();
        let reading = state.items.iter().any(|item| {
            item.sniffed.is_loading()
                || item.video.is_loading()
                || item.art.is_loading()
                || item.result_clip.is_loading()
                || item.earlier.is_some()
        });
        let idle = state.work.queue.is_empty() && state.picker.picking.is_none();
        if !reading && idle && state.items.iter().all(|item| item.job.is_none()) {
            if settled {
                harness.run_steps(2);
                return;
            }
            settled = true;
            continue;
        }
        settled = false;
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the GUI didn't settle");
}

/// Returns the completed conversion job for `item`, or panics with the failure
/// status.
pub(crate) fn made_job(item: &Item) -> &Job {
    match (&item.made, &item.failed) {
        (Some(made), _) => &made.job,
        (None, Some(failed)) => panic!("converting failed: {:?}", failed.job.status),
        (None, None) => panic!("nothing was converted"),
    }
}

pub(crate) fn video(dir: &Path, size: &str) -> Option<PathBuf> {
    video_of(dir, size, 1)
}

/// Generates a synthetic test video of `size` and `seconds`, or skips the test
/// if `ffmpeg` is not on `PATH`.
pub(crate) fn video_of(dir: &Path, size: &str, seconds: u32) -> Option<PathBuf> {
    let video = dir.join("clip.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=s={size}:d={seconds}"),
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&video)
        .status();
    // Skip if ffmpeg is not installed; fail if ffmpeg is present but errors.
    match made {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tgradish_corpus::skip("needs ffmpeg on PATH")
        }
        made => {
            assert!(made.expect("ffmpeg runs").success(), "ffmpeg could not make the video");
            Some(video)
        }
    }
}

/// Checks whether `ffmpeg` and `ffprobe` are on `PATH` (skipping or failing
/// according to `TGRADISH_REQUIRE_FFMPEG`).
pub(crate) fn has_ffmpeg() -> bool {
    tgradish_corpus::ffmpeg_on_path()
}

/// Writes the embedded corpus asset `name` to `dir/as_name`.
pub(crate) fn corpus(dir: &Path, name: &str, as_name: &str) -> PathBuf {
    let files = tgradish_corpus::files();
    let file = files.iter().find(|file| file.name == name).expect("in the corpus");
    let path = dir.join(as_name);
    std::fs::write(&path, &file.bytes).unwrap();
    path
}

/// Converts `walker_2x.gif` from the corpus into a lossless 0.8 s MKV video,
/// or skips the test if `ffmpeg` is not on `PATH`.
pub(crate) fn walking_video(dir: &Path) -> Option<PathBuf> {
    let gif = corpus(dir, "walker_2x.gif", "walker.gif");
    let video = dir.join("walker.mkv");
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(&gif)
        .args(["-c:v", "ffv1", "-pix_fmt", "bgra"])
        .arg(&video)
        .status();
    match made {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tgradish_corpus::skip("needs ffmpeg on PATH")
        }
        made => {
            assert!(made.expect("ffmpeg runs").success(), "ffmpeg could not make the video");
            Some(video)
        }
    }
}

/// Creates a `0o000` directory inside `dir` that cannot be listed, or returns
/// `None` if the current user can still read it (e.g. `root`).
#[cfg(unix)]
pub(crate) fn unlistable(dir: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let unlistable = dir.join("unlistable");
    std::fs::create_dir(&unlistable).unwrap();
    std::fs::set_permissions(&unlistable, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&unlistable).is_ok() {
        return None;
    }
    Some(unlistable)
}

/// Restores `0o755` permissions on a directory created by [`unlistable`] so
/// temporary directory cleanup can remove it.
#[cfg(unix)]
pub(crate) fn listable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
}

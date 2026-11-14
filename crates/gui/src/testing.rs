//! What the window's tests share: a window to drive, ways to wait for its
//! work and to convert, and inputs to give it. Videos are made with the
//! ffmpeg on PATH; without one, the tests that need them are skipped, or
//! fail where `TGRADISH_REQUIRE_FFMPEG` says it must be there.

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

/// A 4x4 PNG with a 2x2 square of `colour` in the middle.
pub(crate) fn square(path: &Path, colour: [u8; 4]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, 4, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let inside = |i: usize| (1..3).contains(&(i % 4)) && (1..3).contains(&(i / 4));
    let pixels: Vec<u8> = (0..16).flat_map(|i| if inside(i) { colour } else { [0; 4] }).collect();
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

/// A pasted image as the clipboard gives one, for `App::add_pasted`: the
/// file and its folder.
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

/// The window as eframe and egui-winit drive it on a screen, a frame a
/// moment apart: its size kept in pixels as its scale changes, each
/// frame's input given to the window's hook first, and the pointer said
/// as egui-winit says it, where it moves in points at the scale of the
/// moment, and then, for each press until it moves again, that same place
/// in points, whatever the scale has become since.
pub(crate) struct Screen {
    pub(crate) harness: Harness<'static, App>,
    /// The window's size in pixels.
    size: egui::Vec2,
    /// Where the pointer last moved, in points as egui-winit keeps it.
    said: Option<egui::Pos2>,
}

impl Screen {
    /// The window, `size` pixels large, from `config`, with Settings open
    /// where `settings` says, once its parts have found their sizes.
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

    /// A frame with `events`, as eframe runs one.
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

    /// The pointer moved to `pixels`, in a frame.
    pub(crate) fn move_to(&mut self, pixels: egui::Pos2) {
        let at = pixels / self.harness.ctx.zoom_factor();
        self.said = Some(at);
        self.frame(vec![egui::Event::PointerMoved(at)]);
    }

    /// The primary button pressed and let go where the pointer is, without
    /// it moving, a frame each.
    pub(crate) fn click(&mut self) {
        let pos = self.said.expect("the pointer is on the window");
        for pressed in [true, false] {
            let (button, modifiers) = (egui::PointerButton::Primary, egui::Modifiers::NONE);
            self.frame(vec![egui::Event::PointerButton { pos, button, pressed, modifiers }]);
        }
    }

    /// `key` pressed with the command key, in a frame.
    pub(crate) fn command(&mut self, key: egui::Key) {
        let modifiers = egui::Modifiers::COMMAND;
        let (repeat, physical_key) = (false, None);
        let pressed = egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers };
        self.frame(vec![pressed]);
    }

    /// Where `label`'s middle is on the screen, in pixels.
    pub(crate) fn pixels(&self, label: &str) -> egui::Pos2 {
        self.harness.get_by_label(label).rect().center() * self.harness.ctx.zoom_factor()
    }

    /// Where egui takes the pointer to be, in points.
    pub(crate) fn pointer(&self) -> Option<egui::Pos2> {
        self.harness.ctx.input(|input| input.pointer.hover_pos())
    }

    /// Whether egui takes the pointer to be on `label`.
    pub(crate) fn on(&self, label: &str) -> bool {
        let rect = self.harness.get_by_label(label).rect();
        self.pointer().is_some_and(|at| rect.contains(at))
    }
}

/// Everything the window's messages say, each part on a line of its own,
/// to look in.
pub(crate) fn said(app: &App) -> String {
    let rows = app.banner.rows.iter();
    rows.flat_map(|said| said.message.texts()).collect::<Vec<_>>().join("\n")
}

/// Clicks the selected item's Convert button, which shows its shortcut,
/// in whichever language the window is in.
pub(crate) fn convert(harness: &Harness<App>) {
    let keys = crate::keys::keys_text(crate::keys::Command::Convert);
    harness.get(egui_kittest::kittest::by().label_contains(&keys)).click();
}

/// Steps the window until no item is busy and no file dialog is open, two
/// frames in a row: the window starts what is due as a frame begins, so
/// what a frame's click changed, as a setting that has an input read
/// again, is only started by the next.
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
        let idle = state.queue.is_empty() && state.picking.is_none();
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
    panic!("the window didn't settle");
}

/// The conversion `item` made, or a panic saying why there is none.
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

/// A test video of `size` lasting `seconds`, or `None` without ffmpeg.
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
    // without ffmpeg the test is skipped; ffmpeg failing is a broken test
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

/// Whether ffmpeg and ffprobe are on PATH; tests that need them are
/// skipped without, or fail where they are required.
pub(crate) fn has_ffmpeg() -> bool {
    tgradish_corpus::ffmpeg_on_path()
}

/// The corpus file `name`, written to `dir` as `as_name`.
pub(crate) fn corpus(dir: &Path, name: &str, as_name: &str) -> PathBuf {
    let files = tgradish_corpus::files();
    let file = files.iter().find(|file| file.name == name).expect("in the corpus");
    let path = dir.join(as_name);
    std::fs::write(&path, &file.bytes).unwrap();
    path
}

/// The walker's 0.8 s walk as a lossless video, or `None` without ffmpeg.
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

/// A folder that can't be listed, as presets that can't be read are
/// in; `None` where it can, as for root.
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

/// The folder [`unlistable`] made listed again, so it can be removed.
#[cfg(unix)]
pub(crate) fn listable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
}

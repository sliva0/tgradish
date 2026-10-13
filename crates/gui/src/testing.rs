//! What the window's tests share: a window to drive, ways to wait for its
//! work and to convert, and inputs to give it. Videos are made with the
//! ffmpeg on PATH; without one, the tests that need them are skipped.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use tgradish_core::config::Config;
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

pub(crate) fn harness(config: Config) -> Harness<'static, App> {
    Harness::builder()
        .with_size([1280.0, 820.0])
        .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin(), None))
}

/// Clicks the selected item's Convert button, which shows its shortcut.
pub(crate) fn convert(harness: &Harness<App>) {
    harness.get(egui_kittest::kittest::by().label_contains("Ctrl+Enter")).click();
}

/// Steps the window until no item is busy.
pub(crate) fn finish(harness: &mut Harness<App>) {
    for _ in 0..6000 {
        harness.step();
        let state = harness.state();
        let reading = state.items.iter().any(|item| {
            item.video.is_loading() || item.art.is_loading() || item.result_clip.is_loading()
        });
        if !reading && state.queue.is_empty() && state.items.iter().all(|item| item.job.is_none()) {
            harness.run_steps(2);
            return;
        }
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
            eprintln!("skipped: needs ffmpeg on PATH");
            None
        }
        made => {
            assert!(made.expect("ffmpeg runs").success(), "ffmpeg could not make the video");
            Some(video)
        }
    }
}

/// Whether ffmpeg is on PATH; tests that need it are skipped without.
pub(crate) fn has_ffmpeg() -> bool {
    let found = std::process::Command::new("ffmpeg").arg("-version").output().is_ok();
    if !found {
        eprintln!("skipped: needs ffmpeg on PATH");
    }
    found
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
            eprintln!("skipped: needs ffmpeg on PATH");
            None
        }
        made => {
            assert!(made.expect("ffmpeg runs").success(), "ffmpeg could not make the video");
            Some(video)
        }
    }
}

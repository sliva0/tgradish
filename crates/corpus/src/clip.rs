//! Synthetic video clips generated with ffmpeg to resemble phone footage:
//! 25 fps H.264 with B-frames, built from `testsrc2` with added grain.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clip {
    /// 512x512, 4.84 s (121 frames) with a single keyframe at the start.
    Square,
    /// 258x320, 12.64 s (316 frames), longer than Telegram's 3-second sticker
    /// limit.
    Tall,
}

impl Clip {
    fn name(self) -> &'static str {
        match self {
            Clip::Square => "square",
            Clip::Tall => "tall",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Clip::Square => "testsrc2=s=512x512:r=25:d=4.84",
            Clip::Tall => "testsrc2=s=258x320:r=25:d=12.64",
        }
    }
}

/// Returns true when `TGRADISH_REQUIRE_FFMPEG` is set to anything other than
/// empty or `"0"`. CI sets this so tests fail instead of silently skipping when
/// ffmpeg is missing.
pub fn ffmpeg_required() -> bool {
    std::env::var_os("TGRADISH_REQUIRE_FFMPEG")
        .is_some_and(|value| !value.is_empty() && value != "0")
}

/// Skips a test with `why`, or panics if [`ffmpeg_required`] is true.
pub fn skip<T>(why: &str) -> Option<T> {
    assert!(!ffmpeg_required(), "{why}, which TGRADISH_REQUIRE_FFMPEG says must be there");
    eprintln!("skipped: {why}");
    None
}

/// Returns true if `ffmpeg` and `ffprobe` are available on `PATH`, otherwise
/// calls [`skip`] and returns false.
pub fn ffmpeg_on_path() -> bool {
    let runs = |program| Command::new(program).arg("-version").output().is_ok();
    if runs("ffmpeg") && runs("ffprobe") {
        return true;
    }
    skip::<()>("needs ffmpeg and ffprobe on PATH");
    false
}

/// Returns the path to `clip` inside `dir`, generating it if it does not
/// already exist. Returns `None` (or panics when [`ffmpeg_required`] is true)
/// if `ffmpeg` with `libx264` is not on `PATH`. Panics if generation itself
/// fails. Cached clips are returned without checking for ffmpeg, so tests that
/// also convert with ffmpeg should check [`ffmpeg_on_path`] first.
pub fn clip(clip: Clip, dir: &Path) -> Option<PathBuf> {
    // Bump the filename suffix if the clip generation parameters change.
    let path = dir.join(format!("tgradish-{}-1.mp4", clip.name()));
    if path.exists() {
        return Some(path);
    }
    if !has_x264() {
        return skip("needs ffmpeg on PATH with the x264 encoder");
    }
    std::fs::create_dir_all(dir).expect("the clips' directory can be made");
    // Write to a temporary file first so parallel tests never read a partial
    // clip.
    let thread = format!("{:?}", std::thread::current().id());
    let digits: String = thread.chars().filter(char::is_ascii_digit).collect();
    let partial = dir.join(format!("tgradish-{}-{}-{digits}.mp4", clip.name(), std::process::id()));
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", clip.source()])
        .args(["-vf", "noise=alls=6:allf=t"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-profile:v", "high", "-bf", "2"])
        .args(match clip {
            Clip::Square => ["-g", "1000", "-sc_threshold", "0"],
            Clip::Tall => ["-g", "250", "-sc_threshold", "40"],
        })
        .args(["-pix_fmt", "yuv420p", "-an"])
        .arg(&partial)
        .output()
        .expect("ffmpeg runs");
    if !made.status.success() {
        let _ = std::fs::remove_file(&partial);
        panic!(
            "ffmpeg could not make the {} clip: {}",
            clip.name(),
            String::from_utf8_lossy(&made.stderr)
        );
    }
    // On Windows, renaming over a file held open by another test fails; keep
    // the existing clip in that case.
    if let Err(err) = std::fs::rename(&partial, &path) {
        let _ = std::fs::remove_file(&partial);
        assert!(path.exists(), "the clip couldn't be moved into place: {err}");
    }
    Some(path)
}

/// Returns true if `ffmpeg` is on `PATH` and includes `libx264`. Panics if
/// `ffmpeg -encoders` fails.
fn has_x264() -> bool {
    let listed = match Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output() {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return false,
        listed => listed.expect("ffmpeg runs"),
    };
    assert!(
        listed.status.success(),
        "ffmpeg could not list its encoders: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    String::from_utf8_lossy(&listed.stdout).contains(" libx264 ")
}

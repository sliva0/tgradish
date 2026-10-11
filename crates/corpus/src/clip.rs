//! Video clips like phone footage, made with ffmpeg: H.264 at 25 fps with
//! B-frames, from its moving test pattern with grain added.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clip {
    /// 512 by 512, 4.84 s: 121 frames, a keyframe only at the start.
    Square,
    /// 258 by 320, 12.64 s: 316 frames, longer than a sticker plays.
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

/// `clip`, made in `dir` unless it is there from an earlier run; `None`
/// without an ffmpeg on PATH that has the x264 encoder.
pub fn clip(clip: Clip, dir: &Path) -> Option<PathBuf> {
    // the name changes with how the clip is made
    let path = dir.join(format!("tgradish-{}-1.mp4", clip.name()));
    if path.exists() {
        return Some(path);
    }
    std::fs::create_dir_all(dir).ok()?;
    // made aside, so tests running at once never see half a clip
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
        .status();
    if !made.is_ok_and(|status| status.success()) {
        let _ = std::fs::remove_file(&partial);
        eprintln!("skipped: needs ffmpeg on PATH with the x264 encoder");
        return None;
    }
    std::fs::rename(&partial, &path).ok()?;
    Some(path)
}

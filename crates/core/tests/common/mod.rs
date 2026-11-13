//! What the end-to-end tests share: the backends they run through, and
//! the clips they convert. Without ffmpeg they are skipped, unless
//! `TGRADISH_REQUIRE_FFMPEG=1` says it must be there, as CI does, where a
//! skipped test would pass unseen.
// each test binary uses its own part of this
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tgradish_core::backend::Backend;
use tgradish_core::ffmpeg::{self, FfmpegChoice};
use tgradish_corpus::Clip;

/// Whether tests that need ffmpeg fail without it rather than skip:
/// `TGRADISH_REQUIRE_FFMPEG` is set, to anything but empty or 0.
pub fn ffmpeg_required() -> bool {
    std::env::var_os("TGRADISH_REQUIRE_FFMPEG")
        .is_some_and(|value| !value.is_empty() && value != "0")
}

/// Skips the test, which `why` says what it needs, or fails it where
/// [`ffmpeg_required`].
pub fn skip<T>(why: &str) -> Option<T> {
    assert!(!ffmpeg_required(), "{why}, which TGRADISH_REQUIRE_FFMPEG says must be there");
    eprintln!("skipped: {why}");
    None
}

/// Backends to test, or `None` without a system ffmpeg.
pub fn backends() -> Option<Vec<Backend>> {
    let Ok(system) = ffmpeg::locate(FfmpegChoice::System, None) else {
        return skip("needs ffmpeg and ffprobe on PATH");
    };
    #[allow(unused_mut)]
    let mut backends = vec![Backend::Process(system)];
    #[cfg(feature = "linked")]
    backends.push(Backend::Linked);
    Some(backends)
}

/// A clip like phone footage, made once with ffmpeg and kept for later
/// runs, or `None` without an ffmpeg that has x264.
pub fn clip(kind: Clip) -> Option<PathBuf> {
    tgradish_corpus::clip(kind, Path::new(env!("CARGO_TARGET_TMPDIR")))
        .or_else(|| skip("needs ffmpeg on PATH with the x264 encoder"))
}

/// Name for assertion messages.
pub fn name(backend: &Backend) -> &'static str {
    match backend {
        Backend::Process(_) => "process",
        #[allow(unreachable_patterns)]
        _ => "linked",
    }
}

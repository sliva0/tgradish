//! What the end-to-end tests share: the backends they run through, and
//! the clips they convert. Without ffmpeg they are skipped, unless
//! `TGRADISH_REQUIRE_FFMPEG=1` says it must be there, as CI does, where a
//! skipped test would pass unseen.
// each test binary uses its own part of this
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tgradish_core::backend::Backend;
use tgradish_core::ffmpeg::Ffmpeg;
use tgradish_corpus::Clip;

// the rule every crate's tests share, kept with the clips
pub use tgradish_corpus::skip;

/// Backends to test, or `None` without a system ffmpeg.
pub fn backends() -> Option<Vec<Backend>> {
    let Ok(system) = Ffmpeg::on_path() else {
        return skip("needs ffmpeg and ffprobe on PATH");
    };
    #[allow(unused_mut)]
    let mut backends = vec![Backend::Process(system)];
    #[cfg(feature = "linked")]
    backends.push(Backend::Builtin);
    Some(backends)
}

/// A clip like phone footage, made once with ffmpeg and kept for later
/// runs, or `None` without an ffmpeg that has x264.
pub fn clip(kind: Clip) -> Option<PathBuf> {
    tgradish_corpus::clip(kind, Path::new(env!("CARGO_TARGET_TMPDIR")))
}

/// Name for assertion messages.
pub fn name(backend: &Backend) -> &'static str {
    match backend {
        Backend::Process(_) => "process",
        #[allow(unreachable_patterns)]
        _ => "linked",
    }
}

//! `tgradish watch`: converts videos and images as they appear in a
//! directory. Looks at the directory every `--interval` seconds and
//! converts a file once its size and modification time stop changing, so
//! files that are still being copied are left alone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Result, bail};

use crate::Context;
use crate::args::WatchArgs;
use crate::convert::Converter;

/// Extensions of files worth converting.
const EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "flv", "ts", "mts", "ogv", "gif", "apng", "png",
    "jpg", "jpeg", "webp", "bmp", "tif", "tiff",
];

/// Whether `path` looks like an input rather than a result, a hidden file
/// or a download in progress.
fn is_input(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    let name = name.to_lowercase();
    if name.starts_with('.') || name.starts_with('~') {
        return false;
    }
    if [".sticker.webm", ".emoji.webm", ".spoofed.webm"].iter().any(|s| name.ends_with(s)) {
        return false;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

/// What a file looked like at one scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

fn scan(dir: &Path, recursive: bool, found: &mut HashMap<PathBuf, Stamp>) {
    // unreadable entries are skipped: they may be in the middle of a move
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if recursive && !hidden {
                scan(&path, recursive, found);
            }
        } else if metadata.is_file() && is_input(&path) {
            found.insert(path, Stamp { size: metadata.len(), modified: metadata.modified().ok() });
        }
    }
}

/// Files whose stamp did not change since the previous scan and that were
/// not handled at that stamp yet.
#[derive(Default)]
struct Tracker {
    /// Stamp at the previous scan, for files not handled yet.
    pending: HashMap<PathBuf, Stamp>,
    /// Stamp at which a file was converted or ignored.
    handled: HashMap<PathBuf, Stamp>,
}

impl Tracker {
    fn ignore(&mut self, path: PathBuf, stamp: Stamp) {
        self.handled.insert(path, stamp);
    }

    /// Returns files that are ready, oldest scan order first.
    fn update(&mut self, current: HashMap<PathBuf, Stamp>) -> Vec<(PathBuf, Stamp)> {
        self.handled.retain(|path, _| current.contains_key(path));
        let mut ready = Vec::new();
        let mut pending = HashMap::new();
        for (path, stamp) in current {
            if self.handled.get(&path) == Some(&stamp) {
                continue;
            }
            if self.pending.get(&path) == Some(&stamp) {
                ready.push((path, stamp));
            } else {
                pending.insert(path, stamp);
            }
        }
        self.pending = pending;
        ready.sort_by(|a, b| a.0.cmp(&b.0));
        ready
    }
}

/// Sleeps for `seconds`, waking up early when cancelled.
fn sleep(ctx: &Context, seconds: f64) {
    let until = std::time::Instant::now() + Duration::from_secs_f64(seconds);
    while !ctx.cancel.is_cancelled() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn run(ctx: &Context, args: WatchArgs) -> Result<()> {
    if !args.dir.is_dir() {
        bail!("{} is not a directory", args.dir.display());
    }
    if !(args.interval > 0.0 && args.interval.is_finite()) {
        bail!("--interval must be more than 0");
    }
    let converter = Converter::new(ctx, &args.conversion)?;

    let mut tracker = Tracker::default();
    let mut found = HashMap::new();
    scan(&args.dir, args.recursive, &mut found);
    for (path, stamp) in found {
        let has_result = converter.output_for(&path).exists();
        if args.existing && !has_result {
            tracker.pending.insert(path, stamp);
        } else {
            tracker.ignore(path, stamp);
        }
    }

    if !ctx.global.json && !ctx.global.quiet {
        eprintln!("watching {}, press Ctrl-C to stop", args.dir.display());
    }
    loop {
        sleep(ctx, args.interval);
        if ctx.cancel.is_cancelled() {
            if !ctx.global.json {
                eprintln!("stopped watching");
            }
            return Ok(());
        }
        let mut found = HashMap::new();
        scan(&args.dir, args.recursive, &mut found);
        for (path, stamp) in tracker.update(found) {
            match converter.run(&path, None) {
                Ok(_) => {}
                Err(tgradish_core::Error::Cancelled) => {
                    return Err(tgradish_core::Error::Cancelled.into());
                }
                Err(err) => converter.report_failure(&path, err),
            }
            // also after failures: retried only when the file changes
            tracker.ignore(path, stamp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_inputs() {
        assert!(is_input(Path::new("dir/Pig.MP4")));
        assert!(is_input(Path::new("cat.gif")));
        assert!(!is_input(Path::new("pig.sticker.webm")));
        assert!(!is_input(Path::new("pig.emoji.webm")));
        assert!(!is_input(Path::new(".pig.mp4")));
        assert!(!is_input(Path::new("pig.mp4.part")));
        assert!(!is_input(Path::new("notes.txt")));
    }

    #[test]
    fn waits_for_files_to_settle() {
        let stamp = |size| Stamp { size, modified: None };
        let path = PathBuf::from("a.mp4");
        let mut tracker = Tracker::default();
        let scan = |size| HashMap::from([(path.clone(), stamp(size))]);

        assert!(tracker.update(scan(10)).is_empty(), "new file");
        assert!(tracker.update(scan(20)).is_empty(), "still growing");
        assert_eq!(tracker.update(scan(20)), [(path.clone(), stamp(20))]);
        tracker.ignore(path.clone(), stamp(20));
        assert!(tracker.update(scan(20)).is_empty(), "already converted");
        assert!(tracker.update(scan(30)).is_empty(), "changed again");
        assert_eq!(tracker.update(scan(30)).len(), 1);
    }
}

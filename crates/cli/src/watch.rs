//! `tgradish watch`: converts videos and images as they appear in a
//! directory. Looks at the directory every `--interval` seconds and
//! converts a file once its size and modification time stop changing, so
//! files that are still being copied are left alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context as _, Result, bail};

use crate::Context;
use crate::args::WatchArgs;
use crate::convert::Converter;
use tgradish_core::presets::Format;

use crate::ui;

/// Extensions of files worth converting.
const EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "flv", "ts", "mts", "ogv", "gif", "apng", "png",
    "jpg", "jpeg", "webp", "bmp", "tif", "tiff",
];

/// Aseprite files, which only `.tgs` conversion reads.
const ASEPRITE: &[&str] = &["ase", "aseprite"];

/// Failed conversions are retried this many times, in case the file was
/// locked or not readable yet.
const MAX_FAILURES: u32 = 5;

/// Whether `path` looks like an input for `format` rather than a result, a
/// hidden file or a download in progress.
fn is_input(path: &Path, format: Format) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    let name = name.to_lowercase();
    if name.starts_with('.') || name.starts_with('~') {
        return false;
    }
    if [".sticker.webm", ".emoji.webm", ".spoofed.webm"].iter().any(|s| name.ends_with(s)) {
        return false;
    }
    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else { return false };
    let extension = extension.to_lowercase();
    EXTENSIONS.contains(&extension.as_str())
        || format == Format::Tgs && ASEPRITE.contains(&extension.as_str())
}

/// What a file looked like at one scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

fn current_stamp(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_file().then(|| Stamp { size: metadata.len(), modified: metadata.modified().ok() })
}

/// One look at the directory: input files, and directories that could not
/// be read.
#[derive(Default)]
struct Scan {
    files: HashMap<PathBuf, Stamp>,
    unreadable: Vec<(PathBuf, std::io::Error)>,
    format: Format,
}

impl Scan {
    fn run(dir: &Path, recursive: bool, format: Format) -> Scan {
        let mut scan = Scan { format, ..Scan::default() };
        scan.visit(dir, recursive);
        scan
    }

    fn visit(&mut self, dir: &Path, recursive: bool) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.unreadable.push((dir.to_path_buf(), err));
                return;
            }
        };
        // entries can disappear while listing, they are simply skipped
        for entry in entries.flatten() {
            let path = entry.path();
            // does not follow symlinks, so links cannot make loops
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
                if recursive && !hidden {
                    self.visit(&path, recursive);
                }
            } else if file_type.is_file()
                && is_input(&path, self.format)
                && let Some(stamp) = current_stamp(&path)
            {
                self.files.insert(path, stamp);
            }
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

    /// Returns files that are ready, sorted by path.
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

/// Failed attempts at one file, while its stamp stays the same.
struct Failures {
    stamp: Stamp,
    count: u32,
    retry_at: Instant,
}

/// Whether trying again could help: the file may have been locked, not
/// readable yet, or still being written.
fn worth_retrying(err: &tgradish_core::Error) -> bool {
    use tgradish_core::Error;
    matches!(err, Error::Io(_) | Error::Probe { .. } | Error::Ffmpeg { .. } | Error::Libav(_))
}

/// Sleeps for `seconds`, waking up early when cancelled.
fn sleep(ctx: &Context, seconds: f64) {
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while !ctx.cancel.is_cancelled() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn warn(ctx: &Context, message: String) {
    if ctx.global.json {
        crate::print_json_error(&anyhow::anyhow!(message), None);
    } else {
        eprintln!("{} {message}", ui::warning_label());
    }
}

pub fn run(ctx: &Context, args: WatchArgs) -> Result<()> {
    if !args.dir.is_dir() {
        bail!("{} is not a directory", args.dir.display());
    }
    if !(args.interval > 0.0 && args.interval.is_finite()) {
        bail!("--interval must be more than 0");
    }
    std::fs::read_dir(&args.dir).with_context(|| format!("cannot read {}", args.dir.display()))?;
    let converter = Converter::new(ctx, &args.conversion, None)?;
    let output_for = |path: &Path| converter.output_under(path, Some(&args.dir));

    let mut tracker = Tracker::default();
    for (path, stamp) in Scan::run(&args.dir, args.recursive, converter.format()).files {
        if args.existing && !output_for(&path).exists() {
            tracker.pending.insert(path, stamp);
        } else {
            tracker.ignore(path, stamp);
        }
    }
    // inputs whose result this command wrote, so it may replace it when the
    // input changes again
    let mut ours: HashSet<PathBuf> = HashSet::new();
    let mut failures: HashMap<PathBuf, Failures> = HashMap::new();
    let mut unreadable: HashSet<PathBuf> = HashSet::new();

    if !ctx.global.json && !ctx.global.quiet {
        let dir = crate::term::link(args.dir.display(), &args.dir);
        eprintln!("{}", crate::ui::status("Watching", format!("{dir}, Ctrl-C stops")));
    }
    loop {
        sleep(ctx, args.interval);
        if ctx.cancel.is_cancelled() {
            if !ctx.global.json {
                eprintln!("{}", crate::ui::status("Stopped", "watching"));
            }
            return Ok(());
        }

        let scan = Scan::run(&args.dir, args.recursive, converter.format());
        let now_unreadable: HashSet<PathBuf> =
            scan.unreadable.iter().map(|(p, _)| p.clone()).collect();
        for (path, err) in &scan.unreadable {
            if !unreadable.contains(path) {
                warn(ctx, format!("cannot read {}: {err}", path.display()));
            }
        }
        unreadable = now_unreadable;

        for (path, stamp) in tracker.update(scan.files) {
            if let Some(failed) = failures.get(&path) {
                if failed.stamp != stamp {
                    failures.remove(&path);
                } else if Instant::now() < failed.retry_at {
                    // comes back as ready after the next scan
                    continue;
                }
            }
            // earlier conversions took time, the file may have changed since
            if current_stamp(&path) != Some(stamp) {
                continue;
            }

            let overwrite = converter.overwrite() || ours.contains(&path);
            let result = converter.run(&path, Some(output_for(&path)), overwrite);
            let changed_meanwhile = current_stamp(&path) != Some(stamp);
            match result {
                Ok(_) => {
                    ours.insert(path.clone());
                    failures.remove(&path);
                }
                Err(tgradish_core::Error::Cancelled) => {
                    return Err(tgradish_core::Error::Cancelled.into());
                }
                Err(err) if worth_retrying(&err) && !changed_meanwhile => {
                    let failed = failures.entry(path.clone()).or_insert(Failures {
                        stamp,
                        count: 0,
                        retry_at: Instant::now(),
                    });
                    failed.count += 1;
                    converter.report_failure(&path, err);
                    if failed.count < MAX_FAILURES {
                        let delay = (args.interval * 2f64.powi(failed.count as i32)).min(60.0);
                        failed.retry_at = Instant::now() + Duration::from_secs_f64(delay);
                        // not handled: it is retried while it stays the same
                        continue;
                    }
                    warn(ctx, format!("giving up on {} until it changes", path.display()));
                }
                Err(err) => converter.report_failure(&path, err),
            }
            // a file that changed while converting is converted again once
            // it settles
            if !changed_meanwhile {
                tracker.ignore(path, stamp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_inputs() {
        let webm = |path: &str| is_input(Path::new(path), Format::Webm);
        assert!(webm("dir/Pig.MP4"));
        assert!(webm("cat.gif"));
        assert!(!webm("pig.sticker.webm"));
        assert!(!webm("pig.emoji.webm"));
        assert!(!webm(".pig.mp4"));
        assert!(!webm("pig.mp4.part"));
        assert!(!webm("notes.txt"));
        assert!(!webm("walk.ase"));
        assert!(is_input(Path::new("walk.ase"), Format::Tgs));
        assert!(!is_input(Path::new("walk.sticker.tgs"), Format::Tgs));
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

    #[cfg(unix)]
    #[test]
    fn reports_unreadable_directories() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let scan = Scan::run(dir.path(), true, Format::Webm);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        // root can read anything, then there is nothing to report
        if scan.unreadable.is_empty() {
            return;
        }
        assert_eq!(scan.unreadable[0].0, locked);
    }
}

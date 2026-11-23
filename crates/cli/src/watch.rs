//! `tgradish watch`: polls a directory every `--interval` seconds and converts
//! new or changed media files once their size and modification time settle, so
//! files still being copied are left alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Result, bail};

use crate::Context;
use crate::args::WatchArgs;
use crate::convert::{Converter, Failed};
use tgradish_core::input::{self, Input};

use crate::ui;

/// Failed attempts at a file before giving up on it until it changes (it may
/// have been locked or not readable yet).
const MAX_FAILURES: u32 = 5;

/// Returns whether `path` is a supported input file (not a converted output,
/// hidden file, or in-progress download).
fn is_input(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    let name = name.to_lowercase();
    if name.starts_with('.') || name.starts_with('~') {
        return false;
    }
    if input::is_result_name(path) || name.ends_with(crate::commands::SPOOFED) {
        return false;
    }
    let extension = input::extension(path);
    let extension = extension.as_str();
    input::VIDEOS.contains(&extension)
        || input::IMAGES.contains(&extension)
        || input::ASEPRITE.contains(&extension)
}

/// File size and modification time observed during a directory scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

fn current_stamp(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_file().then(|| Stamp { size: metadata.len(), modified: metadata.modified().ok() })
}

/// Results of a single directory scan: candidate input files and unreadable
/// directories.
#[derive(Default)]
struct Scan {
    files: HashMap<PathBuf, Stamp>,
    unreadable: Vec<(PathBuf, std::io::Error)>,
}

impl Scan {
    fn run(dir: &Path, recursive: bool) -> Scan {
        let mut scan = Scan::default();
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
        // Skip entries that disappear mid-scan.
        for entry in entries.flatten() {
            let path = entry.path();
            // Do not follow directory symlinks (avoids cycles); file symlinks
            // are treated as regular files.
            let Ok(file_type) = entry.file_type() else { continue };
            let file = file_type.is_file()
                || file_type.is_symlink() && std::fs::metadata(&path).is_ok_and(|m| m.is_file());
            if file_type.is_dir() {
                let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
                if recursive && !hidden {
                    self.visit(&path, recursive);
                }
            } else if file
                && is_input(&path)
                && let Some(stamp) = current_stamp(&path)
            {
                self.files.insert(path, stamp);
            }
        }
    }
}

/// Tracks file stamps across scans to identify files that have settled and have
/// not yet been processed at that stamp.
#[derive(Default)]
struct Tracker {
    /// Stamps from the previous scan for unhandled files.
    pending: HashMap<PathBuf, Stamp>,
    /// Stamps at which files were already converted or ignored.
    handled: HashMap<PathBuf, Stamp>,
}

impl Tracker {
    fn ignore(&mut self, path: PathBuf, stamp: Stamp) {
        self.handled.insert(path, stamp);
    }

    /// Marks a previously handled file as pending again so it becomes ready on
    /// the next scan if unchanged.
    fn redo(&mut self, path: &Path) {
        if let Some(stamp) = self.handled.remove(path) {
            self.pending.insert(path.to_path_buf(), stamp);
        }
    }

    /// Returns settled files ready for conversion, sorted by path.
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

/// Retry state for a failing file at a single [`Stamp`].
struct Failures {
    stamp: Stamp,
    count: u32,
    retry_at: Instant,
}

/// Returns whether a failed conversion is worth retrying (for example when the
/// file was temporarily locked, unreadable, or still being written).
fn worth_retrying(err: &Failed) -> bool {
    use tgradish_core::Error;
    match err {
        Failed::Core(err) => {
            matches!(
                err,
                Error::Io { .. }
                    | Error::Unreadable { .. }
                    | Error::Ffmpeg { .. }
                    | Error::Libav(_)
            )
        }
        Failed::Folder { .. } => true,
        Failed::SameOutput { .. } => false,
    }
}

/// Returns whether `path` (seen on the previous scan, missing now) is truly
/// gone (deleted, renamed, or replaced by a directory). A file whose metadata
/// cannot be read (for example inside an unreadable directory) is not treated
/// as gone yet.
fn gone(path: &Path) -> bool {
    use std::io::ErrorKind;
    match std::fs::metadata(path) {
        Ok(metadata) => !metadata.is_file(),
        Err(err) => matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory),
    }
}

/// Prints the stop message and returns `Ok(())`; cancellation is the normal way
/// to end `watch`.
fn stopped(ctx: &Context) -> Result<()> {
    if !ctx.global.json {
        eprintln!("{}", crate::ui::status("Stopped", "watching"));
    }
    Ok(())
}

/// Sleeps for `seconds`, waking early if cancelled.
fn sleep(ctx: &Context, seconds: f64) {
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while !ctx.cancel.is_cancelled() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Emits a watcher-level warning without stopping the loop.
fn warn(ctx: &Context, message: String) {
    if ctx.global.json {
        crate::json_line(&tgradish_core::Event::Warning { message });
    } else {
        eprintln!("{} {message}", ui::warning_label());
    }
}

/// Watches the directory until cancelled by Ctrl-C or `--cancel-on-stdin`,
/// which exits with success whether waiting or converting.
pub fn run(ctx: &Context, args: WatchArgs) -> Result<()> {
    let dir = args.dir.display();
    if !args.dir.exists() {
        bail!("{dir}: directory does not exist");
    }
    if !args.dir.is_dir() {
        bail!("{dir} is not a directory");
    }
    std::fs::read_dir(&args.dir).map_err(|err| {
        anyhow::anyhow!("failed to read {dir}: {}", tgradish_core::io_reason(&err))
    })?;
    let converter = Converter::new(ctx, &args.conversion, args.format, None)?;
    let output_for =
        |path: &Path| converter.output_for(&Input::File(path.to_path_buf()), Some(&args.dir));

    let mut tracker = Tracker::default();
    let first = Scan::run(&args.dir, args.recursive).files;
    // Inputs seen on the previous scan, used to detect deletions.
    let mut seen: HashSet<PathBuf> = first.keys().cloned().collect();
    for (path, stamp) in first {
        if args.existing && !output_for(&path).exists() {
            tracker.pending.insert(path, stamp);
        } else {
            tracker.ignore(path, stamp);
        }
    }
    // Inputs whose output we created, so we may overwrite it if the input
    // changes.
    let mut ours: HashSet<PathBuf> = HashSet::new();
    let mut failures: HashMap<PathBuf, Failures> = HashMap::new();
    // Inputs skipped due to an output-path collision, mapped to the conflicting
    // output path so they can retry once that output is freed.
    let mut refused: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut unreadable: HashSet<PathBuf> = HashSet::new();

    if !ctx.global.json && !ctx.global.quiet {
        let dir = crate::term::link(args.dir.display(), &args.dir);
        eprintln!("{}", crate::ui::status("Watching", format!("{dir} (press Ctrl-C to stop)")));
    }
    loop {
        sleep(ctx, args.interval);
        if ctx.cancel.is_cancelled() {
            return stopped(ctx);
        }

        let scan = Scan::run(&args.dir, args.recursive);
        let now_unreadable: HashSet<PathBuf> =
            scan.unreadable.iter().map(|(p, _)| p.clone()).collect();
        for (path, err) in &scan.unreadable {
            if !unreadable.contains(path) {
                let err = tgradish_core::io_reason(err);
                warn(ctx, format!("failed to read {}: {err}", path.display()));
            }
        }
        unreadable = now_unreadable;
        // If an input disappears (deleted or renamed), release its output path
        // so another input with the same stem can use it, or the original input
        // can replace it if it returns. Keep files we could not stat in `seen`.
        let mut freed = Vec::new();
        let mut unsure = Vec::new();
        for path in seen.iter().filter(|path| !scan.files.contains_key(*path)) {
            if gone(path) {
                freed.extend(converter.forget(path));
            } else {
                unsure.push(path.clone());
            }
        }
        seen = scan.files.keys().cloned().chain(unsure).collect();
        refused.retain(|path, output| {
            let free = freed.contains(output);
            if free {
                tracker.redo(path);
            }
            !free && scan.files.contains_key(path)
        });

        for (path, stamp) in tracker.update(scan.files) {
            if let Some(failed) = failures.get(&path) {
                if failed.stamp != stamp {
                    failures.remove(&path);
                } else if Instant::now() < failed.retry_at {
                    // Leave in pending so it returns as ready on the next scan.
                    continue;
                }
            }
            // Earlier conversions in this batch may have taken time; skip if
            // the file changed since the scan.
            if current_stamp(&path) != Some(stamp) {
                continue;
            }

            let overwrite = converter.overwrite() || ours.contains(&path);
            let input = Input::File(path.clone());
            refused.remove(&path);
            let result = converter.run(&input, Some(output_for(&path)), overwrite);
            let changed_meanwhile = current_stamp(&path) != Some(stamp);
            match result {
                Ok(_) => {
                    ours.insert(path.clone());
                    failures.remove(&path);
                }
                // Cancelled mid-conversion; temporary files are already
                // cleaned up.
                Err(Failed::Core(tgradish_core::Error::Cancelled)) => return stopped(ctx),
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
                        // Do not mark handled so it is retried if unchanged.
                        continue;
                    }
                    warn(ctx, format!("giving up on {} until it changes", path.display()));
                }
                Err(err) => {
                    if let Failed::SameOutput { output, .. } = &err {
                        let output = std::path::absolute(output).unwrap_or_else(|_| output.clone());
                        refused.insert(path.clone(), output);
                    }
                    converter.report_failure(&path, err);
                }
            }
            // If the file changed during conversion, leave it pending so it is
            // reconverted once it settles.
            if !changed_meanwhile {
                tracker.ignore(path, stamp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File symlinks are watched as regular files, whereas directory symlinks
    /// are not followed.
    #[cfg(unix)]
    #[test]
    fn follows_links_to_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let clip = elsewhere.path().join("clip.mp4");
        std::fs::write(&clip, b"a video").unwrap();
        std::fs::write(elsewhere.path().join("inside.mp4"), b"another").unwrap();
        std::os::unix::fs::symlink(&clip, dir.path().join("linked.mp4")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("folder")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("broken.mp4")).unwrap();
        let scan = Scan::run(dir.path(), true);
        let found: Vec<_> = scan.files.keys().collect();
        assert_eq!(found, [&dir.path().join("linked.mp4")]);
    }

    #[test]
    fn recognises_inputs() {
        let input = |path: &str| is_input(Path::new(path));
        assert!(input("dir/Pig.MP4"));
        assert!(input("cat.gif"));
        assert!(input("walk.ase"));
        assert!(!input("pig.sticker.webm"));
        assert!(!input("pig.emoji.webm"));
        assert!(!input(".pig.mp4"));
        assert!(!input("pig.mp4.part"));
        assert!(!input("notes.txt"));
        assert!(!input("walk.sticker.tgs"));
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
        let scan = Scan::run(dir.path(), true);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Skip when running as root, which can read 0o000 directories.
        if scan.unreadable.is_empty() {
            return;
        }
        assert_eq!(scan.unreadable[0].0, locked);
    }
}

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
use crate::convert::{Converter, Failed};
use tgradish_core::input::{self, Input};

use crate::ui;

/// Failed conversions are retried this many times, in case the file was
/// locked or not readable yet.
const MAX_FAILURES: u32 = 5;

/// Whether `path` looks like an input rather than a result, a hidden file
/// or a download in progress.
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
        // entries can disappear while listing, they are simply skipped
        for entry in entries.flatten() {
            let path = entry.path();
            // links to directories aren't followed, so they can't make
            // loops; a link to a file is taken as the file
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

/// Says watching stopped: stopping is the only way out of it, so it ends
/// the command as done.
fn stopped(ctx: &Context) -> Result<()> {
    if !ctx.global.json {
        eprintln!("{}", crate::ui::status("Stopped", "watching"));
    }
    Ok(())
}

/// Sleeps for `seconds`, waking up early when cancelled.
fn sleep(ctx: &Context, seconds: f64) {
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while !ctx.cancel.is_cancelled() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Says something about watching, rather than one conversion, which
/// goes on: as a warning, which events of every format can be.
fn warn(ctx: &Context, message: String) {
    if ctx.global.json {
        crate::json_line(&tgradish_core::webm::Event::Warning { message });
    } else {
        eprintln!("{} {message}", ui::warning_label());
    }
}

/// Watches until stopped, by Ctrl-C or `--cancel-on-stdin`, which ends it
/// as done, whether it was waiting or converting then.
pub fn run(ctx: &Context, args: WatchArgs) -> Result<()> {
    if !args.dir.is_dir() {
        bail!("{} is not a directory", args.dir.display());
    }
    std::fs::read_dir(&args.dir)
        .with_context(|| format!("couldn't read {}", args.dir.display()))?;
    let converter = Converter::new(ctx, &args.conversion, args.format, None)?;
    let output_for =
        |path: &Path| converter.output_for(&Input::File(path.to_path_buf()), Some(&args.dir));

    let mut tracker = Tracker::default();
    for (path, stamp) in Scan::run(&args.dir, args.recursive).files {
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
            return stopped(ctx);
        }

        let scan = Scan::run(&args.dir, args.recursive);
        let now_unreadable: HashSet<PathBuf> =
            scan.unreadable.iter().map(|(p, _)| p.clone()).collect();
        for (path, err) in &scan.unreadable {
            if !unreadable.contains(path) {
                warn(ctx, format!("couldn't read {}: {err}", path.display()));
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
            let input = Input::File(path.clone());
            let result = converter.run(&input, Some(output_for(&path)), overwrite);
            let changed_meanwhile = current_stamp(&path) != Some(stamp);
            match result {
                Ok(_) => {
                    ours.insert(path.clone());
                    failures.remove(&path);
                }
                // stopped while converting, which leaves nothing behind
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

    /// A link to a file is watched as the file, and a link to a
    /// directory isn't followed.
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
        // root can read anything, then there is nothing to report
        if scan.unreadable.is_empty() {
            return;
        }
        assert_eq!(scan.unreadable[0].0, locked);
    }
}

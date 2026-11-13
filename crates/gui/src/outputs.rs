//! Where results go, and which files they may replace. A result never
//! goes over a file someone else put there, so the window remembers what
//! it wrote itself, and looks for tgradish's marks on files in the way.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use tgradish_core::paths;

use crate::app::App;
use crate::item::Item;

/// A file's size and modification time, which change when it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileVersion {
    bytes: u64,
    /// Since 1970, as files are dated.
    modified: Option<Duration>,
}

impl FileVersion {
    /// The file at `path` as it is now, if there is one.
    pub(crate) fn of(path: &Path) -> Option<FileVersion> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok().and_then(|at| at.duration_since(UNIX_EPOCH).ok());
        Some(FileVersion { bytes: metadata.len(), modified })
    }
}

/// A result this window wrote, now or in an earlier run: for which item,
/// and the file as written, so a file put there since isn't taken for it.
pub(crate) struct Written {
    pub(crate) item: u64,
    pub(crate) version: FileVersion,
}

impl Written {
    pub(crate) fn of(item: u64, path: &Path) -> Option<Written> {
        Some(Written { item, version: FileVersion::of(path)? })
    }
}

impl App {
    /// Where an item's result goes, as worked out this frame.
    pub(crate) fn output_for(&self, item: &Item) -> PathBuf {
        if let Some(output) = self.outputs.borrow().get(&item.id) {
            return output.clone();
        }
        let output = self.work_out_output(item);
        self.outputs.borrow_mut().insert(item.id, output.clone());
        output
    }

    /// Where an item's result goes: next to its input or in the folder
    /// for results, numbered if another file in the window has the name.
    pub(crate) fn work_out_output(&self, item: &Item) -> PathBuf {
        let dir = self.config.gui.output_dir.clone();
        let next_to_input = item.input().default_output(item.format, item.choices.target);
        let name = next_to_input.file_name().unwrap_or_default();
        let first = match (&item.paste_dir, dir) {
            // not next to the image, which is in a temporary directory
            (Some(_), dir) => dir.or_else(paths::pictures_dir).unwrap_or_default().join(name),
            (None, Some(dir)) => dir.join(name),
            (None, None) => next_to_input.clone(),
        };
        let claimed: HashSet<&Path> = self
            .items
            .iter()
            .filter(|other| other.id != item.id)
            .flat_map(|other| {
                let waiting = other.job.as_ref().map(|conversion| conversion.job.output.as_path());
                let made = other.last_result().map(|made| made.job.output.as_path());
                waiting.into_iter().chain(made)
            })
            .collect();
        let suffix = format!(".{}.{}", item.choices.target.name(), item.format.extension());
        let own = item.last_result().map(|made| made.job.output.as_path());
        // pasted images all have one name, so tgradish having made a file
        // doesn't make it theirs to replace: only the item's own result is
        let blocked = |path: &Path| {
            let may_replace = match item.paste_dir {
                Some(_) => self.wrote(item.id, path),
                None => self.may_replace(item.id, path),
            };
            path.exists() && !may_replace
        };
        free_name(&first, &suffix, &claimed, own, &blocked)
    }

    /// Whether the item may replace the file at `path`: one it wrote and
    /// nobody changed since, one tgradish made, or any with the setting
    /// that allows it.
    pub(crate) fn may_replace(&self, item: u64, path: &Path) -> bool {
        self.config.gui.overwrite || self.wrote(item, path) || self.made_by_tgradish(path)
    }

    /// Whether the item wrote the file at `path`, and nobody changed it
    /// since.
    pub(crate) fn wrote(&self, item: u64, path: &Path) -> bool {
        self.written.get(path).is_some_and(|written| {
            written.item == item && FileVersion::of(path) == Some(written.version)
        })
    }

    /// Whether tgradish made the file at `path`, by the marks it shows:
    /// read once for each version of the file.
    pub(crate) fn made_by_tgradish(&self, path: &Path) -> bool {
        let Some(version) = FileVersion::of(path) else { return false };
        let mut known = self.made_by_tgradish.borrow_mut();
        match known.get(path) {
            Some(&(seen, made)) if seen == version => made,
            _ => {
                let made = tgradish_core::mark::made_by_tgradish(path);
                known.insert(path.to_path_buf(), (version, made));
                made
            }
        }
    }
}

/// `first`, or the first of `first` numbered before `suffix` (`clip
/// 2.sticker.webm`) that no other item claims and no `blocked` file is
/// at: the item's `own` earlier result if it is one of them.
fn free_name(
    first: &Path,
    suffix: &str,
    claimed: &HashSet<&Path>,
    own: Option<&Path>,
    blocked: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let name = first.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let stem = name.strip_suffix(suffix).unwrap_or(&name).to_owned();
    let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
    let names = || {
        std::iter::once(first.to_path_buf())
            .chain((2..).map(|n| dir.join(format!("{stem} {n}{suffix}"))))
            .filter(|path| !claimed.contains(path.as_path()))
    };
    if let Some(own) = own
        && !blocked(own)
        && names().take(1000).any(|path| path == own)
    {
        return own.to_path_buf();
    }
    names().find(|path| !blocked(path)).expect("some name is free")
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::jobs::Status;
    use crate::testing::{convert, finish, harness, made_job, pasted_image, square};

    #[test]
    fn numbers_results_of_files_with_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        square(&a.join("same.png"), [255, 0, 0, 255]);
        square(&b.join("same.png"), [0, 0, 255, 255]);
        let out = dir.path().join("out");
        let mut config = Config::default();
        config.gui.output_dir = Some(out.clone());
        let mut harness = harness(config);
        harness.state_mut().add(vec![a.join("same.png"), b.join("same.png")]);
        finish(&mut harness);
        harness.get_by_label_contains("Convert all").click();
        harness.run_steps(2);
        finish(&mut harness);
        for item in &harness.state().items {
            assert!(matches!(made_job(item).status, Status::Done(_)));
        }
        assert!(out.join("same.sticker.tgs").exists() && out.join("same 2.sticker.tgs").exists());
    }

    #[test]
    fn keeps_files_put_where_a_result_was() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let output = dir.path().join("art.sticker.tgs");
        assert!(output.exists());
        // someone else's file where the result was
        std::fs::remove_file(&output).unwrap();
        std::fs::write(&output, b"mine").unwrap();
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        // the result goes next to it, under a number
        let item = &harness.state().items[0];
        let made = made_job(item);
        assert_eq!(made.output, dir.path().join("art 2.sticker.tgs"));
        assert!(made.output.exists() && !made.replaces);
        assert_eq!(std::fs::read(&output).unwrap(), b"mine");
    }

    #[test]
    fn replaces_results_tgradish_made_earlier() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let output = dir.path().join("art.sticker.tgs");
        // made by an earlier window, which this one knows nothing of
        for earlier in [true, false] {
            let mut harness = harness(Config::default());
            harness.state_mut().add(vec![art.clone()]);
            finish(&mut harness);
            // said before converting
            let said = harness.query_by_label_contains("Replaces an earlier result").is_some();
            assert_eq!(said, !earlier);
            convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            let made = made_job(&harness.state().items[0]);
            assert_eq!(made.output, output);
            assert_eq!(made.replaces, !earlier);
            // and no longer once this window has made it
            assert!(harness.query_by_label_contains("Replaces an earlier result").is_none());
        }
    }

    #[test]
    fn replaces_a_pasted_images_own_result_only() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let mut config = Config::default();
        config.gui.output_dir = Some(out.clone());
        let mut harness = harness(config);
        let results = |harness: &Harness<App>| -> Vec<PathBuf> {
            harness.state().items.iter().map(|item| made_job(item).output.clone()).collect()
        };
        let (first, second) =
            (out.join("clipboard.sticker.tgs"), out.join("clipboard 2.sticker.tgs"));
        // made again with another setting, the result goes where it went
        for keep_canvas in [false, true] {
            if harness.state().items.is_empty() {
                let (files, folder) = pasted_image();
                harness.state_mut().add_pasted(files, folder);
            }
            finish(&mut harness);
            harness.state_mut().items[0].choices.tgs.keep_canvas = Some(keep_canvas);
            harness.run_steps(2);
            convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            assert_eq!(results(&harness), std::slice::from_ref(&first));
            assert!(!second.exists());
        }
        // another pasted image's doesn't go over it
        let (files, folder) = pasted_image();
        harness.state_mut().add_pasted(files, folder);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert_eq!(results(&harness), [first.clone(), second]);
        // nor does the first's go over a file put where its result was
        std::fs::write(&first, b"mine").unwrap();
        let id = harness.state().items[0].id;
        harness.state_mut().select(id);
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert_eq!(results(&harness)[0], out.join("clipboard 3.sticker.tgs"));
        assert_eq!(std::fs::read(&first).unwrap(), b"mine");
    }

    #[test]
    fn names_results_apart() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("clipboard.sticker.tgs");
        let second = dir.path().join("clipboard 2.sticker.tgs");
        let none = HashSet::new();
        let suffix = ".sticker.tgs";
        let existing = |path: &Path| path.exists();
        let free = |_: &Path| false;
        assert_eq!(free_name(&first, suffix, &none, None, &existing), first);
        // a file in the way that may not be replaced
        std::fs::write(&first, b"").unwrap();
        assert_eq!(free_name(&first, suffix, &none, None, &existing), second);
        assert_eq!(free_name(&first, suffix, &none, None, &free), first);
        // another file's name is taken; an item's own stays its own
        let claimed = HashSet::from([first.as_path()]);
        assert_eq!(free_name(&first, suffix, &claimed, None, &free), second);
        assert_eq!(free_name(&first, suffix, &none, Some(&second), &free), second);
        assert_eq!(free_name(&first, suffix, &claimed, Some(&first), &free), second);
    }
}

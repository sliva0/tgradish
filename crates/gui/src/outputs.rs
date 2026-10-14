//! Where results go, and which files they may replace. A result never
//! goes over a file someone else put there, so the window remembers what
//! it wrote itself, and looks for tgradish's marks on files in the way.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tgradish_core::presets::Format;
use tgradish_core::{convert, paths, still, tgs};

use crate::app::App;
use crate::item::Item;

/// A result this window wrote: for which item, and the file as written,
/// so a file put there since isn't taken for it.
pub(crate) struct Written {
    item: u64,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl Written {
    pub(crate) fn of(item: u64, path: &Path) -> Option<Written> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Written { item, len: metadata.len(), modified: metadata.modified().ok() })
    }
}

/// A file's size and modification time, which change when it is written.
pub(crate) type FileVersion = (u64, Option<std::time::SystemTime>);

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
        let dir = self.config.gui.output_dir.clone().filter(|dir| !dir.as_os_str().is_empty());
        let input = &item.inputs[0];
        let next_to_input = match item.format {
            Format::Webm => convert::default_output(input, item.choices.target),
            Format::Tgs => tgs::default_output(input, item.choices.target),
            Format::Image => still::default_output(input, item.choices.target),
        };
        let name = next_to_input.file_name().unwrap_or_default();
        let first = match (&item.pasted, dir) {
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
                let waiting = other.job.as_ref().map(|(job, ..)| job.output.as_path());
                waiting.into_iter().chain(other.made.as_ref().map(|made| made.job.output.as_path()))
            })
            .collect();
        let suffix = format!(".{}.{}", item.choices.target.name(), item.format.extension());
        let own = item.made.as_ref().map(|made| made.job.output.as_path());
        // pasted images all have one name: theirs never replace a file
        let blocked = |path: &Path| {
            path.exists() && (item.pasted.is_some() || !self.may_replace(item.id, path))
        };
        free_name(&first, &suffix, &claimed, own, &blocked)
    }

    /// Whether the item may replace the file at `path`: one it wrote and
    /// nobody changed since, one tgradish made, or any with the setting
    /// that allows it.
    pub(crate) fn may_replace(&self, item: u64, path: &Path) -> bool {
        self.config.gui.overwrite || self.wrote(item, path) || self.made_by_tgradish(path)
    }

    pub(crate) fn wrote(&self, item: u64, path: &Path) -> bool {
        self.written.get(path).is_some_and(|written| {
            Written::of(item, path).is_some_and(|now| {
                now.item == written.item
                    && now.len == written.len
                    && now.modified == written.modified
            })
        })
    }

    /// Whether tgradish made the file at `path`, by the marks it shows:
    /// read once for each version of the file.
    fn made_by_tgradish(&self, path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else { return false };
        let version = (metadata.len(), metadata.modified().ok());
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
    use egui_kittest::kittest::Queryable;
    use tgradish_core::config::Config;

    use super::*;
    use crate::jobs::Status;
    use crate::testing::{convert, finish, harness, made_job, square};

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
            convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            let made = made_job(&harness.state().items[0]);
            assert_eq!(made.output, output);
            assert_eq!(made.replaces, !earlier);
        }
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

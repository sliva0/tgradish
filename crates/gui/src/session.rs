//! The files open in the window and their settings, kept across restarts:
//! saved as the window runs and when it closes, read when it opens. Kept
//! in the state directory (`~/.local/state/tgradish` on Linux), with
//! copies of pasted images, whose own folders go when the window closes.

use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tgradish_core::presets::Format;

use crate::item::{Choices, Item, Made};
use crate::jobs::{Done, Job, Status};

/// What the file says it is, so a session of another layout is left alone.
const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Session {
    version: u32,
    items: Vec<Saved>,
    /// Which of them is selected.
    selected: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Saved {
    inputs: Vec<PathBuf>,
    sequence: bool,
    /// A pasted image: its copy's name in the session's folder.
    pasted: Option<String>,
    format: Format,
    format_guessed: bool,
    choices: Choices,
    /// The last result, and what it was made with.
    result: Option<SavedResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct SavedResult {
    output: PathBuf,
    format: Format,
    choices: Choices,
}

/// Where pasted images are kept, next to the session file.
fn pasted_dir(path: &Path) -> PathBuf {
    path.with_file_name("pasted")
}

/// Whether `name` is a file's name alone, so a session file can't point
/// copies anywhere else.
fn plain_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!((parts.next(), parts.next()), (Some(Component::Normal(_)), None))
}

/// Copies the pasted image of item `id` next to the session file at
/// `path`, so the window can open it again, and gives the copy's name,
/// which no other has had.
pub fn keep_pasted(path: &Path, id: u64, image: &Path) -> std::io::Result<String> {
    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let extension = image.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let name = format!("{time:x}-{id}.{extension}");
    let dir = pasted_dir(path);
    std::fs::create_dir_all(&dir)?;
    std::fs::copy(image, dir.join(&name))?;
    Ok(name)
}

/// Whether the session can hold `item`: JSON can't name files whose paths
/// aren't Unicode, and a pasted image is only there again if it was
/// copied.
fn keeps(item: &Item) -> bool {
    item.inputs.iter().all(|input| input.to_str().is_some())
        && (item.pasted.is_none() || item.kept.is_some())
}

impl Session {
    /// What `items` are, `selected` the one selected, without those the
    /// session can't hold.
    pub fn of(items: &[Item], selected: Option<u64>) -> Session {
        let kept: Vec<&Item> = items.iter().filter(|item| keeps(item)).collect();
        let saved = kept
            .iter()
            .map(|item| {
                let made = item.made.as_ref().filter(|made| made.job.output.to_str().is_some());
                Saved {
                    // a pasted image is opened from its copy
                    inputs: if item.kept.is_some() { Vec::new() } else { item.inputs.clone() },
                    sequence: item.sequence,
                    pasted: item.kept.clone(),
                    format: item.format,
                    format_guessed: item.format_guessed,
                    choices: item.choices.clone(),
                    result: made.map(|made| SavedResult {
                        output: made.job.output.clone(),
                        format: made.format,
                        choices: made.choices.clone(),
                    }),
                }
            })
            .collect();
        let selected = selected.and_then(|id| kept.iter().position(|item| item.id == id));
        Session { version: VERSION, items: saved, selected }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Writes the session to `path`, whole or not at all, and drops copies
    /// of pasted images it no longer has.
    pub fn save(&self, json: &str, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, json)?;
        std::fs::rename(&partial, path)?;
        let kept: Vec<&str> = self.items.iter().filter_map(|item| item.pasted.as_deref()).collect();
        if let Ok(entries) = std::fs::read_dir(pasted_dir(path)) {
            for entry in entries.flatten() {
                if !kept.iter().any(|name| entry.file_name() == **name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    /// The session saved at `path`, if there is one of this layout.
    pub fn load(path: &Path) -> Option<Session> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<Session>(&text).ok().filter(|session| session.version == VERSION)
    }

    /// The items again, numbered from `next_id`, and which is selected;
    /// also how many were left out because their files are gone.
    pub fn restore(self, path: &Path, mut next_id: u64) -> (Vec<Item>, Option<u64>, usize) {
        let mut items = Vec::new();
        let mut selected = None;
        let mut gone = 0;
        for (index, saved) in self.items.into_iter().enumerate() {
            let Some(item) = restore_item(saved, path, next_id) else {
                gone += 1;
                continue;
            };
            if self.selected == Some(index) {
                selected = Some(item.id);
            }
            next_id += 1;
            items.push(item);
        }
        (items, selected, gone)
    }
}

fn restore_item(saved: Saved, path: &Path, id: u64) -> Option<Item> {
    // a pasted image goes back into a folder of its own
    let (inputs, pasted) = match &saved.pasted {
        Some(name) if !plain_name(name) => return None,
        Some(name) => {
            let dir = tempfile::Builder::new().prefix("tgradish-pasted-").tempdir().ok()?;
            let image = dir.path().join(name);
            std::fs::copy(pasted_dir(path).join(name), &image).ok()?;
            (vec![image], Some(dir))
        }
        None => (saved.inputs, None),
    };
    if inputs.is_empty() || !inputs.iter().all(|input| input.exists()) {
        return None;
    }
    let mut item = Item::new(id, inputs, saved.sequence, saved.choices.target);
    item.pasted = pasted;
    item.kept = saved.pasted;
    item.format = saved.format;
    item.format_guessed = saved.format_guessed;
    item.choices = saved.choices;
    item.made = saved.result.and_then(|result| {
        let done = Done::of_file(&result.output, result.format, result.choices.target)?;
        let mut job = Job::waiting(result.output);
        job.status = Status::Done(Box::new(done));
        Some(Made { job, format: result.format, choices: result.choices })
    });
    Some(item)
}

#[cfg(test)]
mod tests {
    use tgradish_core::telegram::Target;

    use super::*;

    fn pasted(id: u64, bytes: &[u8]) -> Item {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("pasted.png");
        std::fs::write(&image, bytes).unwrap();
        let mut item = Item::new(id, vec![image], false, Target::Sticker);
        item.pasted = Some(dir);
        item
    }

    fn contents(items: &[Item]) -> Vec<Vec<u8>> {
        items.iter().map(|item| std::fs::read(&item.inputs[0]).unwrap()).collect()
    }

    /// Saves `items` at `path`, pasted images copied there as the window
    /// copies them, and opens them again, numbered from 1.
    fn again(items: &mut [Item], path: &Path) -> Vec<Item> {
        for item in items.iter_mut().filter(|item| item.kept.is_none()) {
            item.kept = Some(keep_pasted(path, item.id, &item.inputs[0]).unwrap());
        }
        let session = Session::of(items, None);
        session.save(&session.to_json().unwrap(), path).unwrap();
        Session::load(path).unwrap().restore(path, 1).0
    }

    #[test]
    fn keeps_pasted_images_apart_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut items = vec![pasted(1, b"one"), pasted(2, b"two"), pasted(3, b"three")];
        let mut items = again(&mut items, &path);
        // the first goes; the others are numbered anew each time
        items.remove(0);
        let mut items = again(&mut items, &path);
        let items = again(&mut items, &path);
        assert_eq!(contents(&items), [b"two".to_vec(), b"three".to_vec()]);
        // and only their copies are kept
        assert_eq!(std::fs::read_dir(pasted_dir(&path)).unwrap().count(), 2);
    }

    #[test]
    fn copies_pasted_images_only_within_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let outside = dir.path().join("precious.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let mut one = pasted(1, b"one");
        one.kept = Some(outside.to_str().unwrap().to_owned());
        let session = Session::of(&[one], None);
        let (items, _, gone) = session.restore(&path, 1);
        assert!(items.is_empty() && gone == 1);
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        for name in ["../a.png", "a/b.png", "..", ""] {
            assert!(!plain_name(name), "{name}");
        }
        assert!(plain_name("19a0-3.png"));
    }

    #[test]
    fn names_everything_as_the_options_within_are_named() {
        let mut item = Item::new(1, vec![PathBuf::from("/a.png")], false, Target::Sticker);
        item.choices.show_layers = Some(vec!["sketch".into()]);
        item.choices.webm.exact_scale = "2".parse().ok();
        let json = Session::of(&[item], None).to_json().unwrap();
        for name in ["format-guessed", "show-layers", "exact-scale"] {
            assert!(json.contains(&format!("\"{name}\":")), "{name} in {json}");
        }
        assert!(!json.contains('_'), "{json}");
    }

    #[cfg(unix)]
    #[test]
    fn leaves_out_what_it_cannot_hold() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join(std::ffi::OsStr::from_bytes(b"odd\xff.png"));
        let fine = dir.path().join("fine.png");
        // nor a pasted image that has no copy to open it from
        let items = [
            Item::new(1, vec![odd], false, Target::Sticker),
            Item::new(2, vec![fine], false, Target::Sticker),
            pasted(3, b"three"),
        ];
        let session = Session::of(&items, Some(2));
        assert!(session.to_json().is_ok());
        assert_eq!((session.items.len(), session.selected), (1, Some(0)));
    }
}

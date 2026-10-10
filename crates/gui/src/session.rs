//! The files open in the window and their settings, kept across restarts:
//! saved as the window runs and when it closes, read when it opens. Kept
//! in the state directory (`~/.local/state/tgradish` on Linux), with
//! copies of pasted images, whose own folders go when the window closes.

use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tgradish_core::options::{Crop, Options};
use tgradish_core::presets::Format;
use tgradish_core::telegram::Target;
use tgradish_core::tgs::TgsOptions;

use crate::item::{Choices, Item, Made};
use crate::jobs::{Done, Job, Status};

/// What the file says it is, so a session of another layout is left alone.
const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Session {
    version: u32,
    items: Vec<Saved>,
    /// Which of them is selected.
    selected: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SavedChoices {
    preset: Option<String>,
    target: Target,
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
    webm: Options,
    tgs: TgsOptions,
}

impl SavedChoices {
    fn of(choices: &Choices) -> SavedChoices {
        SavedChoices {
            preset: choices.preset.clone(),
            target: choices.target,
            crop: choices.crop,
            start: choices.start,
            length: choices.length,
            webm: choices.webm.clone(),
            tgs: choices.tgs.clone(),
        }
    }

    fn choices(self) -> Choices {
        Choices {
            preset: self.preset,
            target: self.target,
            crop: self.crop,
            start: self.start,
            length: self.length,
            webm: self.webm,
            tgs: self.tgs,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Saved {
    inputs: Vec<PathBuf>,
    sequence: bool,
    /// A pasted image: its copy's name in the session's folder.
    pasted: Option<String>,
    format: Format,
    format_guessed: bool,
    choices: SavedChoices,
    /// The last result, and what it was made with.
    result: Option<SavedResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedResult {
    output: PathBuf,
    format: Format,
    choices: SavedChoices,
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

/// A name for the copy of a pasted image, which no other has had.
fn new_name(item: &Item, image: &Path) -> String {
    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let extension = image.extension().and_then(|e| e.to_str()).unwrap_or("png");
    format!("{time:x}-{}.{extension}", item.id)
}

impl Session {
    /// What `items` are, `selected` the one selected. Copies pasted images
    /// next to the session file at `path`, once. Files whose paths aren't
    /// Unicode are left out, as JSON can't hold them.
    pub fn of(items: &mut [Item], selected: Option<u64>, path: &Path) -> Session {
        let pasted = pasted_dir(path);
        let text = |path: &Path| path.to_str().is_some();
        let saved = items
            .iter_mut()
            .filter(|item| item.inputs.iter().all(|input| text(input)))
            .map(|item| {
                if item.pasted.is_some()
                    && item.kept.is_none()
                    && let Some(image) = item.inputs.first()
                {
                    let name = new_name(item, image);
                    let copied = std::fs::create_dir_all(&pasted)
                        .and_then(|()| std::fs::copy(image, pasted.join(&name)));
                    item.kept = copied.is_ok().then_some(name);
                }
                let copy = item.pasted.as_ref().and(item.kept.clone());
                let result = item.made.as_ref().filter(|made| text(&made.job.output)).map(|made| {
                    SavedResult {
                        output: made.job.output.clone(),
                        format: made.format,
                        choices: SavedChoices::of(&made.choices),
                    }
                });
                Saved {
                    inputs: if copy.is_some() { Vec::new() } else { item.inputs.clone() },
                    sequence: item.sequence,
                    pasted: copy,
                    format: item.format,
                    format_guessed: item.format_guessed,
                    choices: SavedChoices::of(&item.choices),
                    result,
                }
            })
            .collect();
        let kept: Vec<u64> = items
            .iter()
            .filter(|item| item.inputs.iter().all(|input| text(input)))
            .map(|item| item.id)
            .collect();
        let selected = selected.and_then(|id| kept.iter().position(|&kept| kept == id));
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
    item.choices = saved.choices.choices();
    item.made = saved.result.and_then(|result| {
        let done = Done::of_file(&result.output, result.format, result.choices.target)?;
        let mut job = Job::waiting(result.output);
        job.status = Status::Done(Box::new(done));
        Some(Made { job, format: result.format, choices: result.choices.choices() })
    });
    Some(item)
}

#[cfg(test)]
mod tests {
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

    /// Saves `items` at `path` and opens them again, numbered from 1.
    fn again(items: &mut [Item], path: &Path) -> Vec<Item> {
        let session = Session::of(items, None, path);
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
        let mut session = Session::of(&mut [pasted(1, b"one")], None, &path);
        session.items[0].pasted = Some(outside.to_str().unwrap().to_owned());
        let (items, _, gone) = session.restore(&path, 1);
        assert!(items.is_empty() && gone == 1);
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        for name in ["../a.png", "a/b.png", "..", ""] {
            assert!(!plain_name(name), "{name}");
        }
        assert!(plain_name("19a0-3.png"));
    }

    #[cfg(unix)]
    #[test]
    fn leaves_out_files_json_cannot_name() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join(std::ffi::OsStr::from_bytes(b"odd\xff.png"));
        let fine = dir.path().join("fine.png");
        let mut items = [
            Item::new(1, vec![odd], false, Target::Sticker),
            Item::new(2, vec![fine], false, Target::Sticker),
        ];
        let session = Session::of(&mut items, Some(2), &dir.path().join("session.json"));
        assert!(session.to_json().is_ok());
        assert_eq!((session.items.len(), session.selected), (1, Some(0)));
    }
}

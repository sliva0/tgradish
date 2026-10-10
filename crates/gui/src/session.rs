//! The files open in the window and their settings, kept across restarts:
//! saved as the window runs and when it closes, read when it opens. Kept
//! in the state directory (`~/.local/state/tgradish` on Linux), with
//! copies of pasted images, whose own folders go when the window closes.

use std::path::{Path, PathBuf};

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

impl Session {
    /// What `items` are, `selected` the one selected. Copies pasted images
    /// next to the session file at `path`, once.
    pub fn of(items: &[Item], selected: Option<u64>, path: &Path) -> Session {
        let pasted = pasted_dir(path);
        let saved = items
            .iter()
            .map(|item| {
                let copy = item.pasted.as_ref().and_then(|_| {
                    let image = item.inputs.first()?;
                    let name = format!(
                        "{}.{}",
                        item.id,
                        image.extension().and_then(|e| e.to_str()).unwrap_or("png")
                    );
                    let kept = pasted.join(&name);
                    if !kept.exists() {
                        std::fs::create_dir_all(&pasted).ok()?;
                        std::fs::copy(image, &kept).ok()?;
                    }
                    Some(name)
                });
                let result = item.made.as_ref().map(|made| SavedResult {
                    output: made.job.output.clone(),
                    format: made.format,
                    choices: SavedChoices::of(&made.choices),
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
        let selected = selected.and_then(|id| items.iter().position(|item| item.id == id));
        Session { version: VERSION, items: saved, selected }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("sessions serialize")
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

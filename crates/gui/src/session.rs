//! The files open in the window and their settings, kept across restarts:
//! saved as the window runs and when it closes, read when it opens. Kept
//! in the state directory (`~/.local/state/tgradish` on Linux), with
//! copies of pasted images, whose own folders go when the window closes.
//!
//! A session this version can't read, as one a newer tgradish kept, is
//! set aside rather than written over, and the copies it may name are left
//! alone. Only one window at a time keeps a session, the first to open.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tgradish_core::clipboard::PASTED_IMAGE_NAME;
use tgradish_core::presets::Format;

use crate::item::{Choices, Conversion, Item};
use crate::jobs::{Done, Job, Status};
use crate::outputs::{FileVersion, Written};

/// What the file says it is, so a session of another layout is left alone.
const VERSION: u32 = 1;

/// A session file as it is read: its items one by one, so that one this
/// version can't read, as with an option it doesn't know, is left out alone.
#[derive(Deserialize)]
struct Read {
    version: u32,
    items: Vec<serde_json::Value>,
    selected: Option<usize>,
}

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
    /// The file as the window wrote it, by which it knows its own result
    /// again; `None` for one it found.
    written: Option<FileVersion>,
    format: Format,
    choices: Choices,
}

/// A session's files, open again.
pub struct Restored {
    pub items: Vec<Item>,
    pub selected: Option<u64>,
    /// How many were left out because their files are gone.
    pub gone: usize,
    /// The results among them that an earlier run wrote, for the items
    /// that have them now.
    pub written: Vec<(PathBuf, Written)>,
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

/// What a session file held.
#[derive(Default)]
pub struct Loaded {
    pub session: Session,
    /// Set when some or all of it couldn't be read.
    pub unreadable: Option<Unreadable>,
}

/// A session file this version of tgradish couldn't read all of.
pub struct Unreadable {
    /// How many of its files were left out; `None` when none of it could
    /// be read.
    pub files: Option<usize>,
    /// Where the file is kept as it was.
    pub kept: PathBuf,
}

/// A window's session file: where it is, what the window last wrote there,
/// and which copies of pasted images are the window's to remove.
pub struct Store {
    path: PathBuf,
    /// The session as last written or read, so it is only written again
    /// when it changes.
    written: String,
    /// The copies of pasted images this window read of or made. Others in
    /// the folder may belong to a session that was set aside.
    copies: HashSet<String>,
    /// Held while the window runs, so no other keeps a session here.
    _lock: Option<std::fs::File>,
}

impl Store {
    /// The session at `path`, for this window alone: `None` while another
    /// window has it, as two would write over each other's files and
    /// remove each other's pasted images.
    pub fn open(path: PathBuf) -> Option<Store> {
        let lock = lock(&path).ok()?;
        Some(Store { path, written: String::new(), copies: HashSet::new(), _lock: lock })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where a session that couldn't be read is kept.
    fn aside(&self) -> PathBuf {
        self.path.with_extension("json.unreadable")
    }

    /// Reads the session, if there is one. A file that can't be read is
    /// moved aside, and one that can only be read in part is copied there
    /// first, so that nothing is lost when the session is next written;
    /// an error where that can't be done.
    pub fn read(&mut self) -> std::io::Result<Loaded> {
        let text = match std::fs::read(&self.path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded::default()),
            Err(err) => return Err(err),
        };
        let Some((session, unread)) = parse(&text) else {
            std::fs::rename(&self.path, self.aside())?;
            let unreadable = Unreadable { files: None, kept: self.aside() };
            return Ok(Loaded { session: Session::default(), unreadable: Some(unreadable) });
        };
        let unreadable = if unread > 0 {
            std::fs::copy(&self.path, self.aside())?;
            Some(Unreadable { files: Some(unread), kept: self.aside() })
        } else {
            None
        };
        self.written = session.to_json().unwrap_or_default();
        self.copies = session.items.iter().filter_map(|item| item.pasted.clone()).collect();
        Ok(Loaded { session, unreadable })
    }

    /// Copies the pasted image of item `id` next to the session file, so
    /// the window can open it again, and gives the copy's name, which no
    /// other has had.
    pub fn keep_pasted(&mut self, id: u64, image: &Path) -> std::io::Result<String> {
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let extension = image.extension().and_then(|e| e.to_str()).unwrap_or("png");
        let name = format!("{time:x}-{id}.{extension}");
        let dir = pasted_dir(&self.path);
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(image, dir.join(&name))?;
        self.copies.insert(name.clone());
        Ok(name)
    }

    /// Writes `session` if it changed, whole or not at all, and removes
    /// the copies of pasted images it no longer has.
    pub fn save(&mut self, session: &Session) -> std::io::Result<()> {
        let json = session.to_json().map_err(std::io::Error::other)?;
        if json == self.written {
            return Ok(());
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let partial = self.path.with_extension("json.partial");
        std::fs::write(&partial, &json)?;
        std::fs::rename(&partial, &self.path)?;
        self.written = json;
        let named: HashSet<String> =
            session.items.iter().filter_map(|item| item.pasted.clone()).collect();
        // copies nobody names are left over, unless a session set aside
        // may name them: then only this window's own go
        let all = !self.aside().exists();
        if let Ok(entries) = std::fs::read_dir(pasted_dir(&self.path)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !named.contains(&name) && (all || self.copies.contains(&name)) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        self.copies = named;
        Ok(())
    }
}

/// Another window holds the session.
struct Held;

/// Takes the lock beside the session at `path`, which lasts until the file
/// is closed, as when the window ends in any way. No file where there is
/// no telling whether another window is open, as the folder can't be
/// written to or its file system has no locks: the session is kept as if
/// there were none, and where it can't be, writing it says so.
fn lock(path: &Path) -> Result<Option<std::fs::File>, Held> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let options = std::fs::File::options().create(true).truncate(false).write(true).clone();
    let Ok(file) = options.open(path.with_extension("lock")) else { return Ok(None) };
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Err(Held),
        Err(std::fs::TryLockError::Error(_)) => Ok(None),
    }
}

/// The session `text` holds and how many of its items couldn't be read, or
/// `None` where it isn't a session of this layout at all.
fn parse(text: &str) -> Option<(Session, usize)> {
    let read: Read =
        serde_json::from_str(text).ok().filter(|read: &Read| read.version == VERSION)?;
    let mut session = Session { version: VERSION, items: Vec::new(), selected: None };
    let mut unread = 0;
    for (index, item) in read.items.into_iter().enumerate() {
        match serde_json::from_value(item) {
            Ok(item) => {
                if read.selected == Some(index) {
                    session.selected = Some(session.items.len());
                }
                session.items.push(item);
            }
            Err(_) => unread += 1,
        }
    }
    Some((session, unread))
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
    /// session can't hold. `written` are the results the window wrote.
    pub fn of(
        items: &[Item],
        selected: Option<u64>,
        written: &HashMap<PathBuf, Written>,
    ) -> Session {
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
                        written: written
                            .get(&made.job.output)
                            .filter(|written| written.item == item.id)
                            .map(|written| written.version),
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

    /// The items again, numbered from `next_id`.
    pub fn restore(self, path: &Path, mut next_id: u64) -> Restored {
        let mut restored =
            Restored { items: Vec::new(), selected: None, gone: 0, written: Vec::new() };
        for (index, saved) in self.items.into_iter().enumerate() {
            let result =
                saved.result.as_ref().map(|result| (result.output.clone(), result.written));
            let Some(item) = restore_item(saved, path, next_id) else {
                restored.gone += 1;
                continue;
            };
            if self.selected == Some(index) {
                restored.selected = Some(item.id);
            }
            if let Some((output, Some(version))) = result {
                restored.written.push((output, Written { item: item.id, version }));
            }
            next_id += 1;
            restored.items.push(item);
        }
        restored
    }
}

fn restore_item(saved: Saved, path: &Path, id: u64) -> Option<Item> {
    let (inputs, pasted) = match &saved.pasted {
        Some(name) if !plain_name(name) => return None,
        // a pasted image goes back into a folder of its own, under the
        // name it was pasted with, which its result is named after
        Some(name) => {
            let dir = tempfile::Builder::new().prefix("tgradish-pasted-").tempdir().ok()?;
            let image = dir.path().join(PASTED_IMAGE_NAME);
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
        Some(Conversion::of(job, result.format, result.choices))
    });
    Some(item)
}

#[cfg(test)]
mod tests {
    use tgradish_core::telegram::Target;

    use super::*;

    fn pasted(id: u64, bytes: &[u8]) -> Item {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join(PASTED_IMAGE_NAME);
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
        let mut store = Store::open(path.to_path_buf()).unwrap();
        store.read().unwrap();
        for item in items.iter_mut().filter(|item| item.kept.is_none()) {
            item.kept = Some(store.keep_pasted(item.id, &item.inputs[0]).unwrap());
        }
        store.save(&session(items, None)).unwrap();
        drop(store);
        Store::open(path.to_path_buf()).unwrap().read().unwrap().session.restore(path, 1).items
    }

    /// The session of `items`, none of whose results the window wrote.
    fn session(items: &[Item], selected: Option<u64>) -> Session {
        Session::of(items, selected, &HashMap::new())
    }

    fn copies(path: &Path) -> usize {
        std::fs::read_dir(pasted_dir(path)).map_or(0, Iterator::count)
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
        // named as when they were pasted, as their results are named after them
        for item in &items {
            assert_eq!(item.inputs[0].file_name().unwrap(), PASTED_IMAGE_NAME);
        }
        // and only their copies are kept
        assert_eq!(copies(&path), 2);
    }

    #[test]
    fn lets_one_window_at_a_time_keep_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("session.json");
        let first = Store::open(path.clone()).expect("nobody has it");
        assert!(Store::open(path.clone()).is_none());
        // until the first window is gone
        drop(first);
        assert!(Store::open(path).is_some());
    }

    #[test]
    fn sets_aside_a_session_it_cannot_read() {
        // of another version, and not a session at all
        for text in [r#"{"version":2,"items":[],"selected":null}"#, "not a session"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("session.json");
            std::fs::write(&path, text).unwrap();
            // a copy it may name
            std::fs::create_dir(pasted_dir(&path)).unwrap();
            std::fs::write(pasted_dir(&path).join("19a0-3.png"), b"pasted").unwrap();
            let mut store = Store::open(path.clone()).unwrap();
            let loaded = store.read().unwrap();
            let unreadable = loaded.unreadable.expect("unreadable");
            assert!(loaded.session.items.is_empty() && unreadable.files.is_none());
            assert_eq!(std::fs::read_to_string(&unreadable.kept).unwrap(), text);
            // the window goes on with a session of its own, whose copies
            // come and go, and leaves the other's alone
            let mut items = [pasted(1, b"one")];
            items[0].kept = Some(store.keep_pasted(1, &items[0].inputs[0]).unwrap());
            store.save(&session(&items, None)).unwrap();
            assert_eq!(copies(&path), 2);
            store.save(&session(&[], None)).unwrap();
            assert_eq!(copies(&path), 1);
            assert_eq!(std::fs::read(pasted_dir(&path).join("19a0-3.png")).unwrap(), b"pasted");
            assert_eq!(std::fs::read_to_string(&unreadable.kept).unwrap(), text);
        }
    }

    #[test]
    fn leaves_out_only_the_files_it_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut store = Store::open(path.clone()).unwrap();
        let mut items = [pasted(1, b"one"), pasted(2, b"two")];
        for item in &mut items {
            item.kept = Some(store.keep_pasted(item.id, &item.inputs[0]).unwrap());
            item.choices.webm.crf = Some(20);
        }
        store.save(&session(&items, Some(2))).unwrap();
        // as a newer tgradish would keep the first, with an option of its own
        let text = std::fs::read_to_string(&path).unwrap();
        let newer = text.replacen(r#""webm":{"#, r#""webm":{"made-up":1,"#, 1);
        assert_ne!(newer, text);
        std::fs::write(&path, &newer).unwrap();
        drop(store);

        let mut store = Store::open(path.clone()).unwrap();
        let loaded = store.read().unwrap();
        let unreadable = loaded.unreadable.expect("unreadable");
        assert_eq!(unreadable.files, Some(1));
        assert_eq!(std::fs::read_to_string(&unreadable.kept).unwrap(), newer);
        // the other is there, and still the one selected
        let restored = loaded.session.restore(&path, 7);
        assert_eq!(contents(&restored.items), [b"two".to_vec()]);
        assert_eq!((restored.selected, restored.gone), (Some(7), 0));
        // and writing the session leaves the copy of the one left out
        store.save(&session(&restored.items, restored.selected)).unwrap();
        assert_eq!(copies(&path), 2);
    }

    #[test]
    fn copies_pasted_images_only_within_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let outside = dir.path().join("precious.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let mut one = pasted(1, b"one");
        one.kept = Some(outside.to_str().unwrap().to_owned());
        let restored = session(&[one], None).restore(&path, 1);
        assert!(restored.items.is_empty() && restored.gone == 1);
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
        let json = session(&[item], None).to_json().unwrap();
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
        let session = session(&items, Some(2));
        assert!(session.to_json().is_ok());
        assert_eq!((session.items.len(), session.selected), (1, Some(0)));
    }
}

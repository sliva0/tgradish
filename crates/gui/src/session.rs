//! Persists open files, per-item settings, and window layout across restarts.
//!
//! Saved in the user state directory (`~/.local/state/tgradish` on Linux)
//! while the GUI runs and on exit, alongside copies of pasted clipboard images
//! (whose temporary folders are cleaned up when the GUI closes).
//!
//! If a session file cannot be parsed (e.g. written by a newer version), it is
//! moved aside rather than overwritten, and any pasted image copies it may
//! reference are preserved. The first running instance keeps the session and
//! holds its lock; later instances hand their files to it (see `instance`).

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tgradish_core::clipboard::PASTED_IMAGE_NAME;
use tgradish_core::presets::Format;

use crate::item::{Choices, Conversion, Item, Recipe};
use crate::jobs::Job;
use crate::media::Load;
use crate::outputs::{FileVersion, Written};

/// Session format version; files with a different version are moved aside.
const VERSION: u32 = 1;

/// Raw session layout used on load: items are parsed individually so an item
/// containing an unknown option from a newer version can be skipped without
/// discarding the rest, while a backup copy of the full file is set aside so
/// nothing is lost when saving. Any field this version doesn't know, in an
/// item or the session, makes what holds it unreadable, so it is set aside
/// whole rather than rewritten without what a newer version kept.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    version: u32,
    items: Vec<serde_json::Value>,
    selected: Option<usize>,
    #[serde(default)]
    window: Option<serde_json::Value>,
}

/// The session as its file holds it. Kept: each item's inputs (or its
/// pasted image's copy), whether they are a sequence, its format and
/// whether that was guessed, its [`Choices`] whole, and its last result
/// with the [`Recipe`] that made it; which item is selected; and the
/// window's layout. tests/data/session-v1.json is a session as version 1
/// first wrote it: every later version must still read it and restore its
/// items with their results current. tests/data/session-v2.json is the same
/// as it is written now, which reads and writes again as it was.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Session {
    version: u32,
    items: Vec<Saved>,
    /// Index of the selected item in `items`.
    selected: Option<usize>,
    /// Saved window geometry and splitter positions.
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<Window>,
}

/// Saved window geometry and panel split positions. Window size and position
/// are in OS logical points (excluding tgradish's internal UI scale).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct Window {
    /// Inner window size when last unmaximised.
    pub(crate) size: [f32; 2],
    /// Outer top-left window position when last unmaximised, if reported by the OS.
    pub(crate) position: Option<[f32; 2]>,
    pub(crate) maximized: bool,
    /// Width of the file list panel in egui points.
    pub(crate) list: f32,
    /// Fraction of the main panel height occupied by the preview.
    pub(crate) preview: f32,
}

/// Upper bound on valid window dimensions and coordinates (past any screen).
const FARTHEST: f32 = 32768.0;

impl Window {
    /// Clamps the window size (no smaller than the window's minimum), the
    /// list width and the preview share (within the divider's limits), and
    /// drops a position that no screen could have.
    pub(crate) fn clamped(self) -> Window {
        let within = |value: f32, least: f32, most: f32| {
            if value.is_finite() { value.clamp(least, most) } else { least }
        };
        let place = self.position.filter(|[x, y]| {
            [x, y].iter().all(|value| value.is_finite() && value.abs() <= FARTHEST)
        });
        Window {
            size: [
                within(self.size[0], crate::MIN_WIDTH, FARTHEST),
                within(self.size[1], crate::MIN_HEIGHT, FARTHEST),
            ],
            position: place,
            maximized: self.maximized,
            list: within(self.list, crate::app::NARROWEST_LIST, FARTHEST),
            preview: if self.preview.is_finite() {
                self.preview.clamp(crate::preview::LEAST_SHARE, crate::preview::MOST_SHARE)
            } else {
                crate::app::PREVIEW_SHARE
            },
        }
    }
}

/// Reads only the [`Window`] layout from the session file at `path` so the
/// native window can be sized and positioned before the rest of the session loads.
pub(crate) fn window(path: &Path) -> Option<Window> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut read: serde_json::Value = serde_json::from_str(&text).ok()?;
    let window = serde_json::from_value::<Window>(read.get_mut("window")?.take()).ok()?;
    Some(window.clamped())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Saved {
    inputs: Vec<PathBuf>,
    sequence: bool,
    /// Filename of the copied clipboard image inside the `pasted/` directory.
    // Serialised as `"pasted"` since version 1.
    #[serde(rename = "pasted")]
    pasted_copy: Option<String>,
    format: Format,
    format_guessed: bool,
    choices: Choices,
    /// Last completed conversion result and its recipe.
    result: Option<SavedResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct SavedResult {
    output: PathBuf,
    /// File version recorded when the GUI wrote `output` (`None` for a result
    /// found on disk rather than written by the GUI).
    written: Option<FileVersion>,
    /// Effective conversion recipe used to produce `output`, for staleness checks.
    recipe: Recipe,
}

/// Items and metadata restored from a saved session.
pub(crate) struct Restored {
    pub(crate) items: Vec<Item>,
    pub(crate) selected: Option<u64>,
    /// Number of saved items skipped because their input files no longer exist.
    pub(crate) gone: usize,
    /// Output files written by an earlier run for the restored items.
    pub(crate) written: Vec<(PathBuf, Written)>,
}

/// Returns the `pasted/` directory path next to the session file.
fn pasted_dir(path: &Path) -> PathBuf {
    path.with_file_name("pasted")
}

/// Returns true if `name` is a single normal path component (preventing path
/// traversal outside `pasted/`).
fn plain_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!((parts.next(), parts.next()), (Some(Component::Normal(_)), None))
}

/// Result of reading a session file from disk.
#[derive(Default)]
pub(crate) struct Loaded {
    pub(crate) session: Session,
    /// Populated when part or all of the session file could not be parsed.
    pub(crate) unreadable: Option<Unreadable>,
}

/// Details about a session file that could not be parsed in full.
pub(crate) struct Unreadable {
    /// Count of individual items skipped (`None` when the entire file was
    /// unreadable).
    pub(crate) files: Option<usize>,
    /// Path where the original session file was backed up.
    pub(crate) aside: PathBuf,
}

/// Active session file store: tracks the last serialised JSON, owned pasted
/// image copies, and the advisory lock file.
pub(crate) struct Store {
    path: PathBuf,
    /// Last JSON read or written, used to skip redundant disk writes.
    written: String,
    /// Filenames in `pasted/` owned by this session. Other files in `pasted/`
    /// may belong to a backed-up unreadable session.
    copies: HashSet<String>,
    /// Advisory lock file held while the GUI is running, so no other instance
    /// keeps a session here.
    lock: Option<std::fs::File>,
    /// Cached metadata and referenced pasted-image names for `.unreadable*`
    /// backup sessions so they are only re-read when modified.
    aside_read: Option<(Vec<AsideFile>, AsideNames)>,
}

/// Path, byte length, and modification timestamp of a `.unreadable*` session file.
type AsideFile = (PathBuf, u64, SystemTime);

/// Pasted image filenames referenced by `.unreadable*` session files, or `None`
/// if any backup file could not be parsed as JSON.
type AsideNames = Option<HashSet<String>>;

impl Drop for Store {
    // Explicitly unlock before closing: a child process (such as `ffmpeg`)
    // spawned concurrently inherits file descriptors briefly between `fork`
    // and `exec`, which would keep an implicit close-based lock alive while
    // another instance or test opens the session.
    fn drop(&mut self) {
        if let Some(lock) = &self.lock {
            let _ = lock.unlock();
        }
    }
}

impl Store {
    /// Opens the session store at `path` if no other instance holds its lock:
    /// two would overwrite each other's session and remove each other's pasted
    /// images.
    pub(crate) fn open(path: PathBuf) -> Option<Store> {
        let lock = lock(&path).ok()?;
        Some(Store { path, written: String::new(), copies: HashSet::new(), lock, aside_read: None })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Returns true if an exclusive file lock is held (false if the filesystem
    /// does not support locking).
    pub(crate) fn locked(&self) -> bool {
        self.lock.is_some()
    }

    /// Base filename prefix for backed-up unreadable session files.
    fn aside_name(&self) -> String {
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        format!("{name}.unreadable")
    }

    /// Writes `bytes` to a fresh `<session>.unreadable` (or `.unreadable-N`)
    /// backup file and returns its path.
    fn set_aside(&self, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let first = self.path.with_file_name(self.aside_name());
        for number in 1.. {
            let aside = match number {
                1 => first.clone(),
                _ => first.with_extension(format!("unreadable-{number}")),
            };
            let options = std::fs::File::options().write(true).create_new(true).clone();
            match options.open(&aside) {
                Ok(mut file) => {
                    use std::io::Write;
                    return match file.write_all(bytes).and_then(|()| file.sync_all()) {
                        Ok(()) => Ok(aside),
                        Err(err) => {
                            drop(file);
                            let _ = std::fs::remove_file(&aside);
                            Err(err)
                        }
                    };
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }
        unreachable!("a name is found before the numbers run out")
    }

    /// Lists all `.unreadable*` backup session files in the session directory,
    /// or returns `None` if the directory cannot be listed.
    fn set_aside_sessions(&self) -> Option<Vec<PathBuf>> {
        let name = self.aside_name();
        let Some(dir) = self.path.parent() else { return Some(Vec::new()) };
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
            Err(_) => return None,
        };
        let mut aside = Vec::new();
        for entry in entries {
            let entry = entry.ok()?;
            if entry.file_name().to_string_lossy().starts_with(&name) {
                aside.push(entry.path());
            }
        }
        aside.sort();
        Some(aside)
    }

    /// Returns the pasted-image filenames referenced by the backup files in
    /// `aside` (using cached results unless file sizes or mtimes changed), or
    /// `None` if any backup file could not be inspected.
    fn aside_names(&mut self, aside: &[PathBuf]) -> AsideNames {
        let seen: Option<Vec<AsideFile>> = aside
            .iter()
            .map(|path| {
                let meta = std::fs::metadata(path).ok()?;
                Some((path.clone(), meta.len(), meta.modified().ok()?))
            })
            .collect();
        if let (Some(seen), Some((read, names))) = (&seen, &self.aside_read)
            && seen == read
        {
            return names.clone();
        }
        let mut names = Some(HashSet::new());
        for path in aside {
            let named = std::fs::read(path).ok().and_then(|bytes| copies_named(&bytes));
            names = names.zip(named).map(|(mut names, named)| {
                names.extend(named);
                names
            });
        }
        // Only cache if all file metadata was readable.
        self.aside_read = seen.map(|seen| (seen, names.clone()));
        names
    }

    /// Loads the session from disk if present. Completely unreadable files are
    /// moved aside; partially readable files are copied aside before saving
    /// can overwrite them. Returns an error when setting aside fails.
    pub(crate) fn read(&mut self) -> std::io::Result<Loaded> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded::default()),
            Err(err) => return Err(err),
        };
        let Some((session, unread)) = parse(&String::from_utf8_lossy(&bytes)) else {
            let aside = self.set_aside(&bytes)?;
            std::fs::remove_file(&self.path)?;
            let unreadable = Unreadable { files: None, aside };
            return Ok(Loaded { session: Session::default(), unreadable: Some(unreadable) });
        };
        let unreadable = if unread > 0 {
            Some(Unreadable { files: Some(unread), aside: self.set_aside(&bytes)? })
        } else {
            None
        };
        self.written = session.to_json().unwrap_or_default();
        self.copies = session.items.iter().filter_map(|item| item.pasted_copy.clone()).collect();
        Ok(Loaded { session, unreadable })
    }

    /// Copies the pasted clipboard image for item `id` into `pasted/` under a
    /// unique filename and returns that filename.
    pub(crate) fn copy_pasted(&mut self, id: u64, image: &Path) -> std::io::Result<String> {
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let extension = image.extension().and_then(|e| e.to_str()).unwrap_or("png");
        let name = format!("{time:x}-{id}.{extension}");
        let dir = pasted_dir(&self.path);
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(image, dir.join(&name))?;
        self.copies.insert(name.clone());
        Ok(name)
    }

    /// Atomically writes `session` if it changed and prunes unreferenced
    /// pasted-image copies (retaining any listed in `held` for recently
    /// removed items that can still be restored via Undo).
    pub(crate) fn save(
        &mut self,
        session: &Session,
        held: impl IntoIterator<Item = String>,
    ) -> std::io::Result<()> {
        let json = session.to_json().map_err(std::io::Error::other)?;
        let mut named: HashSet<String> =
            session.items.iter().filter_map(|item| item.pasted_copy.clone()).collect();
        named.extend(held);
        // Re-check copies when recently removed items expire from `held` even
        // if `session` itself did not change.
        if json == self.written && named == self.copies {
            return Ok(());
        }
        if json != self.written {
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            // Write and sync atomically before replacing the previous file.
            tgradish_core::fsutil::write_file(&self.path, json.as_bytes(), true)?;
            self.written = json;
        }
        // Delete unreferenced copies in `pasted/`, taking care not to delete
        // files belonging to `.unreadable*` backup sessions: if backups exist
        // and can all be parsed, delete only copies owned by this instance
        // that no backup names; if any backup cannot be parsed (or the
        // directory cannot be listed), keep all copies.
        let leftover = match self.set_aside_sessions() {
            Some(aside) if aside.is_empty() => Leftover::All,
            Some(aside) => match self.aside_names(&aside) {
                Some(kept) => Leftover::MineBut(kept),
                None => Leftover::None,
            },
            None => Leftover::None,
        };
        if let Ok(entries) = std::fs::read_dir(pasted_dir(&self.path)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let gone = !named.contains(&name)
                    && match &leftover {
                        Leftover::All => true,
                        Leftover::MineBut(kept) => {
                            self.copies.contains(&name) && !kept.contains(&name)
                        }
                        Leftover::None => false,
                    };
                if gone {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        self.copies = named;
        Ok(())
    }
}

/// Policy for pruning unreferenced files in `pasted/` during [`Store::save`].
enum Leftover {
    /// Remove all unreferenced files (no `.unreadable*` backup sessions exist).
    All,
    /// Remove only files owned by this instance that are not in the given
    /// backup-referenced set.
    MineBut(HashSet<String>),
    /// Remove nothing (a backup session could not be inspected).
    None,
}

/// Error indicating another running instance holds the session lock.
struct Held;

/// Takes a non-blocking exclusive lock on the session's `.lock` sibling
/// (`path` with its extension replaced), held until the store unlocks it or
/// the process ends. Returns `Ok(None)` where there is no telling whether
/// another instance runs (the lock file can't be created, or the filesystem
/// has no locks): the session is then kept unlocked, and a failed write says
/// so.
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

/// Extracts all `"pasted"` filenames from a raw JSON session file without
/// deserialising full item structs (so items from newer versions still report
/// their copies). Returns `None` if `bytes` is not valid JSON with an `"items"`
/// array.
fn copies_named(bytes: &[u8]) -> Option<HashSet<String>> {
    use serde_json::Value;
    let session = serde_json::from_slice::<Value>(bytes).ok()?;
    let mut named = HashSet::new();
    for item in session.as_object()?.get("items")?.as_array()? {
        match item.as_object()?.get("pasted") {
            None | Some(Value::Null) => {}
            Some(Value::String(name)) => {
                named.insert(name.clone());
            }
            Some(_) => return None,
        }
    }
    Some(named)
}

/// Parses `text` as a version-matching [`Session`], returning the parsed
/// session and the count of individual items that failed to deserialise.
fn parse(text: &str) -> Option<(Session, usize)> {
    let read: Read =
        serde_json::from_str(text).ok().filter(|read: &Read| read.version == VERSION)?;
    // Ignore an invalid `window` field without rejecting the items.
    let window = read.window.and_then(|window| serde_json::from_value(window).ok());
    let mut session = Session { version: VERSION, items: Vec::new(), selected: None, window };
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

/// Returns true if `item` can be saved in JSON (all input paths are valid
/// UTF-8 and any pasted clipboard image has a persistent copy in `pasted/`).
fn keeps(item: &Item) -> bool {
    item.inputs.iter().all(|input| input.to_str().is_some())
        && (item.paste_dir.is_none() || item.pasted_copy.is_some())
}

impl Session {
    /// Builds a serialisable [`Session`] from `items`, `selected`, and
    /// `written`, omitting items that cannot be represented in JSON.
    pub(crate) fn of(
        items: &[Item],
        selected: Option<u64>,
        written: &HashMap<PathBuf, Written>,
    ) -> Session {
        let kept: Vec<&Item> = items.iter().filter(|item| keeps(item)).collect();
        let saved = kept
            .iter()
            .map(|item| {
                let made = item.last_result().filter(|made| made.job.output.to_str().is_some());
                // Every result has a recipe: a conversion without one fails as
                // it starts.
                let made = made.and_then(|made| Some((made, made.recipe.clone()?)));
                Saved {
                    // Pasted images are restored from their copy in `pasted/`.
                    inputs: if item.pasted_copy.is_some() {
                        Vec::new()
                    } else {
                        item.inputs.clone()
                    },
                    sequence: item.sequence,
                    pasted_copy: item.pasted_copy.clone(),
                    format: item.format,
                    format_guessed: item.format_guessed,
                    choices: item.choices.clone(),
                    result: made.map(|(made, recipe)| SavedResult {
                        output: made.job.output.clone(),
                        written: written
                            .get(&made.job.output)
                            .filter(|written| written.item == item.id)
                            .map(|written| written.version),
                        recipe,
                    }),
                }
            })
            .collect();
        let selected = selected.and_then(|id| kept.iter().position(|item| item.id == id));
        Session { version: VERSION, items: saved, selected, window: None }
    }

    /// Attaches the current [`Window`] layout to `self`.
    pub(crate) fn with_window(self, window: Option<Window>) -> Session {
        Session { window, ..self }
    }

    pub(crate) fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Restores saved items with IDs starting at `next_id`.
    pub(crate) fn restore(self, path: &Path, mut next_id: u64) -> Restored {
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
    let (inputs, pasted) = match &saved.pasted_copy {
        Some(name) if !plain_name(name) => return None,
        // Copy a pasted image back into its own temporary directory under
        // `PASTED_IMAGE_NAME` so default output naming matches the original paste.
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
    item.paste_dir = pasted;
    item.pasted_copy = saved.pasted_copy;
    item.format = saved.format;
    item.format_guessed = saved.format_guessed;
    item.choices = saved.choices;
    // Re-verify the previous output file asynchronously in `work::pump`.
    item.earlier = saved.result.map(|result| {
        (Conversion::earlier(Job::waiting(result.output), result.recipe), Load::Idle)
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
        item.paste_dir = Some(dir);
        item
    }

    fn contents(items: &[Item]) -> Vec<Vec<u8>> {
        items.iter().map(|item| std::fs::read(&item.inputs[0]).unwrap()).collect()
    }

    /// Saves `items` at `path`, pasted images copied there as the GUI
    /// copies them, and opens them again, numbered from 1.
    fn again(items: &mut [Item], path: &Path) -> Vec<Item> {
        let mut store = Store::open(path.to_path_buf()).unwrap();
        store.read().unwrap();
        for item in items.iter_mut().filter(|item| item.pasted_copy.is_none()) {
            item.pasted_copy = Some(store.copy_pasted(item.id, &item.inputs[0]).unwrap());
        }
        store.save(&session(items, None), []).unwrap();
        drop(store);
        Store::open(path.to_path_buf()).unwrap().read().unwrap().session.restore(path, 1).items
    }

    /// The session of `items`, none of whose results the GUI wrote.
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
    fn lets_one_instance_at_a_time_keep_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("session.json");
        let first = Store::open(path.clone()).expect("nobody has it");
        assert!(Store::open(path.clone()).is_none());
        // until the first is gone
        drop(first);
        assert!(Store::open(path).is_some());
    }

    #[test]
    fn sets_aside_a_session_it_cannot_read() {
        // of another version, and not a session at all, which there is no
        // telling what copies it names
        let texts = [(r#"{"version":2,"items":[],"selected":null}"#, 1), ("not a session", 2)];
        for (text, left) in texts {
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
            assert_eq!(std::fs::read_to_string(&unreadable.aside).unwrap(), text);
            // the GUI goes on with a session of its own, whose copies
            // come and go, and leaves the other's alone
            let mut items = [pasted(1, b"one")];
            items[0].pasted_copy = Some(store.copy_pasted(1, &items[0].inputs[0]).unwrap());
            store.save(&session(&items, None), []).unwrap();
            assert_eq!(copies(&path), 2);
            store.save(&session(&[], None), []).unwrap();
            assert_eq!(copies(&path), left);
            assert_eq!(std::fs::read(pasted_dir(&path).join("19a0-3.png")).unwrap(), b"pasted");
            assert_eq!(std::fs::read_to_string(&unreadable.aside).unwrap(), text);
        }
    }

    #[test]
    fn leaves_out_only_the_files_it_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut store = Store::open(path.clone()).unwrap();
        let mut items = [pasted(1, b"one"), pasted(2, b"two")];
        for item in &mut items {
            item.pasted_copy = Some(store.copy_pasted(item.id, &item.inputs[0]).unwrap());
            item.choices.webm.crf = Some(20);
        }
        store.save(&session(&items, Some(2)), []).unwrap();
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
        assert_eq!(std::fs::read_to_string(&unreadable.aside).unwrap(), newer);
        // the other is there, and still the one selected
        let restored = loaded.session.restore(&path, 7);
        assert_eq!(contents(&restored.items), [b"two".to_vec()]);
        assert_eq!((restored.selected, restored.gone), (Some(7), 0));
        // and writing the session leaves the copy of the one left out
        store.save(&session(&restored.items, restored.selected), []).unwrap();
        assert_eq!(copies(&path), 2);
    }

    #[test]
    fn keeps_every_session_it_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut texts = vec![
            "not a session".to_owned(),
            r#"{"version":2,"items":[],"selected":null}"#.to_owned(),
        ];
        // and one read in part, which is copied aside rather than moved
        let item = Item::new(1, vec![PathBuf::from("/a.png")], false, Target::Sticker);
        let text = session(&[item], None).to_json().unwrap();
        texts.push(text.replacen(r#""webm":{"#, r#""webm":{"made-up":1,"#, 1));
        let mut asides = Vec::new();
        for text in &texts {
            std::fs::write(&path, text).unwrap();
            let mut store = Store::open(path.clone()).unwrap();
            asides.push(store.read().unwrap().unreadable.expect("unreadable").aside);
        }
        let kept: Vec<String> =
            asides.iter().map(|aside| std::fs::read_to_string(aside).unwrap()).collect();
        assert_eq!(kept, texts);
    }

    #[test]
    fn keeps_the_copies_a_session_set_aside_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut store = Store::open(path.clone()).unwrap();
        let mut items = [pasted(1, b"one"), pasted(2, b"two")];
        for item in &mut items {
            item.pasted_copy = Some(store.copy_pasted(item.id, &item.inputs[0]).unwrap());
        }
        items[0].choices.webm.crf = Some(20);
        store.save(&session(&items, None), []).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replacen(r#""webm":{"#, r#""webm":{"made-up":1,"#, 1)).unwrap();
        drop(store);

        // the second is taken back, then removed
        let mut store = Store::open(path.clone()).unwrap();
        let loaded = store.read().unwrap();
        let aside = loaded.unreadable.expect("unreadable").aside;
        assert_eq!(loaded.session.restore(&path, 1).items.len(), 1);
        store.save(&session(&[], None), []).unwrap();
        // the session set aside still names it
        assert_eq!(copies(&path), 2);
        // until it is gone
        std::fs::remove_file(aside).unwrap();
        let mut three = pasted(3, b"three");
        three.pasted_copy = Some(store.copy_pasted(3, &three.inputs[0]).unwrap());
        store.save(&session(&[three], None), []).unwrap();
        assert_eq!(copies(&path), 1);
    }

    #[test]
    fn keeps_every_copy_while_what_a_session_set_aside_names_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut store = Store::open(path.clone()).unwrap();
        let mut items = [pasted(1, b"one"), pasted(2, b"two")];
        for item in &mut items {
            item.pasted_copy = Some(store.copy_pasted(item.id, &item.inputs[0]).unwrap());
        }
        items[0].choices.webm.crf = Some(20);
        store.save(&session(&items, None), []).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let newer = text.replacen(r#""webm":{"#, r#""webm":{"made-up":1,"#, 1);
        std::fs::write(&path, &newer).unwrap();
        drop(store);

        // the second is taken back, and the session set aside made to
        // name the first alone, which it has read
        let mut store = Store::open(path.clone()).unwrap();
        let loaded = store.read().unwrap();
        let aside = loaded.unreadable.expect("unreadable").aside;
        let mut first: serde_json::Value = serde_json::from_str(&newer).unwrap();
        first["items"].as_array_mut().unwrap().truncate(1);
        std::fs::write(&aside, first.to_string()).unwrap();
        let mut items = loaded.session.restore(&path, 1).items;
        let mut three = pasted(3, b"three");
        three.pasted_copy = Some(store.copy_pasted(3, &three.inputs[0]).unwrap());
        items.push(three);
        store.save(&session(&items, None), []).unwrap();
        assert_eq!(copies(&path), 3);
        // then it can't be told what it names, as it isn't whole
        std::fs::write(&aside, &newer[..newer.len() / 2]).unwrap();
        store.save(&session(&[], None), []).unwrap();
        assert_eq!(copies(&path), 3);
        // and once it is gone, so are they
        std::fs::remove_file(aside).unwrap();
        let mut four = pasted(4, b"four");
        four.pasted_copy = Some(store.copy_pasted(4, &four.inputs[0]).unwrap());
        store.save(&session(&[four], None), []).unwrap();
        assert_eq!(copies(&path), 1);
    }

    #[test]
    fn sets_aside_whatever_holds_what_it_does_not_know() {
        let mut item = Item::new(1, vec![PathBuf::from("/a.png")], false, Target::Sticker);
        item.earlier = Some((
            Conversion::of(
                Job::waiting("/a.sticker.tgs".into()),
                Format::Tgs,
                item.choices.clone(),
            ),
            Load::Idle,
        ));
        item.earlier.as_mut().unwrap().0.recipe = Some(Recipe::Tgs(Default::default()));
        let written = HashMap::from([(
            PathBuf::from("/a.sticker.tgs"),
            Written { item: 1, version: FileVersion::of(Path::new("/")).unwrap() },
        )]);
        let text = Session::of(&[item], None, &written).to_json().unwrap();
        assert_eq!(
            parse(&text).map(|(session, unread)| (session.items.len(), unread)),
            Some((1, 0))
        );
        // a key of its own anywhere in an item leaves the item out
        for within in
            [r#""items":[{"#, r#""choices":{"#, r#""result":{"#, r#""written":{"#, r#""tgs":{"#]
        {
            assert!(text.contains(within), "{within} in {text}");
            // where it is all there is, as in options none of which are set
            let newer =
                text.replacen(within, &format!(r#"{within}"made-up":1,"#), 1).replace(",}", "}");
            let read = parse(&newer).map(|(session, unread)| (session.items.len(), unread));
            assert_eq!(read, Some((0, 1)), "{newer}");
        }
        // and one of the session's own leaves out all of it
        let newer = text.replacen('{', r#"{"made-up":1,"#, 1);
        assert!(parse(&newer).is_none(), "{newer}");
    }

    /// A session as version 1 of the file wrote it before what is one
    /// file's own was kept apart from a format's options: a WebM with its
    /// result, a sprite sheet's `.tgs` and a picture with theirs, a pasted
    /// image, an Aseprite file's tag and layers, and frames of a sequence.
    /// Its files were in `@DIR@`.
    const VERSION_1: &str = include_str!("../tests/data/session-v1.json");

    /// The same session as it is written now: the title in either format's
    /// options, and what is one file's own before them, which version 1
    /// reads as it is.
    const VERSION_2: &str = include_str!("../tests/data/session-v2.json");

    /// The session `fixture` read and restored, its files made where it
    /// says in a folder of its own, each result taken up as it is once found
    /// on disk; and the session written again, `@DIR@` for that folder.
    fn restored(fixture: &str) -> (Vec<bool>, String) {
        use tgradish_core::presets::Presets;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        // its files where it says, which restoring looks for
        let quoted = serde_json::to_string(dir.path().to_str().unwrap()).unwrap();
        let folder = quoted.trim_matches('"');
        let text = fixture.trim_end().replace("@DIR@", folder);
        let files = ["clip.mp4", "sheet.png", "photo.png", "walk.aseprite", "frames/1.png"];
        for name in files.iter().chain(&["frames/2.png", "pasted/19a0-4.png"]) {
            let file = dir.path().join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"").unwrap();
        }
        std::fs::write(&path, &text).unwrap();
        let loaded = Store::open(path.clone()).unwrap().read().unwrap();
        assert!(loaded.unreadable.is_none());
        let window = loaded.session.window;
        let restored = loaded.session.restore(&path, 1);
        assert_eq!((restored.items.len(), restored.gone, restored.selected), (6, 0, Some(3)));
        let mut items = restored.items;
        let (presets, config) = (Presets::builtin(), crate::config::Config::default());
        for item in &mut items {
            item.recipe = item.choices.recipe(item.format, &presets, &config).ok();
            // as an earlier result is taken up once found on disk
            item.made = item.earlier.take().map(|(earlier, _)| earlier);
        }
        let current = items.iter().map(Item::result_is_current).collect();
        let written = restored.written.into_iter().collect();
        let again = Session::of(&items, restored.selected, &written).with_window(window);
        (current, again.to_json().unwrap().replace(folder, "@DIR@"))
    }

    /// Every later version must read a session version 1 wrote, and take
    /// each result as what converting makes now; it writes it as it writes
    /// sessions now.
    #[test]
    fn reads_a_session_version_1_wrote() {
        let (current, again) = restored(VERSION_1);
        assert_eq!(current, [true, true, true, false, false, false]);
        assert_eq!(again, VERSION_2.trim_end());
    }

    /// And one written now, which it writes again as it was.
    #[test]
    fn writes_a_session_again_as_it_was() {
        let (current, again) = restored(VERSION_2);
        assert_eq!(current, [true, true, true, false, false, false]);
        assert_eq!(again, VERSION_2.trim_end());
    }

    #[test]
    fn copies_pasted_images_only_within_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let outside = dir.path().join("precious.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let mut one = pasted(1, b"one");
        one.pasted_copy = Some(outside.to_str().unwrap().to_owned());
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
        item.choices.webm.integer_scale = "2".parse().ok();
        let json = session(&[item], None).to_json().unwrap();
        for name in ["format-guessed", "show-layers", "integer-scale"] {
            assert!(json.contains(&format!("\"{name}\":")), "{name} in {json}");
        }
        assert!(!json.contains('_'), "{json}");
    }

    /// Options renamed since a session was kept read by their old names.
    #[test]
    fn reads_options_by_their_old_names() {
        use tgradish_core::options::{Effort, Scaling};
        let mut item = Item::new(1, vec![PathBuf::from("/a.png")], false, Target::Sticker);
        item.choices.webm.effort = Some(Effort::Best);
        item.choices.tgs.effort = Some(Effort::Fast);
        item.choices.webm.scaling = Some(Scaling::Integer);
        item.choices.webm.integer_scale = "2".parse().ok();
        let json = session(&[item], None).to_json().unwrap();
        let old = json
            .replace("\"effort\":", "\"speed\":")
            .replace("\"integer\"", "\"pixel-perfect\"")
            .replace("\"integer-scale\":", "\"exact-scale\":");
        assert_eq!(old.matches("\"speed\":").count(), 2, "{old}");
        assert!(old.contains("\"pixel-perfect\"") && old.contains("\"exact-scale\":"), "{old}");
        let (read, unread) = parse(&old).unwrap();
        assert_eq!(unread, 0);
        let choices = &read.items[0].choices;
        assert_eq!(
            (choices.webm.effort, choices.tgs.effort),
            (Some(Effort::Best), Some(Effort::Fast))
        );
        assert_eq!(choices.webm.scaling, Some(Scaling::Integer));
        assert_eq!(choices.webm.integer_scale, "2".parse().ok());
    }

    #[test]
    fn names_results_by_their_recipes_alone() {
        let mut item = Item::new(1, vec![PathBuf::from("/a.png")], false, Target::Sticker);
        let job = Job::waiting("/a.sticker.webp".into());
        let mut made = Conversion::of(job, Format::Webp, item.choices.clone());
        made.recipe = Some(Recipe::Webp(Default::default()));
        item.earlier = Some((made, Load::Idle));
        let json = session(&[item], None).to_json().unwrap();
        assert!(json.contains(r#""recipe":{"webp":{"#), "{json}");
        // which is all it says of how it was made
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let result = value["items"][0]["result"].as_object().unwrap();
        let mut keys: Vec<&str> = result.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["output", "recipe", "written"]);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_how_the_window_was_laid_out_within_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let window = Window {
            size: [1500.0, 900.0],
            position: Some([40.0, 30.0]),
            maximized: true,
            list: 380.0,
            preview: 0.5,
        };
        let mut store = Store::open(path.clone()).unwrap();
        store.save(&session(&[], None).with_window(Some(window)), []).unwrap();
        assert_eq!(super::window(&path), Some(window));
        // sizes no window could have, and a place off any screen
        let odd = Window {
            size: [10.0, f32::NAN],
            position: Some([1e9, 0.0]),
            maximized: false,
            list: 1.0,
            preview: 5.0,
        };
        let clamped = odd.clamped();
        assert_eq!(clamped.size, [crate::MIN_WIDTH, crate::MIN_HEIGHT]);
        assert_eq!((clamped.position, clamped.preview), (None, crate::preview::MOST_SHARE));
        assert_eq!(clamped.list, crate::app::NARROWEST_LIST);
        // one it can't read is left out alone, and the files read
        let text = r#"{"version":1,"items":[],"selected":null,"window":{"size":"large"}}"#;
        std::fs::write(&path, text).unwrap();
        assert_eq!(super::window(&path), None);
        drop(store);
        let loaded = Store::open(path.clone()).unwrap().read().unwrap();
        assert!(loaded.unreadable.is_none());
    }

    #[test]
    fn leaves_out_a_pasted_image_it_has_no_copy_of() {
        let dir = tempfile::tempdir().unwrap();
        let fine = dir.path().join("fine.png");
        let items = [pasted(1, b"one"), Item::new(2, vec![fine], false, Target::Sticker)];
        let session = session(&items, Some(2));
        assert!(session.to_json().is_ok());
        assert_eq!((session.items.len(), session.selected), (1, Some(0)));
    }

    // only Unix lets a file be named with bytes that aren't UTF-8
    #[cfg(unix)]
    #[test]
    fn leaves_out_a_name_it_cannot_hold() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join(std::ffi::OsStr::from_bytes(b"odd\xff.png"));
        let fine = dir.path().join("fine.png");
        let items = [
            Item::new(1, vec![odd], false, Target::Sticker),
            Item::new(2, vec![fine], false, Target::Sticker),
        ];
        let session = session(&items, Some(2));
        assert!(session.to_json().is_ok());
        assert_eq!((session.items.len(), session.selected), (1, Some(0)));
    }
}

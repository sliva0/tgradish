//! The system's fonts, for characters the window's own fonts lack: CJK,
//! Arabic, Hebrew, Thai, Indic and other scripts in file names. A font is
//! looked for when such a character first shows up, read on another
//! thread, and added behind the window's fonts, so nothing is loaded for
//! names that don't need it.
//!
//! Linux asks fontconfig for a font with the character; Windows and macOS
//! try the fonts they come with. Colour emoji fonts are bitmaps egui can't
//! draw, so emoji are only shown where a font has them as outlines.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, FontData, FontFamily};
use skrifa::MetadataProvider;

/// A font found for some characters.
struct Found {
    path: PathBuf,
    bytes: Vec<u8>,
    /// Which font of a collection (`.ttc`) has them.
    index: u32,
    /// Every character it has.
    chars: Vec<char>,
}

/// Every character the font `index` of `bytes` has.
fn characters(bytes: &[u8], index: u32) -> Vec<char> {
    skrifa::FontRef::from_index(bytes, index)
        .map(|font| {
            font.charmap().mappings().filter_map(|(code, _)| char::from_u32(code)).collect()
        })
        .unwrap_or_default()
}

pub struct Fallbacks {
    fonts: egui::FontDefinitions,
    /// Characters some font has. egui's own check says no for characters
    /// of the font that also draws its replacement box, so it is kept here.
    covered: HashSet<char>,
    /// Characters looked for.
    asked: HashSet<char>,
    /// Blocks of 128 characters with a lookup running: one at a time, as
    /// a font found for one character usually has its neighbours.
    searching: HashSet<u32>,
    loaded: HashSet<PathBuf>,
    pending: Vec<(u32, Receiver<Option<Found>>)>,
}

fn block(c: char) -> u32 {
    c as u32 >> 7
}

/// Characters that only shape others and draw nothing themselves: zero
/// width spaces and joiners, direction marks, variation selectors, tags.
pub fn formatting(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x034F
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xE0000..=0xE007F
            | 0xE0100..=0xE01EF
    )
}

impl Fallbacks {
    /// Falls back from `fonts`, the window's own.
    pub fn new(fonts: egui::FontDefinitions) -> Fallbacks {
        let covered =
            fonts.font_data.values().flat_map(|data| characters(&data.font, data.index)).collect();
        Fallbacks {
            fonts,
            covered,
            asked: HashSet::new(),
            searching: HashSet::new(),
            loaded: HashSet::new(),
            pending: Vec::new(),
        }
    }

    /// Whether no font has `c`, the system's looked through.
    pub fn missing(&self, c: char) -> bool {
        self.asked.contains(&c) && !self.searching.contains(&block(c)) && !self.covered.contains(&c)
    }

    /// Looks for fonts for the characters of `texts` the window can't show,
    /// and adds those found by now.
    pub fn check<'a>(&mut self, ctx: &egui::Context, texts: impl IntoIterator<Item = &'a str>) {
        let mut added = false;
        let mut done = Vec::new();
        self.pending.retain(|(block, receiver)| match receiver.try_recv() {
            Ok(found) => {
                done.push(*block);
                if let Some(found) = found
                    && self.loaded.insert(found.path.clone())
                {
                    self.covered.extend(found.chars);
                    let name = format!("system {}", found.path.display());
                    let mut data = FontData::from_owned(found.bytes);
                    data.index = found.index;
                    self.fonts.font_data.insert(name.clone(), Arc::new(data));
                    for family in [FontFamily::Proportional, FontFamily::Monospace] {
                        self.fonts.families.entry(family).or_default().push(name.clone());
                    }
                    added = true;
                }
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done.push(*block);
                false
            }
        });
        for block in done {
            self.searching.remove(&block);
        }
        if added {
            ctx.set_fonts(self.fonts.clone());
        }
        for text in texts {
            if text.is_ascii() {
                continue;
            }
            for c in text.chars() {
                if c.is_control()
                    || formatting(c)
                    || self.asked.contains(&c)
                    || self.searching.contains(&block(c))
                    || self.covered.contains(&c)
                {
                    continue;
                }
                self.asked.insert(c);
                self.searching.insert(block(c));
                let (sender, receiver) = channel();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = sender.send(find(c));
                    ctx.request_repaint();
                });
                self.pending.push((block(c), receiver));
            }
        }
    }
}

/// The first font of the system that has `c`.
fn find(c: char) -> Option<Found> {
    for path in candidates(c) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        // collections hold several fonts; most have the same characters
        let count = match skrifa::raw::FileRef::new(&bytes) {
            Ok(skrifa::raw::FileRef::Collection(collection)) => collection.len(),
            _ => 1,
        };
        let index = (0..count.min(8)).find(|&index| {
            skrifa::FontRef::from_index(&bytes, index)
                .is_ok_and(|font| font.charmap().map(c).is_some())
        });
        if let Some(index) = index {
            let chars = characters(&bytes, index);
            return Some(Found { path, bytes, index, chars });
        }
    }
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
fn candidates(c: char) -> Vec<PathBuf> {
    // fontconfig picks a font with the character; colour fonts are bitmaps
    let pattern = format!(":charset={:x}:color=false", c as u32);
    let found = std::process::Command::new("fc-match")
        .args(["--format=%{file}", &pattern])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|path| !path.is_empty());
    found.into_iter().map(PathBuf::from).collect()
}

#[cfg(windows)]
fn candidates(_: char) -> Vec<PathBuf> {
    let windows =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from);
    [
        // Chinese, Japanese, Korean
        "msyh.ttc",
        "YuGothM.ttc",
        "meiryo.ttc",
        "malgun.ttf",
        "simsun.ttc",
        // Arabic, Hebrew and much else
        "segoeui.ttf",
        "arial.ttf",
        // Thai, Lao, Khmer
        "LeelawUI.ttf",
        "tahoma.ttf",
        // Indic scripts
        "Nirmala.ttf",
        // African and Native American scripts, symbols
        "ebrima.ttf",
        "gadugi.ttf",
        "seguisym.ttf",
        "seguiemj.ttf",
    ]
    .iter()
    .map(|name| windows.join("Fonts").join(name))
    .collect()
}

#[cfg(target_os = "macos")]
fn candidates(_: char) -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/GeezaPro.ttc",
        "/System/Library/Fonts/SFHebrew.ttf",
        "/System/Library/Fonts/Thonburi.ttc",
        "/System/Library/Fonts/Kohinoor.ttc",
        // much of Unicode, where it is installed
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(any(unix, windows)))]
fn candidates(_: char) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_system_fonts_for_other_scripts() {
        // whatever this machine has; nothing is no failure
        if let Some(found) = find('中') {
            let font = skrifa::FontRef::from_index(&found.bytes, found.index).unwrap();
            assert!(font.charmap().map('中').is_some(), "{}", found.path.display());
        }
        assert!(find('\u{10ffff}').is_none());
    }
}

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

use eframe::egui::{self, FontData, FontFamily, FontId};
use skrifa::MetadataProvider;

/// A font found for some characters.
struct Found {
    path: PathBuf,
    bytes: Vec<u8>,
    /// Which font of a collection (`.ttc`) has them.
    index: u32,
}

pub struct Fallbacks {
    fonts: egui::FontDefinitions,
    /// Blocks of 128 characters already looked for.
    asked: HashSet<u32>,
    loaded: HashSet<PathBuf>,
    pending: Vec<Receiver<Option<Found>>>,
}

impl Fallbacks {
    /// Falls back from `fonts`, the window's own.
    pub fn new(fonts: egui::FontDefinitions) -> Fallbacks {
        Fallbacks { fonts, asked: HashSet::new(), loaded: HashSet::new(), pending: Vec::new() }
    }

    /// Looks for fonts for the characters of `texts` the window can't show,
    /// and adds those found by now.
    pub fn check<'a>(&mut self, ctx: &egui::Context, texts: impl IntoIterator<Item = &'a str>) {
        let mut added = false;
        self.pending.retain(|receiver| match receiver.try_recv() {
            Ok(found) => {
                if let Some(found) = found
                    && self.loaded.insert(found.path.clone())
                {
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
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        if added {
            ctx.set_fonts(self.fonts.clone());
        }
        let font = FontId::proportional(14.0);
        for text in texts {
            // most text is covered: skip the lookup per character
            if text.is_ascii() || ctx.fonts_mut(|fonts| fonts.has_glyphs(&font, text)) {
                continue;
            }
            for c in text.chars() {
                if c.is_control() || !self.asked.insert(c as u32 >> 7) {
                    continue;
                }
                if ctx.fonts_mut(|fonts| fonts.has_glyph(&font, c)) {
                    continue;
                }
                let (sender, receiver) = channel();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = sender.send(find(c));
                    ctx.request_repaint();
                });
                self.pending.push(receiver);
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
            return Some(Found { path, bytes, index });
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

//! What a WebP is: its size, encoding and maker, and its picture.

use std::io::Cursor;
use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::telegram::{self, Issue, Target};

/// The tool a WebP's XMP names as its maker, if it has one.
pub fn creator_tool(webp: &[u8]) -> Option<String> {
    if webp.get(..4)? != b"RIFF" || webp.get(8..12)? != b"WEBP" {
        return None;
    }
    let mut chunks = webp.get(12..)?;
    while chunks.len() >= 8 {
        let size = u32::from_le_bytes(chunks[4..8].try_into().ok()?) as usize;
        let data = chunks.get(8..8 + size)?;
        if &chunks[..4] == b"XMP " {
            let text = std::str::from_utf8(data).ok()?;
            let key = "xmp:CreatorTool=\"";
            let start = text.find(key)? + key.len();
            let end = start + text[start..].find('"')?;
            return Some(text[start..end].to_owned());
        }
        // chunks are padded to even sizes
        chunks = chunks.get(8 + size + size % 2..)?;
    }
    None
}

/// What an image sticker is, for inspecting it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub alpha: bool,
    pub lossy: bool,
    pub animated: bool,
    /// The tool its metadata names as its maker.
    pub made_with: Option<String>,
}

impl Info {
    /// What it would be: emoji are the only image stickers of their box's
    /// size, square 100 px.
    pub fn target(&self) -> Target {
        if (self.width, self.height) == Target::Emoji.box_size() {
            Target::Emoji
        } else {
            Target::Sticker
        }
    }

    /// Everything that would make Telegram refuse it as `target`.
    pub fn issues(&self, target: Target) -> Vec<Issue> {
        let mut issues = telegram::check_still(self.width, self.height, self.bytes, target);
        if self.animated {
            issues.push(Issue::Animated);
        }
        issues
    }
}

/// What the WebP at `path` is.
pub fn inspect_file(path: &Path) -> Result<Info> {
    let bytes = crate::art::read(path)?;
    let invalid = |message: String| Error::Unreadable { path: path.to_path_buf(), message };
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(&bytes))
        .map_err(|err| invalid(err.to_string()))?;
    let (width, height) = decoder.dimensions();
    let (alpha, lossy, animated) = (decoder.has_alpha(), decoder.is_lossy(), decoder.is_animated());
    // the headers alone can be whole while the picture isn't
    let size = decoder.output_buffer_size().ok_or_else(|| invalid("too large".into()))?;
    decoder.read_image(&mut vec![0; size]).map_err(|err| invalid(err.to_string()))?;
    Ok(Info {
        width,
        height,
        bytes: bytes.len() as u64,
        alpha,
        lossy,
        animated,
        made_with: creator_tool(&bytes),
    })
}

/// A WebP, decoded for a preview: one frame, as long as a `.tgs` one.
pub fn preview(path: &Path) -> Result<crate::preview::Preview> {
    let bytes = crate::art::read(path)?;
    let invalid = |message: String| Error::Unreadable { path: path.to_path_buf(), message };
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(&bytes))
        .map_err(|err| invalid(err.to_string()))?;
    let (width, height) = decoder.dimensions();
    let size = decoder.output_buffer_size().ok_or_else(|| invalid("too large".into()))?;
    let mut pixels = vec![0; size];
    decoder.read_image(&mut pixels).map_err(|err| invalid(err.to_string()))?;
    let rgba = if decoder.has_alpha() {
        pixels
    } else {
        pixels.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect()
    };
    Ok(crate::preview::Preview { width, height, frames: vec![(rgba, 1)] })
}

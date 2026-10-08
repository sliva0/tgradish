//! `convert --clipboard`: inputs from the clipboard.

use std::io::BufWriter;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};

/// Name a pasted image is converted under, so the result is
/// `clipboard.sticker.webm`.
pub const PASTED_IMAGE_NAME: &str = "clipboard.png";

/// Decodes `%XX` escapes in a `file://` URI path.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |i: usize| bytes.get(i).and_then(|&b| (b as char).to_digit(16));
        match (bytes[i], hex(i + 1), hex(i + 2)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                i += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Paths in copied text: plain paths or `file://` URIs, one per line.
fn paths_in_text(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| match line.strip_prefix("file://") {
            // file:///home/x on Unix, file:///C:/x on Windows
            Some(rest) => {
                let path = percent_decode(rest);
                let path = path.strip_prefix("localhost").unwrap_or(&path).to_string();
                if cfg!(windows) {
                    PathBuf::from(path.trim_start_matches('/'))
                } else {
                    path.into()
                }
            }
            None => PathBuf::from(line),
        })
        .collect()
}

/// Files to convert from the clipboard. A pasted image is saved as
/// [`PASTED_IMAGE_NAME`] in a temporary directory that lives as long as the
/// returned guard.
pub fn inputs() -> Result<(Vec<PathBuf>, Option<tempfile::TempDir>)> {
    let mut clipboard = arboard::Clipboard::new().context("could not open the clipboard")?;

    if let Ok(files) = clipboard.get().file_list()
        && !files.is_empty()
    {
        return Ok((files, None));
    }
    if let Ok(text) = clipboard.get_text() {
        let paths = paths_in_text(&text);
        if !paths.is_empty() && paths.iter().all(|path| path.is_file()) {
            return Ok((paths, None));
        }
    }
    if let Ok(image) = clipboard.get_image() {
        let dir = tempfile::Builder::new().prefix("tgradish-clipboard-").tempdir()?;
        let path = dir.path().join(PASTED_IMAGE_NAME);
        let file = std::fs::File::create(&path)?;
        let mut encoder =
            png::Encoder::new(BufWriter::new(file), image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("could not save the pasted image")?;
        writer.write_image_data(&image.bytes).context("could not save the pasted image")?;
        writer.finish().context("could not save the pasted image")?;
        return Ok((vec![path], Some(dir)));
    }
    bail!("the clipboard has no files, file path or image")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_paths_and_uris() {
        let text = "# copied\nfile:///home/me/My%20Pig.mp4\n/tmp/cat.gif\n\n";
        let paths = paths_in_text(text);
        if cfg!(windows) {
            assert_eq!(paths[0], PathBuf::from("home/me/My Pig.mp4"));
        } else {
            assert_eq!(paths[0], PathBuf::from("/home/me/My Pig.mp4"));
        }
        assert_eq!(paths[1], PathBuf::from("/tmp/cat.gif"));
        assert_eq!(paths.len(), 2);
        assert_eq!(percent_decode("a%zzb%41"), "a%zzbA");
    }
}

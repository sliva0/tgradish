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

/// Path of a `file://` URI, without the `file://`. Handles
/// `file:///home/x`, `file:///C:/x`, `file://localhost/x` and, on Windows,
/// network shares like `file://server/share/x`.
fn uri_path(rest: &str, windows: bool) -> PathBuf {
    let path = percent_decode(rest);
    let (host, path) = match path.strip_prefix('/') {
        Some(_) => ("", path.as_str()),
        None => path.split_once('/').map_or((path.as_str(), ""), |(host, p)| (host, p)),
    };
    let host = if host.eq_ignore_ascii_case("localhost") { "" } else { host };
    match (windows, host) {
        // /C:/x is C:/x
        (true, "") => PathBuf::from(path.trim_start_matches('/')),
        (true, host) => PathBuf::from(format!(r"\\{host}\{}", path.replace('/', "\\"))),
        (false, "") => PathBuf::from(format!("/{}", path.trim_start_matches('/'))),
        // shares on other hosts are not reachable as paths here
        (false, host) => PathBuf::from(format!("//{host}/{path}")),
    }
}

/// Paths in copied text: plain paths, possibly in quotes as Windows'
/// "Copy as path" writes them, or `file://` URIs, one per line.
fn paths_in_text(text: &str, windows: bool) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let line = line
                .strip_prefix('"')
                .and_then(|l| l.strip_suffix('"'))
                .or_else(|| line.strip_prefix('\'').and_then(|l| l.strip_suffix('\'')))
                .unwrap_or(line);
            match line.strip_prefix("file://") {
                Some(rest) => uri_path(rest, windows),
                None => PathBuf::from(line),
            }
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
        let paths = paths_in_text(&text, cfg!(windows));
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
        let text = "# copied\nfile:///home/me/My%20Pig.mp4\n/tmp/cat.gif\n'/tmp/a b.mp4'\n\n";
        let paths = paths_in_text(text, false);
        assert_eq!(
            paths,
            [
                PathBuf::from("/home/me/My Pig.mp4"),
                PathBuf::from("/tmp/cat.gif"),
                PathBuf::from("/tmp/a b.mp4"),
            ]
        );
        assert_eq!(
            paths_in_text("file://localhost/tmp/x.mp4", false),
            [PathBuf::from("/tmp/x.mp4")]
        );
        assert_eq!(percent_decode("a%zzb%41"), "a%zzbA");
    }

    #[test]
    fn reads_windows_paths() {
        let text = "\"C:\\Users\\me\\pig.mp4\"\nfile:///C:/Users/me/My%20Pig.mp4\n\
                    file://server/share/pig.mp4";
        assert_eq!(
            paths_in_text(text, true),
            [
                PathBuf::from(r"C:\Users\me\pig.mp4"),
                PathBuf::from("C:/Users/me/My Pig.mp4"),
                PathBuf::from(r"\\server\share\pig.mp4"),
            ]
        );
    }
}

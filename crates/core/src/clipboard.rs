//! Inputs from the clipboard: copied files, copied paths, or an image.

use std::io::BufWriter;
use std::path::PathBuf;

use crate::{Error, Result};

/// Name a pasted image is saved under, so its result is
/// `clipboard.sticker.webm`.
pub const PASTED_IMAGE_NAME: &str = "clipboard.png";

/// What was on the clipboard.
pub struct Pasted {
    pub files: Vec<PathBuf>,
    /// The temporary directory of a pasted image, which is the one file;
    /// it is deleted when this is dropped.
    pub image_dir: Option<tempfile::TempDir>,
}

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
/// "Copy as path" writes them, or `file://` URIs, one per line. `windows`
/// says how to read URIs.
pub fn paths_in_text(text: &str, windows: bool) -> Vec<PathBuf> {
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

/// Files to convert from the clipboard: copied files, else copied paths of
/// files that exist, else an image, saved as [`PASTED_IMAGE_NAME`] in a
/// temporary directory.
pub fn paste() -> Result<Pasted> {
    let failed = |err: arboard::Error| Error::Clipboard(err.to_string());
    let mut clipboard = arboard::Clipboard::new().map_err(failed)?;

    if let Ok(files) = clipboard.get().file_list()
        && !files.is_empty()
    {
        return Ok(Pasted { files, image_dir: None });
    }
    if let Ok(text) = clipboard.get_text() {
        let files = paths_in_text(&text, cfg!(windows));
        if !files.is_empty() && files.iter().all(|path| path.is_file()) {
            return Ok(Pasted { files, image_dir: None });
        }
    }
    let Ok(image) = clipboard.get_image() else {
        return Err(Error::Clipboard("it has no files, file paths or image".into()));
    };
    let dir = tempfile::Builder::new().prefix("tgradish-clipboard-").tempdir()?;
    let path = dir.path().join(PASTED_IMAGE_NAME);
    let not_saved =
        |err: png::EncodingError| Error::Clipboard(format!("couldn't save the image: {err}"));
    let file = std::fs::File::create(&path)?;
    let mut encoder =
        png::Encoder::new(BufWriter::new(file), image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(not_saved)?;
    writer.write_image_data(&image.bytes).map_err(not_saved)?;
    writer.finish().map_err(not_saved)?;
    Ok(Pasted { files: vec![path], image_dir: Some(dir) })
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

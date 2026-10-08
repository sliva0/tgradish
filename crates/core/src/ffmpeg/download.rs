//! Installing the minimal ffmpeg build made by `cargo xtask ffmpeg` into the
//! data directory, for `tgradish ffmpeg download`.

use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{Ffmpeg, FfmpegSource, locate::downloaded_dir};
use crate::error::{Error, Result};

/// A published build of ffmpeg for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Build {
    pub version: String,
    /// Rust target triple the build runs on.
    pub target: String,
    pub url: String,
    /// SHA-256 of the archive, lowercase hex.
    pub sha256: String,
}

/// Builds published with this tgradish version, see docs/ffmpeg.md.
const PUBLISHED: &[(&str, &str, &str, &str)] = &[
    // (version, target, url, sha256)
];

/// Rust target triple of the published build that runs on this machine.
fn host_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-gnu"),
        _ => None,
    }
}

/// The published build for this machine, if there is one.
pub fn published_build() -> Option<Build> {
    let target = host_target()?;
    PUBLISHED.iter().find(|(_, t, _, _)| *t == target).map(|&(version, target, url, sha256)| {
        Build {
            version: version.into(),
            target: target.into(),
            url: url.into(),
            sha256: sha256.into(),
        }
    })
}

fn download_error(message: impl Into<String>) -> Error {
    Error::Download(message.into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Opens `url`, which may also be a `file://` URL or a local path. Returns
/// the reader and the size, if known.
fn open(url: &str) -> Result<(Box<dyn Read>, Option<u64>)> {
    let path = url.strip_prefix("file://").map(PathBuf::from).or_else(|| {
        (!url.starts_with("http://") && !url.starts_with("https://")).then(|| PathBuf::from(url))
    });
    if let Some(path) = path {
        let file = File::open(&path)?;
        let size = file.metadata()?.len();
        return Ok((Box::new(file), Some(size)));
    }
    let response = ureq::get(url).call().map_err(|err| download_error(format!("{url}: {err}")))?;
    let size = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok()?.parse().ok());
    Ok((Box::new(response.into_body().into_reader()), size))
}

/// Downloads `url` into `dest`, checking it against `sha256`.
fn fetch(
    url: &str,
    sha256: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<()> {
    let (mut reader, size) = open(url)?;
    let mut file = File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    let mut done = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        done += n as u64;
        on_progress(done, size);
    }
    file.sync_all()?;
    let actual = hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(sha256.trim()) {
        return Err(download_error(format!(
            "checksum mismatch for {url}: expected {sha256}, got {actual}"
        )));
    }
    Ok(())
}

/// Path inside the archive without its top directory, or `None` for
/// anything that could escape the destination.
fn entry_path(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    let Component::Normal(_) = components.next()? else { return None };
    let rest: PathBuf = components.collect();
    let safe = rest.components().all(|c| matches!(c, Component::Normal(_)));
    (safe && !rest.as_os_str().is_empty()).then_some(rest)
}

fn extract(archive: &Path, dest: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(BufReader::new(File::open(archive)?));
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let Some(path) = entry_path(&entry.path()?) else { continue };
        let target = dest.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }
    Ok(())
}

/// Downloads, verifies and installs an ffmpeg build into `dest`, replacing
/// what was there. Defaults to [`downloaded_dir`]. Reports progress as
/// (bytes done, total bytes).
pub fn install(
    url: &str,
    sha256: &str,
    dest: Option<&Path>,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Ffmpeg> {
    let dest = match dest {
        Some(dest) => dest.to_path_buf(),
        None => downloaded_dir().ok_or_else(|| download_error("no home directory found"))?,
    };
    let parent = dest.parent().ok_or_else(|| download_error("invalid destination"))?;
    std::fs::create_dir_all(parent)?;

    // everything happens next to the destination so the final rename is atomic
    let work = tempfile::Builder::new().prefix(".tgradish-ffmpeg-").tempdir_in(parent)?;
    let archive = work.path().join("ffmpeg.tar.gz");
    fetch(url, sha256, &archive, on_progress)?;
    let unpacked = work.path().join("ffmpeg");
    extract(&archive, &unpacked)?;

    let exe = |name: &str| unpacked.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    for name in ["ffmpeg", "ffprobe"] {
        if !exe(name).is_file() {
            return Err(download_error(format!("the archive has no {name}")));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(exe(name), std::fs::Permissions::from_mode(0o755))?;
        }
    }

    let old = work.path().join("old");
    if dest.exists() {
        std::fs::rename(&dest, &old)?;
    }
    if let Err(err) = std::fs::rename(&unpacked, &dest) {
        if old.exists() {
            let _ = std::fs::rename(&old, &dest);
        }
        return Err(err.into());
    }

    let in_dest = |name: &str| dest.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    Ok(Ffmpeg {
        ffmpeg: in_dest("ffmpeg"),
        ffprobe: in_dest("ffprobe"),
        source: FfmpegSource::Downloaded,
    })
}

/// Deletes the downloaded build. Returns whether there was one.
pub fn uninstall() -> Result<bool> {
    match downloaded_dir() {
        Some(dir) if dir.exists() => {
            std::fs::remove_dir_all(dir)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Archive laid out like the xtask output, plus an entry trying to escape.
    fn archive(dir: &Path) -> (PathBuf, String) {
        let path = dir.join("build.tar.gz");
        let file = File::create(&path).unwrap();
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(file, Default::default()));
        let exe = std::env::consts::EXE_SUFFIX;
        let files = [
            (format!("ffmpeg-x/ffmpeg{exe}"), "ffmpeg"),
            (format!("ffmpeg-x/ffprobe{exe}"), "ffprobe"),
            ("ffmpeg-x/licenses/zlib-LICENSE".to_string(), "zlib"),
        ];
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, data.as_bytes()).unwrap();
        }
        // tar::Builder refuses `..` in paths, so write the name by hand
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().unwrap().name[..18].copy_from_slice(b"ffmpeg-x/../escape");
        header.set_size(4);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append(&header, &b"evil"[..]).unwrap();
        tar.into_inner().unwrap().finish().unwrap();

        let sum = hex(&Sha256::digest(std::fs::read(&path).unwrap()));
        (path, sum)
    }

    #[test]
    fn installs_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let (archive, sum) = archive(dir.path());
        let dest = dir.path().join("data/ffmpeg");
        let url = archive.to_str().unwrap();

        let mut reported = 0;
        let ffmpeg = install(url, &sum, Some(&dest), &mut |done, _| reported = done).unwrap();
        assert_eq!(reported, std::fs::metadata(&archive).unwrap().len());
        assert_eq!(std::fs::read_to_string(&ffmpeg.ffmpeg).unwrap(), "ffmpeg");
        assert!(dest.join("licenses/zlib-LICENSE").is_file());
        assert!(!dir.path().join("data/escape").exists() && !dir.path().join("escape").exists());

        // installing again replaces the old build and leaves nothing behind
        let file_url = format!("file://{url}");
        install(&file_url, &sum.to_uppercase(), Some(&dest), &mut |_, _| {}).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("data")).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn rejects_wrong_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let (archive, _) = archive(dir.path());
        let dest = dir.path().join("ffmpeg");
        let result =
            install(archive.to_str().unwrap(), &"0".repeat(64), Some(&dest), &mut |_, _| {});
        assert!(matches!(result, Err(Error::Download(_))), "{result:?}");
        assert!(!dest.exists());
    }

    #[test]
    fn rejects_escaping_paths() {
        assert_eq!(entry_path(Path::new("top/ffmpeg")), Some(PathBuf::from("ffmpeg")));
        assert_eq!(entry_path(Path::new("top/../x")), None);
        assert_eq!(entry_path(Path::new("/abs/x")), None);
        assert_eq!(entry_path(Path::new("top")), None);
    }
}

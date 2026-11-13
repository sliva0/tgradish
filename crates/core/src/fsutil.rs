//! Writing results without leaving half-written files behind.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Where a result goes beside `input`: its name with `suffix` for an
/// extension, or for a folder, after its whole name, dots and all.
pub(crate) fn beside(input: &Path, suffix: &str) -> PathBuf {
    if !input.is_dir() {
        return input.with_extension(suffix);
    }
    let mut name = input.file_name().map_or_else(|| "frames".into(), |name| name.to_os_string());
    name.push(format!(".{suffix}"));
    input.with_file_name(name)
}

/// Writes `bytes` to `path` through a temporary file in the same directory,
/// so nobody sees a half-written file and hard links to the old file keep
/// their content. With `replace` an existing file is replaced; otherwise
/// this fails with [`std::io::ErrorKind::AlreadyExists`], even if the file
/// appeared while tgradish was working. Where `path` is a link to a file,
/// that file is the one written, and the link stays.
pub fn write_file(path: &Path, bytes: &[u8], replace: bool) -> std::io::Result<()> {
    // renaming over a link would put a file in its place, and leave what
    // it points to, as a config file kept in a dotfiles folder, behind; a
    // link to nothing is replaced, as there is nowhere to write through it
    let linked = match std::fs::symlink_metadata(path) {
        Ok(found) if found.is_symlink() => std::fs::canonicalize(path).ok(),
        _ => None,
    };
    let path = linked.as_deref().unwrap_or(path);
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut builder = tempfile::Builder::new();
    builder.prefix(".tgradish-");
    // temporary files are private; results get the permissions any new
    // file gets, which the umask decides
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
    let mut tmp = builder.tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    let result = if replace { tmp.persist(path) } else { tmp.persist_noclobber(path) };
    result.map(drop).map_err(|err| err.error)
}

/// The bytes of the file at `path` if it has at most `most` of them, or
/// `None`, read without reading more than that, however large it is.
pub(crate) fn read_at_most(path: &Path, most: u64) -> std::io::Result<Option<Vec<u8>>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > most {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    // it may have grown since
    std::io::Read::read_to_end(&mut std::io::Read::take(file, most + 1), &mut bytes)?;
    Ok((bytes.len() as u64 <= most).then_some(bytes))
}

/// [`write_file`] for a result, where an output already there is
/// [`Error::OutputExists`](crate::Error::OutputExists).
pub(crate) fn write_output(path: &Path, bytes: &[u8], replace: bool) -> crate::Result<()> {
    write_file(path, bytes, replace).map_err(|err| match err.kind() {
        std::io::ErrorKind::AlreadyExists => crate::Error::OutputExists(path.to_path_buf()),
        _ => crate::Error::io(crate::FileAction::Write, path)(err),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_results_beside_files_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("walk.v2");
        std::fs::create_dir(&folder).unwrap();
        // a folder's whole name, a file's without its extension
        assert_eq!(beside(&folder, "sticker.webm"), dir.path().join("walk.v2.sticker.webm"));
        let file = dir.path().join("pig.v2.mp4");
        assert_eq!(beside(&file, "sticker.webm"), dir.path().join("pig.v2.sticker.webm"));
    }

    #[test]
    fn replaces_only_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.webm");
        write_file(&path, b"one", false).unwrap();
        let err = write_file(&path, b"two", false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        write_file(&path, b"three", true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"three");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn gives_results_normal_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.webm");
        write_file(&path, b"one", false).unwrap();
        // whatever the umask leaves, others can read it unless it says not
        // to: compare with a file created the ordinary way
        let plain = dir.path().join("plain");
        std::fs::write(&plain, b"x").unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), mode(&plain));
    }

    #[cfg(unix)]
    #[test]
    fn writes_what_a_link_points_to() {
        let dir = tempfile::tempdir().unwrap();
        let (kept, used) = (dir.path().join("dotfiles"), dir.path().join("config"));
        std::fs::create_dir(&kept).unwrap();
        std::fs::create_dir(&used).unwrap();
        let (file, link) = (kept.join("config.toml"), used.join("config.toml"));
        std::fs::write(&file, b"old").unwrap();
        // a link relative to its folder, as stow makes them
        std::os::unix::fs::symlink("../dotfiles/config.toml", &link).unwrap();
        write_file(&link, b"new", true).unwrap();
        assert!(link.symlink_metadata().unwrap().is_symlink());
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        // nor is it replaced unasked
        let err = write_file(&link, b"newer", false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&link).unwrap(), b"new");
        // nothing left beside either
        for folder in [&kept, &used] {
            assert_eq!(std::fs::read_dir(folder).unwrap().count(), 1);
        }
        // a link to nothing gives way to the file
        let dangling = used.join("gone.toml");
        std::os::unix::fs::symlink("nowhere/gone.toml", &dangling).unwrap();
        write_file(&dangling, b"here", true).unwrap();
        assert!(dangling.symlink_metadata().unwrap().is_file());
    }

    #[cfg(unix)]
    #[test]
    fn keeps_hard_linked_files_intact() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        let output = dir.path().join("output.webm");
        std::fs::write(&input, b"video").unwrap();
        std::fs::hard_link(&input, &output).unwrap();
        write_file(&output, b"sticker", true).unwrap();
        assert_eq!(std::fs::read(&input).unwrap(), b"video");
    }
}

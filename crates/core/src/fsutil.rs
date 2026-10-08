use std::io::Write;
use std::path::Path;

/// Writes `bytes` to `path` through a temporary file in the same directory,
/// so nobody sees a half-written file and hard links to the old file keep
/// their content. With `replace` an existing file is replaced; otherwise
/// this fails with [`std::io::ErrorKind::AlreadyExists`], even if the file
/// appeared while tgradish was working.
pub fn write_file(path: &Path, bytes: &[u8], replace: bool) -> std::io::Result<()> {
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut tmp = tempfile::Builder::new().prefix(".tgradish-").tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    let result = if replace { tmp.persist(path) } else { tmp.persist_noclobber(path) };
    result.map(drop).map_err(|err| err.error)
}

#[cfg(test)]
mod tests {
    use super::*;

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

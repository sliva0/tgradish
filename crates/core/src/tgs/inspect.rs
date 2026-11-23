//! Reads `.tgs` (or plain Lottie JSON) files and validates them against
//! Telegram's rules.

use std::path::Path;

use tgradish_tgs::check::Stats;

use crate::art;
use crate::error::{Error, Result};
use crate::telegram::Issue;

/// Reads and unpacks a `.tgs` file (or reads plain Lottie JSON), returning the
/// JSON bytes and the compressed `.tgs` byte length when gzip-packed.
pub(super) fn read_lottie(path: &Path) -> Result<(Vec<u8>, Option<usize>)> {
    let bytes = art::read(path, art::MAX_STICKER_BYTES)?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let json = tgradish_tgs::file::unpack(&bytes).map_err(|err| Error::Unreadable {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;
        Ok((json, Some(bytes.len())))
    } else {
        Ok((bytes, None))
    }
}

/// Reads the Lottie JSON bytes of a `.tgs` file (used by tests inspecting
/// marks).
#[cfg(test)]
pub(crate) fn read_json(path: &Path) -> Result<Vec<u8>> {
    read_lottie(path).map(|(json, _)| json)
}

/// Inspects a `.tgs` or plain Lottie JSON file against Telegram's rules and
/// renderer limits.
pub fn inspect_file(path: &Path) -> Result<(Stats, Vec<Issue>)> {
    let (json, packed) = read_lottie(path)?;
    let (stats, found) = tgradish_tgs::check::check(&json, packed)
        .map_err(|err| Error::Unreadable { path: path.to_path_buf(), message: err.to_string() })?;
    let issues = crate::telegram::check_lottie(found, &stats);
    Ok((stats, issues))
}

//! Reading a `.tgs` and checking it against Telegram's rules.

use std::path::Path;

use tgradish_tgs::check::Stats;

use crate::art;
use crate::error::{Error, Result};
use crate::telegram::Issue;

/// The Lottie JSON of a `.tgs` (or plain Lottie JSON) file, and the
/// file's size when it was packed.
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

/// The Lottie JSON of a `.tgs` (or plain Lottie JSON) file, which the
/// tests read [`crate::mark`]'s bytes from.
#[cfg(test)]
pub(crate) fn read_json(path: &Path) -> Result<Vec<u8>> {
    read_lottie(path).map(|(json, _)| json)
}

/// Reads a `.tgs` (or plain Lottie JSON) and checks it against Telegram's
/// rules and renderers.
pub fn inspect_file(path: &Path) -> Result<(Stats, Vec<Issue>)> {
    let (json, packed) = read_lottie(path)?;
    let (stats, found) = tgradish_tgs::check::check(&json, packed)
        .map_err(|err| Error::Unreadable { path: path.to_path_buf(), message: err.to_string() })?;
    let issues = crate::telegram::check_lottie(found, &stats);
    Ok((stats, issues))
}

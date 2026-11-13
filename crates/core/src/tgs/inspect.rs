//! Reading a `.tgs` and checking it against Telegram's rules.

use std::path::Path;

pub use tgradish_tgs::check::{Issue, Stats};

use crate::art;
use crate::error::{Error, Result};

/// The Lottie JSON of a `.tgs` (or plain Lottie JSON) file, and the
/// file's size when it was packed.
fn read_lottie(path: &Path) -> Result<(Vec<u8>, Option<usize>)> {
    let bytes = art::read(path)?;
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
/// tests read marks from.
#[cfg(test)]
pub(crate) fn read_json(path: &Path) -> Result<Vec<u8>> {
    read_lottie(path).map(|(json, _)| json)
}

/// Reads a `.tgs` (or plain Lottie JSON) and checks it against Telegram's
/// rules and renderers.
pub fn inspect_file(path: &Path) -> Result<(Stats, Vec<Issue>)> {
    let (json, packed) = read_lottie(path)?;
    tgradish_tgs::check::check(&json, packed)
        .map_err(|err| Error::Unreadable { path: path.to_path_buf(), message: err.to_string() })
}

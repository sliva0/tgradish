//! `.tgs` files: gzipped Lottie JSON.

use std::io::Read;
use std::num::NonZeroU64;

use thiserror::Error;

/// Lottie JSON larger than this is refused when reading: Telegram's gzip
/// unpacker allows 5 MiB, so nothing larger works anywhere.
pub const MAX_UNPACKED: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReadError {
    #[error("not gzip data: {0}")]
    Gzip(String),
    #[error("unpacks to more than {} MiB", MAX_UNPACKED >> 20)]
    TooLarge,
}

/// Gzip with zopfli, which beats `gzip -9` by 8-15% on stickers.
/// `iterations` trades time for size: 15 is zopfli's default.
pub fn pack(json: &[u8], iterations: u64) -> Vec<u8> {
    let options = zopfli::Options {
        iteration_count: NonZeroU64::new(iterations.max(1)).unwrap(),
        ..zopfli::Options::default()
    };
    let mut out = Vec::new();
    zopfli::compress(options, zopfli::Format::Gzip, json, &mut out)
        .expect("compressing into memory can't fail");
    out
}

/// The Lottie JSON in a `.tgs`.
pub fn unpack(tgs: &[u8]) -> Result<Vec<u8>, ReadError> {
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(tgs)
        .take(MAX_UNPACKED as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|err| ReadError::Gzip(err.to_string()))?;
    if json.len() > MAX_UNPACKED {
        return Err(ReadError::TooLarge);
    }
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let json = br#"{"v":"5.7.2","layers":[]}"#.repeat(50);
        let tgs = pack(&json, 5);
        assert!(tgs.starts_with(&[0x1f, 0x8b]) && tgs.len() < json.len() / 5);
        assert_eq!(unpack(&tgs).unwrap(), json);
        assert!(matches!(unpack(b"{}"), Err(ReadError::Gzip(_))));
    }
}

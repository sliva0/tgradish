//! `.tgs` container: gzipped Lottie JSON.

use std::io::Read;
use std::num::NonZeroU64;

use thiserror::Error;

/// Maximum unpacked Lottie JSON size. Telegram's gzip unpacker stops at 5 MiB
/// unpacked, so nothing larger works anywhere.
pub const MAX_UNPACKED: usize = 5 * 1024 * 1024;

/// Error returned when a `.tgs` file cannot be unpacked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ReadError {
    /// Invalid gzip stream reported by `flate2`.
    #[error("invalid gzip data: {0}")]
    Gzip(String),
    /// Unpacked JSON exceeds [`MAX_UNPACKED`].
    #[error("unpacked JSON exceeds {} MiB", MAX_UNPACKED >> 20)]
    TooLarge,
}

internal! {
/// Compresses `json` with zopfli, which beats `gzip -9` by 8–15% on stickers.
/// `iterations` trades speed for smaller output (15 is zopfli's default).
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
}

internal! {
/// Compresses `json` with `gzip -9`. Much faster than [`pack`] and slightly
/// larger.
pub fn pack_quick(json: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(json).expect("compressing into memory can't fail");
    encoder.finish().expect("compressing into memory can't fail")
}
}

/// Estimates the compressed size of `json` using `gzip -9`. Much faster than
/// [`pack`] and close enough to zopfli's ordering for comparing candidates;
/// the exact zopfli saving varies with the art (see [`crate::sticker`]).
#[cfg(feature = "internals")]
pub fn quick_size(json: &[u8]) -> usize {
    pack_quick(json).len()
}

/// Unpacks the Lottie JSON from a gzipped `.tgs` buffer.
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

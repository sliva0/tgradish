//! Reading bytes: little-endian reads that stop at the end, inflating, and
//! keeping what a file costs within the limits.

use std::io::Read;

use crate::{Error, Format, Limits, Oversize, Result};

pub(super) fn error(message: impl ToString) -> Error {
    Error::Decode { format: Format::Aseprite, message: message.to_string() }
}

pub(super) fn truncated() -> Error {
    error("the file ends too early")
}

/// Little-endian reads that fail instead of running past the end.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    pub(super) fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len());
        let end = end.ok_or_else(truncated)?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    pub(super) fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(super) fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub(super) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(super) fn skip(&mut self, count: usize) -> Result<()> {
        self.take(count).map(drop)
    }

    pub(super) fn string(&mut self) -> Result<String> {
        let length = self.u16()?;
        Ok(String::from_utf8_lossy(self.take(length.into())?).into_owned())
    }

    pub(super) fn rest(&mut self) -> &'a [u8] {
        let out = &self.bytes[self.at..];
        self.at = self.bytes.len();
        out
    }
}

/// Inflates zlib data that must come out exactly `size` bytes long.
pub(super) fn inflate(data: &[u8], size: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take(size as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|err| error(format!("compressed data is broken: {err}")))?;
    if out.len() != size {
        return Err(error(format!(
            "compressed data has {} bytes, {size} were expected",
            out.len()
        )));
    }
    Ok(out)
}

pub(super) fn too_large(limits: &Limits) -> Error {
    limits.bytes(Oversize::Decoding)
}

/// Bytes for the product of `factors`, within the limits.
pub(super) fn budget(factors: [usize; 3], limits: &Limits) -> Result<usize> {
    factors
        .iter()
        .try_fold(1usize, |total, &n| total.checked_mul(n))
        .filter(|&bytes| bytes <= limits.max_bytes)
        .ok_or_else(|| too_large(limits))
}

/// Adds `bytes` to what decoding has used so far, within the limits.
pub(super) fn spend(spent: &mut usize, bytes: usize, limits: &Limits) -> Result<usize> {
    *spent = spent
        .checked_add(bytes)
        .filter(|&total| total <= limits.max_bytes)
        .ok_or_else(|| too_large(limits))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use crate::aseprite::testfile::*;
    use crate::aseprite::{Layers, Sprite};
    use crate::{Error, Limits};

    #[test]
    fn keeps_within_limits() {
        let big = file(5000, 5000, 32, 1, &[vec![layer(1, 0, 0, 0, 255)]]);
        let small = Limits { max_dimension: 4096, max_bytes: 1 << 20 };
        assert!(matches!(Sprite::read(&big, &small), Err(Error::TooLarge { .. })));
        let roomy = Limits { max_dimension: 8192, max_bytes: 1 << 20 };
        // the canvas is fine, 100 MB of frames aren't
        let sprite = Sprite::read(&big, &roomy).unwrap();
        assert!(matches!(sprite.animation(None, &Layers::default()), Err(Error::TooLarge { .. })));
        // nor is a cel that claims to be huge
        let cel = file(
            1,
            1,
            32,
            1,
            &[vec![layer(1, 0, 0, 0, 255), image_cel(0, (0, 0), (60000, 60000), &RED, false)]],
        );
        assert!(matches!(Sprite::read(&cel, &roomy), Err(Error::TooLarge { .. })));
    }

    #[test]
    fn counts_the_sprite_beside_its_frames() {
        // a 4x4 cel of 64 bytes on a 4x4 canvas: drawing takes the frame
        // and a canvas, 128 bytes, while the sprite still holds the cel
        let cel = image_cel(0, (0, 0), (4, 4), &[RED; 16].concat(), false);
        let bytes = file(4, 4, 32, 1, &[vec![layer(1, 0, 0, 0, 255), cel]]);
        let limits = |max_bytes| Limits { max_dimension: 8192, max_bytes };
        let draw = |max_bytes| {
            Sprite::read(&bytes, &limits(max_bytes))?.animation(None, &Layers::default())
        };
        assert!(matches!(Sprite::read(&bytes, &limits(63)), Err(Error::TooLarge { .. })));
        assert!(matches!(draw(191), Err(Error::TooLarge { .. })));
        assert_eq!(draw(192).unwrap().frames().len(), 1);

        // 100 tiles of a byte each in the file are 400 bytes as they are
        // kept, with the file's 100 beside them while they are read, and
        // the tileset's 12 bytes of pixels
        let tiles =
            file(1, 1, 32, 1, &[vec![layer(1, 2, 0, 0, 255), tileset(), tilemap(8, &[1; 100])]]);
        assert!(matches!(Sprite::read(&tiles, &limits(511)), Err(Error::TooLarge { .. })));
        let sprite = Sprite::read(&tiles, &limits(512)).unwrap();
        assert_eq!(sprite.spent, 412);
    }
}

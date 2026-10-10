//! Minimal EBML reader ([RFC 8794]), just enough to inspect WebM files and patch
//! their duration.
//!
//! Element IDs are kept in their encoded form, marker bit included (for example
//! Segment is `0x18538067`), so they can be compared with the values listed in
//! the Matroska specification as is.
//!
//! [RFC 8794]: https://www.rfc-editor.org/rfc/rfc8794.html

use thiserror::Error;

/// Element IDs used by tgradish, from the Matroska specification:
/// <https://github.com/ietf-wg-cellar/matroska-specification/blob/master/ebml_matroska.xml>
pub mod ids {
    pub const EBML: u32 = 0x1A45_DFA3;
    pub const DOC_TYPE: u32 = 0x4282;

    pub const VOID: u32 = 0xEC;
    pub const CRC32: u32 = 0xBF;

    pub const SEGMENT: u32 = 0x1853_8067;

    pub const SEEK_HEAD: u32 = 0x114D_9B74;
    pub const SEEK: u32 = 0x4DBB;
    pub const SEEK_ID: u32 = 0x53AB;
    pub const SEEK_POSITION: u32 = 0x53AC;

    pub const INFO: u32 = 0x1549_A966;
    pub const TRACKS: u32 = 0x1654_AE6B;
    pub const CLUSTER: u32 = 0x1F43_B675;
    pub const CUES: u32 = 0x1C53_BB6B;
    pub const CHAPTERS: u32 = 0x1043_A770;
    pub const TAGS: u32 = 0x1254_C367;
    pub const ATTACHMENTS: u32 = 0x1941_A469;

    pub const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
    pub const DURATION: u32 = 0x4489;
    pub const TITLE: u32 = 0x7BA9;
    pub const MUXING_APP: u32 = 0x4D80;
    pub const WRITING_APP: u32 = 0x5741;

    pub const TRACK_ENTRY: u32 = 0xAE;
    pub const TRACK_NUMBER: u32 = 0xD7;
    pub const TRACK_UID: u32 = 0x73C5;
    pub const TRACK_TYPE: u32 = 0x83;
    pub const CODEC_ID: u32 = 0x86;
    pub const DEFAULT_DURATION: u32 = 0x23_E383;
    pub const VIDEO: u32 = 0xE0;
    pub const PIXEL_WIDTH: u32 = 0xB0;
    pub const PIXEL_HEIGHT: u32 = 0xBA;
    pub const ALPHA_MODE: u32 = 0x53C0;

    pub const TIMESTAMP: u32 = 0xE7;
    pub const SIMPLE_BLOCK: u32 = 0xA3;
    pub const BLOCK_GROUP: u32 = 0xA0;
    pub const BLOCK: u32 = 0xA1;
    pub const BLOCK_DURATION: u32 = 0x9B;

    pub const TAG: u32 = 0x7373;
    pub const SIMPLE_TAG: u32 = 0x67C8;
    pub const TARGETS: u32 = 0x63C0;
    pub const TAG_TRACK_UID: u32 = 0x63C5;
    pub const TAG_NAME: u32 = 0x45A3;
    pub const TAG_STRING: u32 = 0x4487;

    /// Elements that can appear directly in a Segment. An unknown-size Cluster
    /// ends where one of these starts.
    pub const SEGMENT_CHILDREN: [u32; 8] =
        [SEEK_HEAD, INFO, TRACKS, CLUSTER, CUES, CHAPTERS, TAGS, ATTACHMENTS];
}

/// Longest element ID allowed by the default `EBMLMaxIDLength`.
const MAX_ID_LEN: usize = 4;
/// Longest data size allowed by the default `EBMLMaxSizeLength`.
const MAX_SIZE_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EbmlError {
    #[error("unexpected end of data at byte {0}")]
    Truncated(usize),
    #[error("invalid variable-size integer at byte {0}")]
    InvalidVint(usize),
    #[error("invalid {size}-byte {kind} value at byte {offset}")]
    InvalidValue { kind: &'static str, size: usize, offset: usize },
}

pub type Result<T> = std::result::Result<T, EbmlError>;

/// Element header with the location of the element in the parsed buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element {
    pub id: u32,
    /// Offset of the first byte of the element ID.
    pub start: usize,
    /// Offset of the first data byte.
    pub data_start: usize,
    /// Offset one past the last data byte. Elements of unknown size and
    /// elements that claim to be longer than their parent end where the
    /// parent ends.
    pub end: usize,
    pub unknown_size: bool,
    /// The declared size did not fit in the parent, `end` was clamped.
    pub truncated: bool,
}

impl Element {
    pub fn data<'a>(&self, buf: &'a [u8]) -> &'a [u8] {
        &buf[self.data_start..self.end]
    }

    pub fn children<'a>(&self, buf: &'a [u8]) -> Children<'a> {
        Children::new(buf, self.data_start, self.end)
    }
}

/// Number of bytes in a VINT, taken from its first byte.
fn vint_len(buf: &[u8], pos: usize, max: usize) -> Result<usize> {
    let first = *buf.get(pos).ok_or(EbmlError::Truncated(pos))?;
    let len = first.leading_zeros() as usize + 1;
    if first == 0 || len > max {
        return Err(EbmlError::InvalidVint(pos));
    }
    if pos + len > buf.len() {
        return Err(EbmlError::Truncated(buf.len()));
    }
    Ok(len)
}

/// Reads an element ID, keeping the marker bit. Returns the ID and its length.
pub fn read_id(buf: &[u8], pos: usize) -> Result<(u32, usize)> {
    let len = vint_len(buf, pos, MAX_ID_LEN)?;
    let id = buf[pos..pos + len].iter().fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
    Ok((id, len))
}

/// Reads a VINT with the marker bit removed. Returns the value and its length.
pub fn read_vint(buf: &[u8], pos: usize) -> Result<(u64, usize)> {
    let len = vint_len(buf, pos, MAX_SIZE_LEN)?;
    let marker_mask = 0xFFu64 >> len;
    let value = buf[pos + 1..pos + len]
        .iter()
        .fold(u64::from(buf[pos]) & marker_mask, |acc, &b| (acc << 8) | u64::from(b));
    Ok((value, len))
}

/// Reads an element data size. `None` means the size is unknown.
pub fn read_size(buf: &[u8], pos: usize) -> Result<(Option<u64>, usize)> {
    let (value, len) = read_vint(buf, pos)?;
    let all_ones = (1u64 << (7 * len)) - 1;
    Ok(((value != all_ones).then_some(value), len))
}

/// Reads the element header at `pos`, inside a parent ending at `parent_end`.
pub fn read_header(buf: &[u8], pos: usize, parent_end: usize) -> Result<Element> {
    let buf = &buf[..parent_end];
    let (id, id_len) = read_id(buf, pos)?;
    let (size, size_len) = read_size(buf, pos + id_len)?;
    let data_start = pos + id_len + size_len;

    let declared_end =
        size.map(|size| usize::try_from(size).ok().and_then(|size| data_start.checked_add(size)));
    let (end, truncated) = match declared_end {
        Some(Some(end)) if end <= parent_end => (end, false),
        Some(_) => (parent_end, true),
        None => (parent_end, false),
    };

    Ok(Element { id, start: pos, data_start, end, unknown_size: size.is_none(), truncated })
}

/// Iterator over the elements in `buf[start..end]`. Stops after the first error.
pub struct Children<'a> {
    buf: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Children<'a> {
    pub fn new(buf: &'a [u8], start: usize, end: usize) -> Self {
        Self { buf, pos: start, end }
    }

    /// Continues iteration from `pos`, used after manually parsing an element
    /// of unknown size.
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Returns the first child with the given ID.
    pub fn find_id(mut self, id: u32) -> Result<Option<Element>> {
        self.find_map(|el| match el {
            Ok(el) if el.id == id => Some(Ok(el)),
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        })
        .transpose()
    }
}

impl Iterator for Children<'_> {
    type Item = Result<Element>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        match read_header(self.buf, self.pos, self.end) {
            Ok(el) => {
                self.pos = el.end;
                Some(Ok(el))
            }
            Err(err) => {
                self.pos = self.end;
                Some(Err(err))
            }
        }
    }
}

pub fn read_uint(buf: &[u8], el: &Element) -> Result<u64> {
    let data = el.data(buf);
    if data.len() > 8 {
        return Err(EbmlError::InvalidValue {
            kind: "unsigned integer",
            size: data.len(),
            offset: el.data_start,
        });
    }
    Ok(data.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b)))
}

pub fn read_float(buf: &[u8], el: &Element) -> Result<f64> {
    let data = el.data(buf);
    match data.len() {
        0 => Ok(0.0),
        4 => Ok(f64::from(f32::from_be_bytes(data.try_into().unwrap()))),
        8 => Ok(f64::from_be_bytes(data.try_into().unwrap())),
        size => Err(EbmlError::InvalidValue { kind: "float", size, offset: el.data_start }),
    }
}

pub fn read_string(buf: &[u8], el: &Element) -> String {
    let data = el.data(buf);
    let data = data.split(|&b| b == 0).next().unwrap_or_default();
    String::from_utf8_lossy(data).into_owned()
}

pub fn encode_id(id: u32) -> Vec<u8> {
    let bytes = id.to_be_bytes();
    let skip = bytes.iter().take_while(|&&b| b == 0).count();
    bytes[skip..].to_vec()
}

/// Shortest VINT length that can hold `size` as a known size.
fn min_size_len(size: u64) -> Option<usize> {
    (1..=MAX_SIZE_LEN).find(|&len| size < (1u64 << (7 * len)) - 1)
}

/// Encodes a data size as a VINT at least `min_len` bytes long. EBML allows
/// sizes to use more bytes than necessary, which is used to absorb padding.
pub fn encode_size(size: u64, min_len: usize) -> Option<Vec<u8>> {
    let len = min_size_len(size)?.max(min_len);
    if len > MAX_SIZE_LEN {
        return None;
    }
    let value = size | (1u64 << (7 * len));
    Some(value.to_be_bytes()[8 - len..].to_vec())
}

/// Encodes a whole element, with the size VINT at least `min_size_len` bytes.
pub fn encode_element(id: u32, data: &[u8], min_size_len: usize) -> Option<Vec<u8>> {
    let mut out = encode_id(id);
    out.extend(encode_size(data.len() as u64, min_size_len)?);
    out.extend_from_slice(data);
    Some(out)
}

pub fn encode_uint(id: u32, value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let skip = bytes.iter().take_while(|&&b| b == 0).count().min(7);
    encode_element(id, &bytes[skip..], 1).expect("8 bytes always fit")
}

pub fn encode_string(id: u32, value: &str) -> Option<Vec<u8>> {
    encode_element(id, value.as_bytes(), 1)
}

/// Encodes a Void element exactly `total_len` bytes long, with `payload` at
/// the start of its data and zeros after it. Returns `None` if that is
/// impossible: a Void needs at least 2 bytes, and `payload` must fit.
pub fn encode_void(total_len: usize, payload: &[u8]) -> Option<Vec<u8>> {
    // 1 byte of ID, then the shortest size VINT that leaves a valid size
    let (size_len, data_len) = (1..=MAX_SIZE_LEN).find_map(|size_len| {
        let data_len = total_len.checked_sub(1 + size_len)?;
        (min_size_len(data_len as u64)? <= size_len).then_some((size_len, data_len))
    })?;
    if payload.len() > data_len {
        return None;
    }
    let mut data = payload.to_vec();
    data.resize(data_len, 0);
    let out = encode_element(ids::VOID, &data, size_len)?;
    debug_assert_eq!(out.len(), total_len);
    Some(out)
}

/// Helpers for building EBML data in tests.
#[cfg(test)]
pub(crate) mod build {
    use super::*;

    pub fn el(id: u32, data: &[u8]) -> Vec<u8> {
        encode_element(id, data, 1).unwrap()
    }

    /// Encodes an element header with unknown size, followed by `data`.
    pub fn el_unknown_size(id: u32, data: &[u8]) -> Vec<u8> {
        let mut out = encode_id(id);
        out.push(0xFF);
        out.extend_from_slice(data);
        out
    }

    pub fn uint(id: u32, value: u64) -> Vec<u8> {
        encode_uint(id, value)
    }

    pub fn f64_el(id: u32, value: f64) -> Vec<u8> {
        el(id, &value.to_be_bytes())
    }

    pub fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    #[test]
    fn reads_ids_with_marker() {
        assert_eq!(read_id(&[0x1A, 0x45, 0xDF, 0xA3], 0), Ok((ids::EBML, 4)));
        assert_eq!(read_id(&[0x44, 0x89], 0), Ok((ids::DURATION, 2)));
        assert_eq!(read_id(&[0xA3], 0), Ok((ids::SIMPLE_BLOCK, 1)));
    }

    #[test]
    fn reads_vints() {
        assert_eq!(read_vint(&[0x81], 0), Ok((1, 1)));
        assert_eq!(read_vint(&[0x40, 0x02], 0), Ok((2, 2)));
        assert_eq!(read_vint(&[0x01, 0, 0, 0, 0, 0, 0x01, 0x00], 0), Ok((256, 8)));
        assert_eq!(read_size(&[0xFF], 0), Ok((None, 1)));
        assert_eq!(read_size(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], 0), Ok((None, 8)));
        assert_eq!(read_size(&[0x7F, 0xFF], 0), Ok((None, 2)));
        assert_eq!(read_size(&[0x7F, 0xFE], 0), Ok((Some(0x3FFE), 2)));
    }

    #[test]
    fn rejects_bad_vints() {
        assert_eq!(read_vint(&[0x00, 0x81], 0), Err(EbmlError::InvalidVint(0)));
        assert_eq!(read_id(&[0x08, 0, 0, 0, 0], 0), Err(EbmlError::InvalidVint(0)));
        assert_eq!(read_vint(&[0x40], 0), Err(EbmlError::Truncated(1)));
        assert_eq!(read_vint(&[], 0), Err(EbmlError::Truncated(0)));
    }

    #[test]
    fn iterates_children() {
        let data = concat(&[uint(ids::TRACK_NUMBER, 1), el(ids::CODEC_ID, b"V_VP9\0")]);
        let buf = el(ids::TRACK_ENTRY, &data);
        let parent = read_header(&buf, 0, buf.len()).unwrap();
        let children: Vec<_> = parent.children(&buf).collect::<Result<_>>().unwrap();

        assert_eq!(children.len(), 2);
        assert_eq!(read_uint(&buf, &children[0]), Ok(1));
        assert_eq!(read_string(&buf, &children[1]), "V_VP9");
    }

    #[test]
    fn clamps_oversized_elements() {
        let mut buf = el(ids::INFO, &[0; 10]);
        buf.truncate(6);
        let el = read_header(&buf, 0, buf.len()).unwrap();
        assert!(el.truncated);
        assert_eq!(el.end, 6);
    }

    #[test]
    fn unknown_size_extends_to_parent_end() {
        let buf = el_unknown_size(ids::SEGMENT, &[0xEC, 0x80]);
        let el = read_header(&buf, 0, buf.len()).unwrap();
        assert!(el.unknown_size);
        assert_eq!((el.data_start, el.end), (5, 7));
    }

    #[test]
    fn reads_floats() {
        let buf = el(ids::DURATION, &1.5f32.to_be_bytes());
        let el4 = read_header(&buf, 0, buf.len()).unwrap();
        assert_eq!(read_float(&buf, &el4), Ok(1.5));

        let buf = f64_el(ids::DURATION, 420.69);
        let el8 = read_header(&buf, 0, buf.len()).unwrap();
        assert_eq!(read_float(&buf, &el8), Ok(420.69));

        let buf = el(ids::DURATION, &[0; 3]);
        let el3 = read_header(&buf, 0, buf.len()).unwrap();
        assert!(read_float(&buf, &el3).is_err());
    }

    #[test]
    fn encodes_sizes() {
        assert_eq!(encode_size(0, 1), Some(vec![0x80]));
        assert_eq!(encode_size(126, 1), Some(vec![0xFE]));
        // 127 with one byte would be the "unknown size" marker
        assert_eq!(encode_size(127, 1), Some(vec![0x40, 0x7F]));
        assert_eq!(encode_size(5, 3), Some(vec![0x20, 0x00, 0x05]));
        assert_eq!(encode_size(5, 9), None);
    }

    #[test]
    fn encodes_voids_of_exact_length() {
        assert_eq!(encode_void(0, b""), None);
        assert_eq!(encode_void(1, b""), None);
        assert_eq!(encode_void(2, b""), Some(vec![0xEC, 0x80]));
        assert_eq!(encode_void(4, b"hi"), Some(vec![0xEC, 0x82, b'h', b'i']));
        assert_eq!(encode_void(4, b"hi!"), None);
        for len in 2..400 {
            let void = encode_void(len, b"tgradish").or_else(|| encode_void(len, b"")).unwrap();
            assert_eq!(void.len(), len);
            let el = read_header(&void, 0, void.len()).unwrap();
            assert_eq!((el.id, el.end, el.truncated), (ids::VOID, len, false));
        }
    }
}

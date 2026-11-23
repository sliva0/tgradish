//! Internal 8-byte mark embedded in watermarked stickers: tgradish version,
//! client front-end, 24-bit username hash and a CRC-8 check byte, XORed with a
//! fixed key. Disabled when watermarking is off.
//!
//! Stored in the video track UID for WebM files, and in rectangle ordering (or
//! the first layer's name when there are too few rectangles) for `.tgs` files
//! (see `tgradish_tgs::mark`).

use std::sync::OnceLock;

/// Front-end that produced a sticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    /// Library call or unspecified front-end.
    Library,
    Cli,
    Gui,
}

impl Client {
    /// Code stored in marks. Never reuse a code: 3 was a bot's, which no
    /// longer exists.
    fn code(self) -> u8 {
        match self {
            Client::Library => 0,
            Client::Cli => 1,
            Client::Gui => 2,
        }
    }

    fn from_code(code: u8) -> Option<Client> {
        Some(match code {
            0 => Client::Library,
            1 => Client::Cli,
            2 => Client::Gui,
            _ => return None,
        })
    }
}

static CLIENT: OnceLock<Client> = OnceLock::new();

/// Sets the front-end recorded in marks for this process (defaults to
/// [`Client::Library`]). Only the first call takes effect.
pub fn set_client(client: Client) {
    let _ = CLIENT.set(client);
}

/// Decoded contents of an 8-byte mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    /// tgradish version `[major, minor, patch]`.
    pub version: [u8; 3],
    pub client: Client,
    /// 24-bit FNV-1a hash of the username, or 0 if unavailable.
    pub user: u32,
}

/// Format version stored in the high nibble of byte 0.
const FORMAT: u8 = 1;
/// Fixed XOR key applied to mark bytes (so the same values give the same bytes
/// in every sticker).
const KEY: [u8; 8] = [0x9e, 0x37, 0x79, 0xb9, 0x7f, 0x4a, 0x7c, 0x15];

impl Mark {
    /// Builds the mark for the current process.
    pub fn current() -> Mark {
        Mark {
            version: version_numbers(env!("CARGO_PKG_VERSION")),
            client: CLIENT.get().copied().unwrap_or(Client::Library),
            user: user_hash(),
        }
    }

    /// Returns [`Mark::current`] when `watermark` is true, or `None`, so that
    /// a sticker made with the watermark off carries nothing of the user's or
    /// tgradish's.
    pub fn written(watermark: bool) -> Option<Mark> {
        watermark.then(Mark::current)
    }

    pub fn to_bytes(self) -> [u8; 8] {
        let user = self.user.to_be_bytes();
        let mut bytes = [
            FORMAT << 4 | self.client.code(),
            self.version[0],
            self.version[1],
            self.version[2],
            user[1],
            user[2],
            user[3],
            0,
        ];
        bytes[7] = check(&bytes[..7]);
        for (byte, key) in bytes.iter_mut().zip(KEY) {
            *byte ^= key;
        }
        bytes
    }

    /// The mark in `bytes`, if they are one.
    pub fn from_bytes(mut bytes: [u8; 8]) -> Option<Mark> {
        for (byte, key) in bytes.iter_mut().zip(KEY) {
            *byte ^= key;
        }
        if bytes[0] >> 4 != FORMAT || bytes[7] != check(&bytes[..7]) {
            return None;
        }
        Some(Mark {
            version: [bytes[1], bytes[2], bytes[3]],
            client: Client::from_code(bytes[0] & 0x0f)?,
            user: u32::from_be_bytes([0, bytes[4], bytes[5], bytes[6]]),
        })
    }

    pub fn to_u64(self) -> u64 {
        u64::from_be_bytes(self.to_bytes())
    }

    pub fn from_u64(value: u64) -> Option<Mark> {
        Mark::from_bytes(value.to_be_bytes())
    }
}

/// Reading marks back, which only the tests do: tgradish writes them, and
/// neither the command line nor the GUI reads them.
#[cfg(test)]
impl Mark {
    /// The first mark in `bits`, which hold copies of it one after another
    /// (as `.tgs` files do): bit by bit the majority of the copies, else
    /// any copy that reads.
    pub(crate) fn from_bits(bits: &[bool]) -> Option<Mark> {
        let copies: Vec<&[bool]> = bits.as_chunks::<64>().0.iter().map(|copy| &copy[..]).collect();
        let read = |bit: &dyn Fn(usize) -> bool| {
            let mut bytes = [0u8; 8];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = (0..8).fold(0, |acc, at| acc << 1 | u8::from(bit(index * 8 + at)));
            }
            Mark::from_bytes(bytes)
        };
        let majority =
            |bit: usize| copies.iter().filter(|copy| copy[bit]).count() * 2 > copies.len();
        read(&majority).or_else(|| copies.iter().find_map(|copy| read(&|bit| copy[bit])))
    }

    /// The user hash as text, like `#3fa9c1`.
    pub(crate) fn user_text(&self) -> String {
        format!("#{:06x}", self.user)
    }

    pub(crate) fn version_text(&self) -> String {
        let [major, minor, patch] = self.version;
        format!("{major}.{minor}.{patch}")
    }
}

/// The bits of `bytes`, highest first.
#[cfg(test)]
pub(crate) fn bits(bytes: &[u8]) -> Vec<bool> {
    bytes.iter().flat_map(|&byte| (0..8).rev().map(move |bit| byte >> bit & 1 == 1)).collect()
}

/// The first three numbers of `version`, as `2.1.0-rc.1` has them before
/// what a pre-release or a build adds. One that isn't a number, or is more
/// than a byte holds, is 255.
fn version_numbers(version: &str) -> [u8; 3] {
    let release = version.split(['-', '+']).next().unwrap_or_default();
    let mut numbers = [0u8; 3];
    for (number, part) in numbers.iter_mut().zip(release.split('.')) {
        *number = part.parse().unwrap_or(u8::MAX);
    }
    numbers
}

/// CRC-8 of the tool's name and `bytes`, so other programs' random bytes
/// rarely read as a mark.
fn check(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in b"tgradish".iter().chain(bytes) {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { crc << 1 ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

/// 24 bits of FNV-1a over the user's name; 0 without one.
fn user_hash() -> u32 {
    let name = ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|name| !name.is_empty()));
    let Some(name) = name else { return 0 };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in b"tgradish user ".iter().chain(name.as_bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    // folded, so every bit of the hash counts; never 0, which means none
    let folded = (hash ^ hash >> 24 ^ hash >> 48) as u32 & 0x00ff_ffff;
    folded.max(1)
}

/// The mark in a sticker file, if it has one.
#[cfg(test)]
pub(crate) fn read_file(path: &std::path::Path) -> Option<Mark> {
    if crate::input::sniff(path) == crate::input::FileKind::Tgs {
        let json = crate::tgs::read_json(path).ok()?;
        Mark::from_bits(&tgradish_tgs::mark::order_bits(&json)).or_else(|| {
            let bytes = tgradish_tgs::mark::layer_name(&json)?;
            Mark::from_bytes(bytes.try_into().ok()?)
        })
    } else {
        let info = crate::webm::container::inspect_file(path).ok()?;
        info.video.and_then(|video| video.uid).and_then(Mark::from_u64)
    }
}

/// Returns true if `path` carries tgradish's visible watermark, indicating it
/// was created by tgradish and is safe to replace on re-conversion:
/// - `.tgs`: Lottie `nm` field matches tgradish's naming.
/// - WebM: `WritingApp`, `MuxingApp`, and padding signature match tgradish.
/// - WebP: XMP `CreatorTool` matches tgradish.
///
/// Only the header is read for WebM files, and images/`.tgs` files over 16 MiB
/// return false without being read in full, so checking a long video reads
/// little of it.
pub fn made_by_tgradish(path: &std::path::Path) -> bool {
    let small = || crate::fsutil::read_at_most(path, MADE_BYTES).ok().flatten();
    use crate::input::FileKind;
    match crate::input::sniff(path) {
        FileKind::Webp { .. } => {
            let tool = small().and_then(|webp| crate::webp::creator_tool(&webp));
            tool.as_deref().is_some_and(is_tool_id)
        }
        FileKind::Tgs => {
            let Some(bytes) = small() else { return false };
            let json = match bytes.starts_with(&[0x1f, 0x8b]) {
                true => tgradish_tgs::file::unpack(&bytes).ok(),
                // plain Lottie JSON, named .tgs
                false => Some(bytes),
            };
            json.and_then(|json| serde_json::from_slice::<serde_json::Value>(&json).ok())
                .and_then(|value| value.get("nm")?.as_str().map(tgradish_name))
                .unwrap_or(false)
        }
        FileKind::Webm => crate::webm::container::head_file(path).is_ok_and(|head| {
            let app = |app: &Option<String>| app.as_deref().is_some_and(is_tool_id);
            app(&head.writing_app)
                && app(&head.muxing_app)
                && head.signature.as_deref().is_some_and(|signature| {
                    signature
                        .strip_suffix(crate::REPOSITORY)
                        .and_then(|tool| tool.strip_suffix(' '))
                        .is_some_and(is_tool_id)
                })
        }),
        // tgradish makes no other kind of file
        _ => false,
    }
}

/// The largest picture or `.tgs` [`made_by_tgradish`] reads: many times
/// what Telegram takes, which is all tgradish makes, even of stickers
/// that don't fit.
const MADE_BYTES: u64 = 16 << 20;

/// Whether `text` is `tgradish VERSION`, as `crate::TOOL_ID` is: numbers
/// between dots, and after them what a pre-release or a build adds, as
/// `-rc.1` does.
fn is_tool_id(text: &str) -> bool {
    let Some(version) = text.strip_prefix("tgradish ") else { return false };
    let (release, added) = match version.find(['-', '+']) {
        Some(at) => (&version[..at], Some(&version[at + 1..])),
        None => (version, None),
    };
    let part_of_one = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+');
    release.split('.').all(|part| part.parse::<u32>().is_ok())
        && added.is_none_or(|added| !added.is_empty() && added.chars().all(part_of_one))
}

/// Whether `name` is one tgradish gives stickers: `made with tgradish
/// VERSION`, or a title followed by ` (made with tgradish)`.
fn tgradish_name(name: &str) -> bool {
    name.strip_prefix(crate::UNTITLED).is_some_and(is_tool_id) || name.ends_with(crate::TITLED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_what_it_writes() {
        let mark = Mark { version: [2, 0, 1], client: Client::Gui, user: 0x3f_a9c1 };
        let bytes = mark.to_bytes();
        assert_eq!(Mark::from_bytes(bytes), Some(mark));
        assert_eq!(Mark::from_u64(mark.to_u64()), Some(mark));
        // any change to a byte is noticed
        for index in 0..8 {
            let mut changed = bytes;
            changed[index] ^= 0x10;
            assert_eq!(Mark::from_bytes(changed), None, "byte {index}");
        }
        assert_eq!(mark.user_text(), "#3fa9c1");
        assert_eq!(mark.version_text(), "2.0.1");
    }

    #[test]
    fn reads_copies_by_majority() {
        let mark = Mark { version: [2, 0, 0], client: Client::Cli, user: 42 };
        let copy = bits(&mark.to_bytes());
        let mut three: Vec<bool> = copy.iter().chain(&copy).chain(&copy).copied().collect();
        // a bit wrong in one copy is outvoted
        three[5] = !three[5];
        three[64 + 30] = !three[64 + 30];
        assert_eq!(Mark::from_bits(&three), Some(mark));
        // a single copy, and too few bits
        assert_eq!(Mark::from_bits(&copy), Some(mark));
        assert_eq!(Mark::from_bits(&copy[..63]), None);
    }

    #[test]
    fn knows_its_names() {
        assert!(tgradish_name("made with tgradish 2.0.0"));
        assert!(tgradish_name("dance (made with tgradish)"));
        assert!(!tgradish_name("tgradish tutorial"));
        assert!(!tgradish_name("made with tgradish 2.x"));
        assert!(is_tool_id(crate::TOOL_ID) && !is_tool_id("tgradish-ish 1.0"));
        // versions before a release, and builds of one
        for tool in ["tgradish 2.1.0-rc.1", "tgradish 2.1.0+linux.5", "tgradish 2.1.0-rc.1+5"] {
            assert!(is_tool_id(tool), "{tool}");
            assert!(tgradish_name(&format!("made with {tool}")), "{tool}");
        }
        for other in ["tgradish ", "tgradish 2.1.0-", "tgradish 2.1.0-rc 1", "tgradish -rc.1"] {
            assert!(!is_tool_id(other), "{other}");
        }
        // as stickers are named
        let named = |title: Option<&str>| {
            let selection =
                crate::options::Selection { title: title.map(Into::into), ..Default::default() };
            crate::tgs::Options::default().sticker_options(&selection).unwrap().name.unwrap()
        };
        assert!(tgradish_name(&named(None)) && tgradish_name(&named(Some("dance"))));
        // and how inspecting finds the signature in WebM padding
        assert!(crate::signature().starts_with(crate::webm::container::SIGNATURE_PREFIX));
    }

    #[test]
    fn reads_the_numbers_of_a_version() {
        assert_eq!(version_numbers("2.0.1"), [2, 0, 1]);
        // before a release the patch is still a number
        assert_eq!(version_numbers("2.1.0-rc.1"), [2, 1, 0]);
        assert_eq!(version_numbers("10.20.30+linux.5"), [10, 20, 30]);
        // what a byte can't hold, and what isn't there
        assert_eq!(version_numbers("2.300.x"), [2, 255, 255]);
        assert_eq!(version_numbers("2"), [2, 0, 0]);
        assert_eq!(Mark::current().version, version_numbers(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn reads_the_mark_of_a_sticker_it_made() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let input = dir.path().join("walker.gif");
        std::fs::write(&input, &walker.bytes).unwrap();
        let cancel = crate::CancelToken::new();
        let make = |name: &str, watermark| {
            let request = crate::tgs::Request {
                output: Some(dir.path().join(name)),
                options: crate::tgs::Options { watermark, ..Default::default() },
                ..crate::tgs::Request::new(crate::input::Input::File(input.clone()))
            };
            crate::tgs::convert(None, &request, &cancel, &mut |_| {}).unwrap().output
        };
        assert_eq!(read_file(&make("walker.tgs", None)), Some(Mark::current()));
        // without the watermark the rectangles keep their own order, and
        // no layer is named
        let json = crate::tgs::read_json(&make("unmarked.tgs", Some(false))).unwrap();
        let bits = tgradish_tgs::mark::order_bits(&json);
        assert!(bits.len() >= 64 && !bits.contains(&true), "{bits:?}");
        assert_eq!(tgradish_tgs::mark::layer_name(&json), None);
    }

    #[test]
    fn writes_a_mark_only_with_the_watermark() {
        assert_eq!(Mark::written(true), Some(Mark::current()));
        assert_eq!(Mark::written(false), None);
    }

    #[test]
    fn rarely_reads_random_bytes_as_marks() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let found = (0..20_000)
            .filter(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                Mark::from_u64(state).is_some()
            })
            .count();
        assert!(found < 10, "{found}");
    }
}

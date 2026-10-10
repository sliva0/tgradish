//! A mark hidden in every sticker tgradish makes, which no option removes:
//! the version of tgradish that made it, through which front-end, and a
//! hash of the user's name. The hash tells stickers by one person from
//! others' without saying who it is.
//!
//! The mark is 8 bytes that look random: in WebM files they are the video
//! track's UID, in `.tgs` files the order of rectangles that may come in
//! any order (see `tgradish_tgs::mark`).

use std::sync::OnceLock;

use serde::Serialize;

/// Which front-end made a sticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    /// tgradish as a library, or a front-end that didn't say.
    Library,
    Cli,
    Window,
    Bot,
}

impl Client {
    fn code(self) -> u8 {
        match self {
            Client::Library => 0,
            Client::Cli => 1,
            Client::Window => 2,
            Client::Bot => 3,
        }
    }

    fn from_code(code: u8) -> Option<Client> {
        Some(match code {
            0 => Client::Library,
            1 => Client::Cli,
            2 => Client::Window,
            3 => Client::Bot,
            _ => return None,
        })
    }
}

static CLIENT: OnceLock<Client> = OnceLock::new();

/// Says which front-end this process is; marks made before or without it
/// say [`Client::Library`]. Only the first call counts.
pub fn set_client(client: Client) {
    let _ = CLIENT.set(client);
}

/// What a mark says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Mark {
    /// tgradish's version: major, minor, patch.
    pub version: [u8; 3],
    pub client: Client,
    /// 24 bits of a hash of the user's name, 0 when there was none.
    pub user: u32,
}

/// Layout of the bytes, in the high half of the first byte.
const FORMAT: u8 = 1;
/// Mixed into the bytes so they look random; marks of different stickers
/// still match where their content does.
const KEY: [u8; 8] = [0x9e, 0x37, 0x79, 0xb9, 0x7f, 0x4a, 0x7c, 0x15];

impl Mark {
    /// The mark this process puts into stickers.
    pub fn current() -> Mark {
        let mut version = [0u8; 3];
        for (part, number) in version.iter_mut().zip(env!("CARGO_PKG_VERSION").split('.')) {
            *part = number.parse().unwrap_or(u8::MAX);
        }
        Mark {
            version,
            client: CLIENT.get().copied().unwrap_or(Client::Library),
            user: user_hash(),
        }
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

    /// The first mark in `bits`, which hold copies of it one after another
    /// (as `.tgs` files do): bit by bit the majority of the copies, else
    /// any copy that reads.
    pub fn from_bits(bits: &[bool]) -> Option<Mark> {
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
    pub fn user_text(&self) -> String {
        format!("#{:06x}", self.user)
    }

    pub fn version_text(&self) -> String {
        let [major, minor, patch] = self.version;
        format!("{major}.{minor}.{patch}")
    }
}

/// The bits of `bytes`, highest first.
pub fn bits(bytes: &[u8]) -> Vec<bool> {
    bytes.iter().flat_map(|&byte| (0..8).rev().map(move |bit| byte >> bit & 1 == 1)).collect()
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
pub fn read_file(path: &std::path::Path) -> Option<Mark> {
    if crate::tgs::is_sticker(path) {
        let json = crate::tgs::read_json(path).ok()?;
        Mark::from_bits(&tgradish_tgs::mark::hidden_bits(&json))
    } else {
        let info = crate::webm::inspect_file(path).ok()?;
        info.video.and_then(|video| video.uid).and_then(Mark::from_u64)
    }
}

/// Whether tgradish made the sticker at `path`: its hidden mark, or for
/// older files the marks in its metadata.
pub fn made_by_tgradish(path: &std::path::Path) -> bool {
    if read_file(path).is_some() {
        return true;
    }
    if crate::tgs::is_sticker(path) {
        let Ok(json) = crate::tgs::read_json(path) else { return false };
        serde_json::from_slice::<serde_json::Value>(&json)
            .ok()
            .and_then(|value| value.get("nm")?.as_str().map(|name| name.contains("tgradish")))
            .unwrap_or(false)
    } else {
        crate::webm::inspect_file(path).is_ok_and(|info| {
            info.signature.is_some()
                || [&info.muxing_app, &info.writing_app]
                    .iter()
                    .any(|app| app.as_deref().is_some_and(|app| app.starts_with("tgradish")))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_what_it_writes() {
        let mark = Mark { version: [2, 0, 1], client: Client::Window, user: 0x3f_a9c1 };
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

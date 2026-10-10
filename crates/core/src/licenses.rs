//! The licence notices of what tgradish is built from: its own licence,
//! the Rust crates', and in builds with ffmpeg linked in, ffmpeg's and its
//! libraries'. Packed into the binary (see `build.rs`), so it carries them
//! wherever it is copied.

use std::io::Read;

/// One licence notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub text: String,
}

/// Every notice of this build, tgradish's own first.
pub fn notices() -> Vec<Notice> {
    const PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/notices.deflate"));
    let mut text = String::new();
    flate2::read::DeflateDecoder::new(PACKED)
        .read_to_string(&mut text)
        .expect("the build packs valid text");
    text.split_terminator('\u{1e}')
        .filter_map(|record| record.split_once('\u{1f}'))
        .map(|(title, text)| Notice { title: title.to_owned(), text: text.to_owned() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_every_notice() {
        let notices = notices();
        assert_eq!(notices[0].title, "tgradish");
        assert!(notices[0].text.contains("MIT License"));
        let crates = notices.iter().find(|notice| notice.title == "Rust crates").unwrap();
        assert!(crates.text.contains("used by:"));
        let ffmpeg = notices.iter().any(|notice| notice.title.starts_with("ffmpeg"));
        assert_eq!(ffmpeg, cfg!(feature = "linked"));
    }
}

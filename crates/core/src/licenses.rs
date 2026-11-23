//! Licence notices embedded in the binary at build time (see `build.rs`):
//! tgradish's own licence, Rust crates, libwebp, and (in `linked-static`
//! builds) ffmpeg and its dependencies.

use std::fmt;
use std::io::Read;

include!("notices.rs");

/// Single licence or third-party notice.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Notice {
    /// Component and notice category.
    pub kind: Kind,
    /// Full verbatim licence text.
    pub text: String,
}

impl Notice {
    /// Creates a notice for front-ends that add extra notices (such as fonts).
    pub fn new(kind: Kind, text: impl Into<String>) -> Notice {
        Notice { kind, text: text.into() }
    }

    /// English title displayed by the CLI.
    pub fn title(&self) -> String {
        self.kind.to_string()
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Tgradish => write!(f, "tgradish"),
            Kind::Licence { of } => write!(f, "{of}: licence"),
            Kind::NamedLicence { of, licence } => write!(f, "{of}: {licence}"),
            Kind::LicenceNotes { of } => write!(f, "{of}: licence notes"),
            Kind::PatentGrant { of } => write!(f, "{of}: patent grant"),
            Kind::Sources { of } => write!(f, "{of}: what is built in, and its sources"),
            Kind::Crates => write!(f, "Rust crates"),
            Kind::Font { name, subsets: 1 } => write!(f, "Font: {name}, a subset"),
            Kind::Font { name, .. } => write!(f, "Font: {name}, subsets"),
        }
    }
}

/// Returns every notice bundled into this build, with tgradish's licence first.
pub fn notices() -> Vec<Notice> {
    const PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/notices.deflate"));
    let mut text = String::new();
    flate2::read::DeflateDecoder::new(PACKED)
        .read_to_string(&mut text)
        .expect("the build packs valid text");
    text.split_terminator('\u{1e}')
        .filter_map(|record| record.split_once('\u{1f}'))
        .map(|(index, text)| {
            let index: usize = index.parse().expect("the build packs each notice's place");
            Notice::new(FILES[index].2, text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_every_notice() {
        let notices = notices();
        assert_eq!(notices[0].kind, Kind::Tgradish);
        assert_eq!(notices[0].title(), "tgradish");
        assert!(notices[0].text.contains("MIT License"));
        let crates = notices.iter().find(|notice| notice.kind == Kind::Crates).unwrap();
        assert_eq!(crates.title(), "Rust crates");
        assert!(crates.text.contains("used by:"));
        let webp = Kind::Licence { of: "libwebp" };
        assert!(notices.iter().any(|notice| notice.kind == webp));
        assert_eq!(webp.to_string(), "libwebp: licence");
        // of the static build alone: linked to a distribution's ffmpeg, the
        // binary holds none of it
        let ffmpeg = notices.iter().any(|notice| notice.title().starts_with("ffmpeg"));
        assert_eq!(ffmpeg, cfg!(feature = "linked-static"));
        let sources = Kind::Sources { of: "ffmpeg" };
        assert_eq!(notices.iter().any(|notice| notice.kind == sources), ffmpeg);
        assert_eq!(sources.to_string(), "ffmpeg: what is built in, and its sources");
    }
}

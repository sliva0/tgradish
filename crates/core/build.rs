//! Packs the licence notices of what tgradish is built from, compressed,
//! so a binary on its own carries them; `licenses` reads them back for
//! `tgradish licenses`. The texts are in this crate, since a published
//! crate is built without the repository around it: tgradish's own beside
//! this file, and in `licenses/` the Rust crates' from `cargo xtask
//! licenses`, ffmpeg's from `cargo xtask ffmpeg`, and libwebp's from the
//! libwebp-sys crate's copy of it.

use std::io::Write;
use std::path::Path;

include!("src/notice_titles.rs");

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/notice_titles.rs");
    let read = |path: &str| {
        let path = root.join(path);
        println!("cargo:rerun-if-changed={}", path.display());
        std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
    };
    let mut notices = vec![("tgradish".to_owned(), read("LICENSE.txt"))];
    // only builds with the release's ffmpeg in them carry its notices:
    // one linked to a distribution's libraries holds no copy of ffmpeg, and
    // those aren't the build these files describe
    if std::env::var_os("CARGO_FEATURE_LINKED_STATIC").is_some() {
        for (file, title) in [
            ("SOURCES.txt", FFMPEG_SOURCES),
            ("ffmpeg-COPYING.LGPLv2.1", "ffmpeg: GNU Lesser General Public License 2.1"),
            ("ffmpeg-LICENSE.md", "ffmpeg: licence notes"),
            ("libvpx-LICENSE", "libvpx: licence"),
            ("libvpx-PATENTS", "libvpx: patent grant"),
            ("dav1d-COPYING", "dav1d: licence"),
            ("zlib-LICENSE", "zlib: licence"),
        ] {
            notices.push((title.to_owned(), read(&format!("licenses/ffmpeg/{file}"))));
        }
    }
    // libwebp, which every build has, from libwebp-sys
    for (file, title) in [("COPYING", "libwebp: licence"), ("PATENTS", "libwebp: patent grant")] {
        notices.push((title.to_owned(), read(&format!("licenses/libwebp/{file}"))));
    }
    notices.push((CRATES.to_owned(), read("licenses/THIRD-PARTY-CRATES.txt")));

    // titles and texts, apart by unit and record separators
    let mut packed = String::new();
    for (title, text) in notices {
        packed.push_str(&title);
        packed.push('\u{1f}');
        packed.push_str(&text);
        packed.push('\u{1e}');
    }
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(packed.as_bytes()).expect("writes to memory");
    let out = Path::new(&std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("notices.deflate");
    std::fs::write(out, encoder.finish().expect("writes to memory")).expect("OUT_DIR is writable");
}

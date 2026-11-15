//! Packs the licence notices of what tgradish is built from, compressed,
//! so a binary on its own carries them; `licenses` reads them back for
//! `tgradish licenses`. The texts are in this crate, since a published
//! crate is built without the repository around it: tgradish's own beside
//! this file, and in `licenses/` the Rust crates' from `cargo xtask
//! licenses`, ffmpeg's from `cargo xtask ffmpeg`, and libwebp's from the
//! libwebp-sys crate's copy of it. `src/notices.rs` lists them.

use std::io::Write;
use std::path::Path;

// what each notice is matters only to `licenses`
#[allow(dead_code)]
mod notices {
    include!("src/notices.rs");
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/notices.rs");
    let ffmpeg = std::env::var_os("CARGO_FEATURE_LINKED_STATIC").is_some();
    // each notice as its place in the list and its text, apart by unit and
    // record separators
    let mut packed = String::new();
    for (index, (file, of_ffmpeg, _)) in notices::FILES.iter().enumerate() {
        if *of_ffmpeg && !ffmpeg {
            continue;
        }
        let path = root.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        packed.push_str(&index.to_string());
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

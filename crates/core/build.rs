//! Compresses the licence notices of what this build contains (ffmpeg's only
//! with `linked-static`) into `OUT_DIR/notices.deflate` so standalone binaries
//! embed them for `tgradish licenses`.
//!
//! Notice files live inside this crate so published crate builds work without
//! the surrounding workspace: tgradish's own licence sits beside this file, and
//! `licenses/` holds crate notices (from `cargo xtask licenses`), ffmpeg's
//! notices (from `cargo xtask ffmpeg`), and libwebp's licence (from
//! `libwebp-sys`). `src/notices.rs` lists them.

use std::io::Write;
use std::path::Path;

// Notice categories are only used by `licenses.rs`.
#[allow(dead_code)]
mod notices {
    include!("src/notices.rs");
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/notices.rs");
    let ffmpeg = std::env::var_os("CARGO_FEATURE_LINKED_STATIC").is_some();
    // Store each notice as `<index>\x1f<text>\x1e`.
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

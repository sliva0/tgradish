//! The resources of tgradish.exe: its icon, its manifest, and the version
//! Explorer shows on a file's Details tab. The version is Cargo.toml's,
//! written into both here, so no release has to remember them.

use std::path::Path;

/// What the manifest says in place of its version.
const MANIFEST_VERSION: &str = "version=\"0.0.0.0\"";

fn main() {
    for file in ["tgradish.rc", "tgradish.exe.manifest", "assets/tgradish.ico"] {
        println!("cargo::rerun-if-changed={file}");
    }
    let version = env!("CARGO_PKG_VERSION");
    let [major, minor, patch] = [
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    ];

    // Windows wants four numbers there
    let manifest = std::fs::read_to_string("tgradish.exe.manifest")
        .expect("tgradish.exe.manifest should be beside build.rs");
    assert_eq!(
        manifest.matches(MANIFEST_VERSION).count(),
        1,
        "tgradish.exe.manifest should say {MANIFEST_VERSION} once, for its own version"
    );
    let manifest =
        manifest.replace(MANIFEST_VERSION, &format!("version=\"{major}.{minor}.{patch}.0\""));
    // where tgradish.rc finds it: embed-resource has every resource compiler
    // look in OUT_DIR
    let out = std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR");
    std::fs::write(Path::new(&out).join("tgradish.versioned.manifest"), manifest)
        .expect("OUT_DIR is writable");

    // with rc.exe from the Windows SDK, or MinGW's windres. The macros are
    // bare numbers and a word without spaces, which every one of them
    // passes on unharmed, as they wouldn't quotes
    embed_resource::compile(
        "tgradish.rc",
        [
            format!("TGRADISH_VERSION={version}"),
            format!("TGRADISH_MAJOR={major}"),
            format!("TGRADISH_MINOR={minor}"),
            format!("TGRADISH_PATCH={patch}"),
        ],
    )
    .manifest_required()
    .expect("the Windows resources should compile");
}

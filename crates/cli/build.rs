//! Embeds Windows resources into `tgradish.exe`: icon, manifest, and the
//! version shown on Explorer's Details tab. `build.rs` writes `Cargo.toml`'s
//! version into both so releases do not have to update them.

use std::path::Path;

/// Placeholder version string in `tgradish.exe.manifest`.
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

    // Windows manifests require a four-part version.
    let manifest = std::fs::read_to_string("tgradish.exe.manifest")
        .expect("tgradish.exe.manifest should be beside build.rs");
    assert_eq!(
        manifest.matches(MANIFEST_VERSION).count(),
        1,
        "tgradish.exe.manifest should say {MANIFEST_VERSION} once, for its own version"
    );
    let manifest =
        manifest.replace(MANIFEST_VERSION, &format!("version=\"{major}.{minor}.{patch}.0\""));
    // embed-resource adds OUT_DIR to the resource compiler's include path so
    // tgradish.rc can find this file.
    let out = std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR");
    std::fs::write(Path::new(&out).join("tgradish.versioned.manifest"), manifest)
        .expect("OUT_DIR is writable");

    // Works with both rc.exe (Windows SDK) and windres (MinGW). The macros are
    // bare numbers and a token without spaces, which both compilers pass
    // through unchanged (unlike quotes).
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

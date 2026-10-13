//! Development tasks, run with `cargo xtask <task>`.
//!
//! `cargo xtask ffmpeg [--target TARGET] [--no-asm]` builds the minimal
//! static ffmpeg libraries that release builds link in: only what tgradish
//! needs to decode common inputs and encode VP9 WebM. Runs on Linux, for the
//! machine's own architecture; Windows builds are cross-compiled with
//! mingw-w64. Needs a C toolchain, make, pkg-config, nasm (x86-64, unless
//! --no-asm), meson, ninja, curl and tar. Targets are `linux-x86_64`,
//! `linux-aarch64` and `windows-x86_64`.
//!
//! The static libraries end up in `target/ffmpeg/prefix-<triple>`, and the
//! licences of everything in them in `target/ffmpeg/licenses-<triple>`.
//! ffmpeg is configured without GPL parts, so it is LGPL 2.1 or later.
//! Point `PKG_CONFIG_PATH` at the prefix's `lib/pkgconfig` to build
//! tgradish with `--features linked-static`.
//!
//! `cargo xtask package --target TARGET [--system-ffmpeg] [--max-glibc
//! 2.28]` packs a built tgradish into a release archive. With
//! `--system-ffmpeg` it is a build without ffmpeg, which uses the system's.
//! Linux builds with ffmpeg built in are also packed as a .deb and an .rpm,
//! with nfpm, which it downloads.
//!
//! `cargo xtask ffmpeg-sources` packs the exact source archives of the
//! ffmpeg build into `target/ffmpeg/ffmpeg-<version>-sources.tar`, which
//! the release workflow publishes with each release, as the LGPL asks.
//!
//! `cargo xtask local-release [--targets linux-x86_64,windows-x86_64]
//! [--no-asm]` builds and packs every release variant for manual tests,
//! like the release workflow: with ffmpeg built in, and for Linux also
//! without. ffmpeg is built first where its libraries are missing or were
//! built from another recipe. Windows needs mingw-w64. The archives and
//! binaries end up in `target/release-artifacts`.
//!
//! `cargo xtask licenses [--check]` writes the licences of the Rust crates
//! in tgradish to `licenses/THIRD-PARTY-CRATES.txt`, which binaries carry;
//! `--check` fails if it is out of date, as after dependencies change. Needs
//! cargo-about 0.9.2, which CI uses too: other versions word the list
//! differently. cargo-about skips files a `.gitignore` above the crates'
//! sources ignores, such as one of a home directory kept in git: then set
//! `CARGO_HOME` to a directory outside it. `cargo xtask ffmpeg` keeps
//! `licenses/ffmpeg` up to date the same way.
//!
//! `cargo xtask icons [--check]` makes the icons for menus and the Windows
//! executable from the window's own, `crates/gui/assets/icon.png`;
//! `--check` fails if they are out of date.
//!
//! `cargo xtask screenshot` renders the window at work from the synthetic
//! art to `.github/screenshot.png`, the README's picture of it. Needs a
//! GPU wgpu can use.

mod ffmpeg;
mod icons;
mod licences;
mod package;
mod release;
mod screenshot;

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ffmpeg") => ffmpeg::build_ffmpeg(&args[1..]),
        Some("ffmpeg-sources") => ffmpeg::package_sources(),
        Some("package") => package::package_release(&args[1..]),
        Some("licenses") => licences::crate_licenses(&args[1..]),
        Some("icons") => icons::icons(&args[1..]),
        Some("local-release") => release::local_release(&args[1..]),
        Some("screenshot") => screenshot::screenshot(&args[1..]),
        _ => bail!(
            "usage: cargo xtask ffmpeg [--target TARGET] [--no-asm]\n       \
             cargo xtask ffmpeg-sources\n       \
             cargo xtask package --target TARGET [--system-ffmpeg] [--max-glibc 2.28]\n       \
             cargo xtask licenses [--check]\n       \
             cargo xtask icons [--check]\n       \
             cargo xtask local-release [--targets TARGET,...] [--no-asm]\n       \
             cargo xtask screenshot\n\
             targets: linux-x86_64, linux-aarch64, windows-x86_64"
        ),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Target {
    LinuxX64,
    LinuxArm64,
    WindowsX64,
}

impl Target {
    const ALL: [Target; 3] = [Target::LinuxX64, Target::LinuxArm64, Target::WindowsX64];

    fn name(self) -> &'static str {
        match self {
            Target::LinuxX64 => "linux-x86_64",
            Target::LinuxArm64 => "linux-aarch64",
            Target::WindowsX64 => "windows-x86_64",
        }
    }

    fn parse(name: Option<&String>) -> Result<Target> {
        let names: Vec<_> = Target::ALL.iter().map(|target| target.name()).collect();
        Target::ALL
            .into_iter()
            .find(|target| Some(target.name()) == name.map(String::as_str))
            .with_context(|| {
                format!("unknown target {name:?}, expected one of {}", names.join(", "))
            })
    }

    /// Linux on the machine's own architecture.
    fn host() -> Target {
        match std::env::consts::ARCH {
            "aarch64" => Target::LinuxArm64,
            _ => Target::LinuxX64,
        }
    }

    fn triple(self) -> &'static str {
        match self {
            Target::LinuxX64 => "x86_64-unknown-linux-gnu",
            Target::LinuxArm64 => "aarch64-unknown-linux-gnu",
            Target::WindowsX64 => "x86_64-pc-windows-gnu",
        }
    }

    fn is_windows(self) -> bool {
        self == Target::WindowsX64
    }

    fn cross_prefix(self) -> Option<&'static str> {
        self.is_windows().then_some("x86_64-w64-mingw32-")
    }

    fn exe(self, name: &str) -> String {
        if self.is_windows() { format!("{name}.exe") } else { name.to_string() }
    }
}

/// Runs `cmd`, shown first on stderr, and fails if it does.
fn run(cmd: &mut Command) -> Result<()> {
    eprintln!("$ {cmd:?}");
    let status = cmd.status().with_context(|| format!("could not run {cmd:?}"))?;
    ensure!(status.success(), "{cmd:?} failed with {status}");
    Ok(())
}

/// What `cmd` prints, failing if it does.
fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().with_context(|| format!("could not run {cmd:?}"))?;
    ensure!(out.status.success(), "{cmd:?} failed with {}", out.status);
    Ok(String::from_utf8(out.stdout)?)
}

/// The repository's top directory, which every task's paths start from:
/// xtask is built from a directory just inside it.
fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

/// Cargo, as a command: the one running xtask, which `cargo run` names in
/// `CARGO`, so what xtask builds or lists uses the same toolchain; or
/// cargo from PATH when xtask runs on its own.
fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

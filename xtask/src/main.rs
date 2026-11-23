//! Development and release tasks, run with `cargo xtask <task>`.
//!
//! - `ffmpeg [--target TARGET] [--no-asm] [--force]`: builds the minimal static
//!   LGPL 2.1+ ffmpeg libraries linked into release builds (decoders for common
//!   inputs and VP9 WebM encoding only, with no GPL components). Runs on Linux
//!   for the host architecture, cross-compiles Windows with mingw-w64, and
//!   builds musl with `musl-gcc` (`musl-tools` on Debian/Ubuntu, `musl` on
//!   Arch). Requires a C toolchain, `make`, `pkg-config`, `nasm` (x86-64 unless
//!   `--no-asm`), `meson`, `ninja`, `curl` and `tar`. Targets: `linux-x86_64`,
//!   `linux-aarch64`, `linux-musl-x86_64`, `linux-musl-aarch64` (as on Alpine)
//!   and `windows-x86_64`. Static libraries go to
//!   `target/ffmpeg/prefix-<triple>`, licences to
//!   `target/ffmpeg/licenses-<triple>` (and `crates/core/licenses/ffmpeg`),
//!   downloaded tarballs to `target/ffmpeg/sources`, and build trees to
//!   `target/ffmpeg/build-<triple>`. Cached builds from the same recipe and
//!   `xtask/src/ffmpeg.rs` are reused unless `--force` is passed.
//! - `build --target TARGET [--system-ffmpeg]`: builds a release binary into
//!   `target/<triple>/release`, statically linking the built ffmpeg unless
//!   `--system-ffmpeg` is passed (Windows uses mingw-w64, musl uses
//!   `musl-gcc`). `local-release` builds with it too; on its own, it relinks
//!   tgradish against a modified ffmpeg, as the ffmpeg sources' README says.
//! - `ffmpeg-sources`: packs the exact source tarballs of the static ffmpeg
//!   build into `target/ffmpeg/ffmpeg-<version>-sources.tar`, which the release
//!   workflow publishes with each release for LGPL compliance.
//! - `local-release [--targets linux-x86_64,windows-x86_64] [--no-asm]
//!   [--max-glibc 2.28]`: builds and packages all release variants, as the
//!   release workflow does for each target (with built-in ffmpeg, plus one
//!   without it for glibc targets, building ffmpeg first if missing or
//!   outdated; Windows requires mingw-w64, musl requires `musl-gcc`).
//!   Archives get the binary, licence notices and, on Linux, the desktop
//!   entry, icons, man pages and completions; glibc builds with built-in
//!   ffmpeg also get `.deb` and `.rpm` packages via `nfpm` (downloaded
//!   automatically). `--max-glibc` fails if a glibc build needs a newer
//!   glibc. Outputs go to `target/release-artifacts`.
//! - `release`: automates the release steps in `CONTRIBUTING.md`:
//!   - `prepare X.Y.Z [--tgs X.Y.Z]`: bumps versions everywhere (including
//!     `tgradish-tgs` when `--tgs` is given), dates `CHANGELOG.md`
//!     `Unreleased`, refreshes both lock files (refusing unless the working
//!     tree is clean), reports whether `tgradish-tgs` changed since the last
//!     release, and prints the commit/tag/push commands.
//!   - `check [--tag vX.Y.Z]`: verifies that all version references (and the
//!     optional tag) agree and `CHANGELOG.md` has a dated section for the
//!     version. Run by CI and by the release workflow before building.
//!   - `notes [X.Y.Z]`: prints the changelog section for the given version (or
//!     `Cargo.toml`'s version), used by the release workflow to draft the
//!     GitHub release.
//!   - `aur [--push]`: once the GitHub release is published, syncs `.aur/`
//!     clones from `packaging/aur/` (cloning if missing), fills in download
//!     checksums from the release, commits, and with `--push` pushes to the AUR.
//!   - `crates [--dry-run]`: publishes unpublished crate versions to crates.io
//!     in dependency order after interactive confirmation.
//! - `licenses [--check]`: writes third-party Rust crate licences to
//!   `crates/core/licenses/THIRD-PARTY-CRATES.txt` (embedded in binaries) using
//!   pinned `cargo-about` 0.9.2 (other versions format the output differently
//!   and are rejected) and copies `LICENSE.txt` into every published crate (as
//!   crates are packaged without the workspace root); `--check` fails if either
//!   is out of date. Because `cargo-about` skips files matched by any parent
//!   `.gitignore` (such as a dotfile repo in `$HOME`), set `CARGO_HOME` outside
//!   it if needed.
//! - `icons [--check]`: generates menu PNG icons and `tgradish.ico` from
//!   `crates/gui/assets/icon.png`; `--check` fails if they are out of date.
//! - `strings [--no-pictures]`: builds `target/strings/index.html` for string
//!   review and GUI translation, pairing each GUI message with screenshots and
//!   listing CLI help pages. Capturing screenshots requires a wgpu-capable GPU
//!   and ffmpeg; `--no-pictures` reuses previous screenshots.

mod build;
mod ffmpeg;
mod icons;
mod licenses;
mod package;
mod release;
mod strings;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ffmpeg") => ffmpeg::build_ffmpeg(&args[1..]),
        Some("ffmpeg-sources") => ffmpeg::package_sources(),
        Some("build") => build::build_release(&args[1..]),
        Some("licenses") => licenses::crate_licenses(&args[1..]),
        Some("icons") => icons::icons(&args[1..]),
        Some("local-release") => release::local_release(&args[1..]),
        Some("release") => release::release(&args[1..]),
        Some("strings") => strings::strings(&args[1..]),
        _ => bail!(
            "usage: cargo xtask ffmpeg [--target TARGET] [--no-asm] [--force]\n       \
             cargo xtask ffmpeg-sources\n       \
             cargo xtask build --target TARGET [--system-ffmpeg]\n       \
             cargo xtask licenses [--check]\n       \
             cargo xtask icons [--check]\n       \
             cargo xtask local-release [--targets TARGET,...] [--no-asm] [--max-glibc 2.28]\n       \
             cargo xtask release prepare|check|notes|aur|crates ...\n       \
             cargo xtask strings [--no-pictures]\n\
             targets: {}",
            Target::names()
        ),
    }
}

/// Where the notices binaries carry are kept, from the repository's top:
/// inside core, whose build script packs them, since a published crate
/// can't read files outside itself.
const NOTICES: &str = "crates/core/licenses";

/// The C compiler of musl targets: musl's own wrapper of the system's gcc,
/// from musl-tools on Debian and Ubuntu, or musl on Arch.
const MUSL_GCC: &str = "musl-gcc";

/// Linux builds are for glibc, or for musl as on Alpine. Both have the
/// GUI, so both link their C library dynamically: a static program
/// can't load the system's graphics libraries.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    LinuxX64,
    LinuxArm64,
    LinuxMuslX64,
    LinuxMuslArm64,
    WindowsX64,
}

impl Target {
    const ALL: [Target; 5] = [
        Target::LinuxX64,
        Target::LinuxArm64,
        Target::LinuxMuslX64,
        Target::LinuxMuslArm64,
        Target::WindowsX64,
    ];

    fn name(self) -> &'static str {
        match self {
            Target::LinuxX64 => "linux-x86_64",
            Target::LinuxArm64 => "linux-aarch64",
            Target::LinuxMuslX64 => "linux-musl-x86_64",
            Target::LinuxMuslArm64 => "linux-musl-aarch64",
            Target::WindowsX64 => "windows-x86_64",
        }
    }

    /// The names of every target, for usage messages.
    fn names() -> String {
        Target::ALL.map(Target::name).join(", ")
    }

    fn parse(name: Option<&String>) -> Result<Target> {
        Target::ALL
            .into_iter()
            .find(|target| Some(target.name()) == name.map(String::as_str))
            .with_context(|| {
                format!("unknown target {name:?}, expected one of {}", Target::names())
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
            Target::LinuxMuslX64 => "x86_64-unknown-linux-musl",
            Target::LinuxMuslArm64 => "aarch64-unknown-linux-musl",
            Target::WindowsX64 => "x86_64-pc-windows-gnu",
        }
    }

    /// The processor, as Linux names it.
    fn arch(self) -> &'static str {
        match self {
            Target::LinuxArm64 | Target::LinuxMuslArm64 => "aarch64",
            Target::LinuxX64 | Target::LinuxMuslX64 | Target::WindowsX64 => "x86_64",
        }
    }

    fn is_windows(self) -> bool {
        self == Target::WindowsX64
    }

    fn is_musl(self) -> bool {
        matches!(self, Target::LinuxMuslX64 | Target::LinuxMuslArm64)
    }

    /// Where musl distributions keep musl's dynamic loader, which a musl
    /// build names to start with.
    fn musl_loader(self) -> String {
        format!("/lib/ld-musl-{}.so.1", self.arch())
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

/// What `cmd` prints, failing if it does, with what it said on stderr,
/// which says why.
fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().with_context(|| format!("could not run {cmd:?}"))?;
    ensure!(
        out.status.success(),
        "{cmd:?} failed with {}:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim_end()
    );
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

/// rustc, as a command, of the same toolchain as [`cargo`]: `RUSTC` if set,
/// as cargo itself takes it, else the one beside `CARGO`, as rustup and
/// distributions install them, else rustc from PATH. PATH's may be another
/// toolchain's, as rust-toolchain.toml picks one only through rustup.
fn rustc() -> Command {
    let beside_cargo = || {
        let cargo = PathBuf::from(std::env::var_os("CARGO")?);
        let rustc = cargo.with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
        rustc.is_file().then(|| rustc.into_os_string())
    };
    Command::new(std::env::var_os("RUSTC").or_else(beside_cargo).unwrap_or_else(|| "rustc".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cargo-about lists the crates of the targets xtask/about.toml names,
    /// which must be those releases are built for.
    #[test]
    fn about_toml_lists_every_target() {
        let about = std::fs::read_to_string(workspace_root().join("xtask/about.toml")).unwrap();
        let (_, list) = about.split_once("targets = [").unwrap();
        let (list, _) = list.split_once(']').unwrap();
        let mut listed: Vec<&str> = list
            .lines()
            .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        let mut built: Vec<&str> = Target::ALL.map(Target::triple).to_vec();
        listed.sort();
        built.sort();
        assert_eq!(listed, built);
    }
}

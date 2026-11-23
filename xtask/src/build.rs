//! Release build runner behind `local-release`, which the release workflow
//! runs too, and `build`, which relinks tgradish against a modified ffmpeg.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};

use crate::ffmpeg::{c_flags, ffmpeg_prefix};
use crate::{MUSL_GCC, Target, cargo, output, run, rustc, workspace_root};

/// Entry point for `cargo xtask build`.
pub fn build_release(args: &[String]) -> Result<()> {
    let mut target = None;
    let mut system_ffmpeg = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--target" => target = Some(Target::parse(iter.next())?),
            "--system-ffmpeg" => system_ffmpeg = true,
            other => bail!("unknown argument {other:?}"),
        }
    }
    let target = target.context("usage: cargo xtask build --target TARGET [--system-ffmpeg]")?;
    build(target, system_ffmpeg)
}

/// Builds tgradish for `target` in release mode, with the static ffmpeg
/// that `cargo xtask ffmpeg` built linked in, or with `system_ffmpeg`
/// without it. Windows builds are cross-compiled with mingw-w64, musl ones
/// made with musl-gcc.
pub fn build(target: Target, system_ffmpeg: bool) -> Result<()> {
    ensure!(!(system_ffmpeg && target.is_windows()), "Windows builds always include ffmpeg");
    let root = workspace_root();
    let mut cargo = cargo();
    cargo
        .current_dir(root)
        .args(["build", "--release", "--locked", "-p", "tgradish"])
        .args(["--target", target.triple()]);
    // the target's code alone, as --target leaves build scripts out of
    // them; each flag a word of its own, so paths with spaces stay whole
    let mut rustflags = remapped_paths(root);
    if !system_ffmpeg {
        let pkgconfig = ffmpeg_prefix(target).join("lib/pkgconfig");
        ensure!(
            pkgconfig.is_dir(),
            "{} is missing; run cargo xtask ffmpeg --target {}",
            pkgconfig.display(),
            target.name()
        );
        cargo
            .args(["--features", "linked-static"])
            .env("PKG_CONFIG_PATH", pkgconfig)
            .env("PKG_CONFIG_ALL_STATIC", "1");
    }
    if target.is_windows() {
        let stubs = root.join("xtask/mingw-stubs");
        cargo
            .env("PKG_CONFIG_ALLOW_CROSS", "1")
            .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER", "x86_64-w64-mingw32-gcc")
            .env("CC_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc")
            .env("AR_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-ar")
            // MinGW-w64 before 12 lacks d3d12video.h, which ffmpeg-sys-next's
            // bindings include; the stubs come last, so they only fill in
            // headers MinGW lacks
            .env(
                "BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnu",
                format!("-I/usr/x86_64-w64-mingw32/include -I{}", stubs.display()),
            );
        // links winpthread and libgcc into the executable instead of
        // depending on their DLLs; packaging checks the result
        rustflags.extend(["-C".into(), "link-arg=-static".into()]);
    }
    if target.is_musl() {
        let triple = target.triple().replace('-', "_");
        let upper = triple.to_uppercase();
        cargo
            .env("PKG_CONFIG_ALLOW_CROSS", "1")
            .env(format!("CC_{triple}"), MUSL_GCC)
            .env(format!("AR_{triple}"), "ar")
            .env(format!("CARGO_TARGET_{upper}_LINKER"), MUSL_GCC);
        // linked to musl dynamically, unlike Rust's musl builds by default,
        // as the GUI loads the system's graphics libraries; with the
        // loader where musl distributions keep it, whatever path the build
        // machine's musl-gcc names
        rustflags.extend([
            "-C".into(),
            "target-feature=-crt-static".into(),
            "-C".into(),
            format!("link-arg=-Wl,--dynamic-linker={}", target.musl_loader()),
            "-L".into(),
            format!("native={}", static_unwinder(root, target)?.display()),
        ]);
        if let Some(flags) = c_flags(target) {
            cargo.env(format!("CFLAGS_{triple}"), flags);
        }
        // ffmpeg's bindings, with libc's types as musl declares them
        if !system_ffmpeg {
            cargo.env(
                format!("BINDGEN_EXTRA_CLANG_ARGS_{triple}"),
                format!("-isystem {}", musl_headers()?.display()),
            );
        }
    }
    cargo.env("CARGO_ENCODED_RUSTFLAGS", rustflags.join("\x1f"));
    run(&mut cargo)
}

/// Returns `--remap-path-prefix` flags so embedded source paths in panics and
/// debug info are reproducible across machines (the workspace as `/tgradish`,
/// `CARGO_HOME` as `/cargo`, as the AUR source package does).
fn remapped_paths(root: &Path) -> Vec<String> {
    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
        Some(PathBuf::from(home).join(".cargo"))
    });
    let mut flags = vec![format!("--remap-path-prefix={}=/tgradish", root.display())];
    if let Some(cargo_home) = cargo_home {
        flags.push(format!("--remap-path-prefix={}=/cargo", cargo_home.display()));
    }
    flags
}

/// Creates a linker script `libgcc_s.so` pointing at Rust's self-contained
/// static `libunwind.a`. Dynamically linked musl builds normally link
/// `libgcc_s`, which Ubuntu does not provide for musl; placing this directory
/// first on the library search path avoids any runtime `libgcc` dependency.
fn static_unwinder(root: &Path, target: Target) -> Result<PathBuf> {
    let libdir = output(rustc().current_dir(root).args([
        "--print",
        "target-libdir",
        "--target",
        target.triple(),
    ]))?;
    let libunwind = Path::new(libdir.trim()).join("self-contained/libunwind.a");
    ensure!(
        libunwind.is_file(),
        "{} is missing: rustup target add {}",
        libunwind.display(),
        target.triple()
    );
    let dir = root.join("target").join(target.triple()).join("static-unwinder");
    std::fs::create_dir_all(&dir)?;
    // a linker script, which ld reads in place of a library
    std::fs::write(dir.join("libgcc_s.so"), format!("INPUT(\"{}\")\n", libunwind.display()))?;
    Ok(dir)
}

/// The folder of musl's headers, where musl-gcc finds stdio.h.
fn musl_headers() -> Result<PathBuf> {
    let mut gcc = Command::new(MUSL_GCC)
        .args(["-M", "-x", "c", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not run {MUSL_GCC} (required for musl builds)"))?;
    gcc.stdin.take().context("no stdin")?.write_all(b"#include <stdio.h>\n")?;
    let out = gcc.wait_with_output()?;
    ensure!(out.status.success(), "{MUSL_GCC} could not find stdio.h");
    let deps = String::from_utf8(out.stdout)?;
    let stdio = deps
        .split_whitespace()
        .find(|word| word.ends_with("/stdio.h"))
        .context("musl-gcc did not report stdio.h")?;
    Ok(PathBuf::from(stdio).parent().context("stdio.h path has no parent directory")?.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_same_paths_on_every_machine() {
        let flags = remapped_paths(Path::new("/home/me/my projects/tgradish"));
        assert_eq!(flags[0], "--remap-path-prefix=/home/me/my projects/tgradish=/tgradish");
        assert!(flags.iter().any(|flag| flag.ends_with("=/cargo")), "{flags:?}");
    }
}

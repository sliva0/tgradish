//! Release builds of tgradish for a target, with the settings the release
//! workflow and `local-release` both build with, kept here so the two
//! can't drift apart.

use anyhow::{Context, Result, bail, ensure};

use crate::ffmpeg::ffmpeg_prefix;
use crate::{Target, cargo, run, workspace_root};

/// The `build` task: [`build`] as its arguments ask.
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
/// without it. Windows builds are cross-compiled with mingw-w64.
pub fn build(target: Target, system_ffmpeg: bool) -> Result<()> {
    ensure!(!(system_ffmpeg && target.is_windows()), "Windows builds always link ffmpeg in");
    let root = workspace_root();
    let mut cargo = cargo();
    cargo
        .current_dir(root)
        .args(["build", "--release", "--locked", "-p", "tgradish"])
        .args(["--target", target.triple()]);
    if !system_ffmpeg {
        let pkgconfig = ffmpeg_prefix(target).join("lib/pkgconfig");
        ensure!(
            pkgconfig.is_dir(),
            "{} is missing, run cargo xtask ffmpeg --target {}",
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
            )
            // links winpthread and libgcc into the executable instead of
            // depending on their DLLs; `xtask package` checks the result
            .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS", "-C link-arg=-static");
    }
    run(&mut cargo)
}

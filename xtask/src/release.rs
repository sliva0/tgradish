//! Every release variant built and packed on this machine, as the release
//! workflow makes them, so they can be tried by hand before a tag sends
//! them out.

use anyhow::{Context, Result, bail};

use crate::ffmpeg::{ffmpeg_for, ffmpeg_prefix, recipe};
use crate::package::package;
use crate::{Target, cargo, run, workspace_root};

/// Builds every release variant of `targets` and packs them into
/// `target/release-artifacts`, building ffmpeg first where its libraries
/// are missing or from another recipe.
pub fn local_release(args: &[String]) -> Result<()> {
    let mut targets = vec![Target::host(), Target::WindowsX64];
    let mut asm = true;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--targets" => {
                let names = iter
                    .next()
                    .context("--targets needs a list, like linux-x86_64,windows-x86_64")?;
                targets = names
                    .split(',')
                    .map(|name| Target::parse(Some(&name.trim().to_owned())))
                    .collect::<Result<_>>()?;
            }
            "--no-asm" => asm = false,
            other => bail!("unknown argument {other:?}"),
        }
    }
    let root = workspace_root();
    for target in targets {
        let prefix = ffmpeg_prefix(target);
        let built = std::fs::read_to_string(prefix.join("recipe.txt")).ok();
        if built.as_deref() != Some(recipe(asm).as_str()) {
            eprintln!("building ffmpeg for {}", target.name());
            ffmpeg_for(target, asm)?;
        }
        let triple = target.triple();
        let mut linked = cargo();
        linked
            .current_dir(root)
            .args(["build", "--release", "--locked", "-p", "tgradish"])
            .args(["--features", "linked-static", "--target", triple])
            .env("PKG_CONFIG_PATH", prefix.join("lib/pkgconfig"))
            .env("PKG_CONFIG_ALL_STATIC", "1");
        if target.is_windows() {
            let stubs = root.join("xtask/mingw-stubs");
            linked
                .env("PKG_CONFIG_ALLOW_CROSS", "1")
                .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER", "x86_64-w64-mingw32-gcc")
                .env("CC_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc")
                .env("AR_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-ar")
                // the stubs come last, so they only fill in headers MinGW lacks
                .env(
                    "BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnu",
                    format!("-I/usr/x86_64-w64-mingw32/include -I{}", stubs.display()),
                )
                .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS", "-C link-arg=-static");
        }
        run(&mut linked)?;
        package(target, false, None)?;
        // the build without ffmpeg goes where the linked one was, so after
        // that one is packed
        if !target.is_windows() {
            run(cargo().current_dir(root).args([
                "build",
                "--release",
                "--locked",
                "-p",
                "tgradish",
                "--target",
                triple,
            ]))?;
            package(target, true, None)?;
        }
    }
    let out = root.join("target/release-artifacts");
    println!("\nin {}:", out.display());
    let mut entries: Vec<_> = std::fs::read_dir(&out)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        for name in ["tgradish", "tgradish.exe"] {
            let binary = entry.path().join(name);
            if let Ok(metadata) = std::fs::metadata(&binary) {
                println!("  {} ({:.1} MB)", binary.display(), metadata.len() as f64 / 1e6);
            }
        }
    }
    Ok(())
}

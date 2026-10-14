//! Every release variant built and packed on this machine, as the release
//! workflow makes them, so they can be tried by hand before a tag sends
//! them out.

use anyhow::{Context, Result, bail};

use crate::build::build;
use crate::ffmpeg::ensure_ffmpeg;
use crate::package::package;
use crate::{Target, workspace_root};

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
        ensure_ffmpeg(target, asm)?;
        build(target, false)?;
        package(target, false, None)?;
        // the build without ffmpeg goes where the linked one was, so after
        // that one is packed
        if !target.is_windows() {
            build(target, true)?;
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

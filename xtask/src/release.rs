//! Release automation: `local-release` builds and packages every release
//! variant locally, and `release` manages version bumps, consistency checks,
//! changelog notes, AUR updates, and crates.io publishing.

mod aur;
mod changelog;
mod crates;
mod version;

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use self::version::{Tree, Version};
use crate::build::build;
use crate::ffmpeg::ensure_ffmpeg;
use crate::package::{glibc_version, package};
use crate::{Target, cargo, output, run, rustc, workspace_root};

const USAGE: &str = "\
usage: cargo xtask release prepare X.Y.Z [--tgs X.Y.Z]
       cargo xtask release check [--tag vX.Y.Z]
       cargo xtask release notes [X.Y.Z]
       cargo xtask release aur [--push]
       cargo xtask release crates [--dry-run]";

/// The `release` task: one of its steps, as its arguments ask.
pub fn release(args: &[String]) -> Result<()> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("prepare") => prepare(rest),
        Some("check") => check(rest),
        Some("notes") => notes(rest),
        Some("aur") => aur::aur(rest),
        Some("crates") => crates::crates(rest),
        _ => bail!("{USAGE}"),
    }
}

/// The files the version is written in, as they are now.
fn read_tree(root: &Path) -> Result<Tree> {
    version::paths()
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(root.join(&path))
                .with_context(|| format!("could not read {path}"))?;
            Ok((path, text))
        })
        .collect()
}

/// Fails, saying where, unless every place agrees on the version, and
/// with `tag` where given.
fn ensure_agrees(tree: &Tree, tag: Option<&str>) -> Result<()> {
    let problems = version::problems(tree, tag)?;
    ensure!(problems.is_empty(), "version mismatch:\n  {}", problems.join("\n  "));
    Ok(())
}

/// `release check`: [`ensure_agrees`], with the tag the release is made
/// from where given.
fn check(args: &[String]) -> Result<()> {
    let tag = match args {
        [] => None,
        [flag, tag] if flag == "--tag" => Some(tag.as_str()),
        _ => bail!("usage: cargo xtask release check [--tag vX.Y.Z]"),
    };
    let tree = read_tree(workspace_root())?;
    ensure_agrees(&tree, tag)?;
    let version = version::app_version(&tree)?;
    println!("{version}, tgradish-tgs {}: all versions match", version::tgs_version(&tree)?);
    Ok(())
}

/// `release notes`: the changelog's section for a version, Cargo.toml's
/// unless given, which the release workflow drafts the release with.
fn notes(args: &[String]) -> Result<()> {
    let tree = read_tree(workspace_root())?;
    let version = match args {
        [] => version::app_version(&tree)?,
        [version] => Version::parse(version)?,
        _ => bail!("usage: cargo xtask release notes [X.Y.Z]"),
    };
    print!("{}", changelog::notes(&tree[version::CHANGELOG], version)?);
    Ok(())
}

/// `release prepare`: the new version written everywhere, the changelog's
/// Unreleased dated as its section, and the lock files refreshed, from a
/// tree with nothing uncommitted, so that `git diff` shows only this; then
/// what to commit, tag and push.
fn prepare(args: &[String]) -> Result<()> {
    let usage = "usage: cargo xtask release prepare X.Y.Z [--tgs X.Y.Z]";
    let mut version = None;
    let mut tgs = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--tgs" => tgs = Some(Version::parse(iter.next().context(usage)?)?),
            other if version.is_none() && !other.starts_with('-') => {
                version = Some(Version::parse(other)?)
            }
            other => bail!("unknown argument {other:?}\n{usage}"),
        }
    }
    let version = version.context(usage)?;
    let root = workspace_root();
    let git = || {
        let mut git = Command::new("git");
        git.current_dir(root);
        git
    };
    let status = output(git().args(["status", "--porcelain"]))?;
    ensure!(status.is_empty(), "commit or stash uncommitted changes first:\n{}", status.trim_end());

    let before = read_tree(root)?;
    let mut tree = before.clone();
    let date = output(Command::new("date").arg("+%F"))?;
    version::prepare(&mut tree, version, tgs, date.trim())?;
    // release builds take the newest stable Rust, which is the one here
    // where rustup keeps it up to date
    let pinned = version::toolchain(&tree)?;
    let here = output(rustc().arg("-V"))?;
    let here = here.split_whitespace().nth(1).and_then(|here| Version::parse(here).ok());
    let rust = match here {
        Some(here) if here > pinned => {
            version::set_toolchain(&mut tree, here)?;
            format!("updated from {pinned} to {here} (local stable Rust)")
        }
        _ => format!(
            "kept at {pinned} (if a newer stable Rust is available, run `rustup update` and raise it)"
        ),
    };
    for (path, text) in &tree {
        if before[path] != *text {
            std::fs::write(root.join(path), text)
                .with_context(|| format!("could not write {path}"))?;
        }
    }
    run(cargo().current_dir(root).args(["update", "--workspace"]))?;
    run(cargo().current_dir(root).args([
        "update",
        "--workspace",
        "--manifest-path",
        "crates/tgs-lab/Cargo.toml",
    ]))?;
    ensure_agrees(&read_tree(root)?, Some(&format!("v{version}")))
        .context("run `git checkout -- .` to revert")?;

    println!("\nUpdated all versions to {version} and dated Unreleased in CHANGELOG.md.");
    println!("RUSTUP_TOOLCHAIN in {}: {rust}.", version::WORKFLOW);
    tgs_changes(root, tgs, &tree)?;
    println!(
        "\nReview git diff, then run:\n\n  \
         git commit -am \"Release {version}\"\n  \
         git push origin main\n  \
         gh workflow run release.yml --ref main\n\n\
         Once that run passes, tag the commit:\n\n  \
         git tag -a v{version} -m \"tgradish {version}\"\n  \
         git push origin v{version}\n\n\
         The release workflow creates a draft release. Publish it on GitHub, then run:\n\n  \
         cargo xtask release aur --push\n  \
         cargo xtask release crates"
    );
    Ok(())
}

/// Says whether tgradish-tgs changed since the last release, so whether it
/// needs a version of its own, unless `tgs` gave it one.
fn tgs_changes(root: &Path, tgs: Option<Version>, tree: &Tree) -> Result<()> {
    let now = version::tgs_version(tree)?;
    if tgs.is_some() {
        println!("tgradish-tgs: updated to {now}.");
        return Ok(());
    }
    let Ok(tag) = output(Command::new("git").current_dir(root).args([
        "describe",
        "--tags",
        "--abbrev=0",
        "--match",
        "v*",
    ])) else {
        println!("tgradish-tgs: no previous release tag found; kept at {now}.");
        return Ok(());
    };
    let tag = tag.trim();
    let changed = output(Command::new("git").current_dir(root).args([
        "diff",
        "--stat",
        tag,
        "HEAD",
        "--",
        "crates/tgs",
    ]))?;
    if changed.trim().is_empty() {
        println!("tgradish-tgs: unchanged since {tag}; kept at {now}.");
    } else {
        println!(
            "\nWARNING: crates/tgs changed since {tag}, but its version is still {now} \
             (crates.io rejects re-uploading an existing version):\n{changed}\
             If packaged files changed, revert with `git checkout -- .` and re-run with \
             `--tgs` set to a version above {now}."
        );
    }
    Ok(())
}

/// What `url` holds, or nothing where it is missing.
fn fetch(url: &str) -> Result<Option<String>> {
    let out = output(Command::new("curl").args(["-sSL", "--write-out", "\n%{http_code}", url]))?;
    let (body, code) = out.trim_end().rsplit_once('\n').unwrap_or(("", out.trim_end()));
    match code {
        "200" => Ok(Some(body.to_owned())),
        "404" => Ok(None),
        _ => bail!("{url} returned HTTP {code}"),
    }
}

/// Builds every release variant of `targets` and packs them into
/// `target/release-artifacts`, building ffmpeg first where its libraries
/// are missing or from another recipe. The release workflow builds each
/// target with this too, so a local run makes the files a release has.
/// With `--max-glibc`, fails if a glibc build needs a newer glibc.
pub fn local_release(args: &[String]) -> Result<()> {
    let mut targets = vec![Target::host(), Target::WindowsX64];
    let mut asm = true;
    let mut max_glibc = None;
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
            "--max-glibc" => {
                let max = iter.next().context("--max-glibc needs a version, like 2.28")?;
                max_glibc = Some(glibc_version(max)?);
            }
            other => bail!("unknown argument {other:?}"),
        }
    }
    let root = workspace_root();
    for target in targets {
        let glibc = !target.is_windows() && !target.is_musl();
        // musl and Windows builds have no glibc to check
        let max_glibc = max_glibc.filter(|_| glibc);
        // only builds for this machine run here
        let runs = target == Target::host();
        ensure_ffmpeg(target, asm)?;
        build(target, false)?;
        let linked = package(target, false, max_glibc)?;
        if runs {
            smoke_test(&linked)?;
        }
        // the build without ffmpeg goes where the linked one was, so after
        // that one is packed; only for glibc, as the AUR package tgradish-bin
        // uses it
        if glibc {
            build(target, true)?;
            let system = package(target, true, max_glibc)?;
            // it runs the system's ffmpeg, which the release workflow's
            // build containers lack
            if runs && Command::new("ffmpeg").arg("-version").output().is_ok() {
                smoke_test(&system)?;
            }
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

/// Runs `.github/smoke.sh`, which every release test job runs too, on a
/// packed binary, in `target/smoke`, where it leaves the files it made.
fn smoke_test(binary: &Path) -> Result<()> {
    let root = workspace_root();
    let dir = root.join("target/smoke");
    std::fs::create_dir_all(&dir)?;
    run(Command::new("sh").current_dir(&dir).arg(root.join(".github/smoke.sh")).arg(binary))
}

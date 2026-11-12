//! The licences of the Rust crates in tgradish, which every binary carries
//! (see `crates/core/build.rs`). cargo-about lists them; the list is
//! committed, so builds need no network or cargo-about, and CI checks it
//! after dependencies change. Its sections are sorted here so a reader can
//! find a licence and its crates.
//!
//! tgradish's own licence is `LICENSE.txt` at the top of the repository,
//! and every published crate has a copy, kept the same here: crates.io
//! packs a crate's own folder and nothing above it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::{NOTICES, cargo, run, workspace_root};

/// Width of the rule above each licence's section, which `xtask/about.hbs`
/// draws: the two must match for the sections to be found.
const RULE_WIDTH: usize = 80;

/// The list has more sections than this only if cargo-about found the
/// crates' own licence files. With them, each copyright holder's text is a
/// section of its own, well over a hundred; without them, crates share the
/// bare text of their licence, about one section for each licence
/// `xtask/about.toml` accepts.
const MOST_SECTIONS_WITHOUT_FILES: usize = 40;

/// The folders of the crates that are published, which is every one of
/// `crates/` that doesn't say otherwise.
fn published_crates(root: &Path) -> Result<Vec<PathBuf>> {
    let mut crates = Vec::new();
    for entry in std::fs::read_dir(root.join("crates"))? {
        let dir = entry?.path();
        let Ok(manifest) = std::fs::read_to_string(dir.join("Cargo.toml")) else { continue };
        if !manifest.lines().any(|line| line.trim() == "publish = false") {
            crates.push(dir);
        }
    }
    crates.sort();
    Ok(crates)
}

/// Writes or checks the copy of `LICENSE.txt` in each published crate.
fn own_licence(root: &Path, check: bool) -> Result<()> {
    let name = "LICENSE.txt";
    let text = std::fs::read(root.join(name)).with_context(|| format!("could not read {name}"))?;
    let crates = published_crates(root)?;
    ensure!(!crates.is_empty(), "found no published crates in crates/");
    for dir in crates {
        let copy = dir.join(name);
        if check {
            ensure!(
                std::fs::read(&copy).ok().as_ref() == Some(&text),
                "{} isn't a copy of {name}: run cargo xtask licenses and commit it",
                copy.display()
            );
        } else {
            std::fs::write(&copy, &text)?;
        }
    }
    Ok(())
}

/// Writes or checks `THIRD-PARTY-CRATES.txt` in [`NOTICES`]: the crates of
/// every target and build, ffmpeg linked in or not; and the published
/// crates' copies of tgradish's own licence.
pub fn crate_licenses(args: &[String]) -> Result<()> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => bail!("usage: cargo xtask licenses [--check]"),
    };
    let root = workspace_root();
    own_licence(root, check)?;
    let file = root.join(NOTICES).join("THIRD-PARTY-CRATES.txt");
    let fresh = root.join("target/THIRD-PARTY-CRATES.txt");
    run(cargo()
        .current_dir(root)
        .args(["about", "generate", "--locked", "--fail", "-c", "xtask/about.toml"])
        .args(["-m", "crates/cli/Cargo.toml", "--features", "linked-static"])
        .arg("xtask/about.hbs")
        .arg("-o")
        .arg(&fresh))?;
    let text = sorted_sections(&std::fs::read_to_string(&fresh)?);
    // crates' own licence files say who holds the copyright; without them
    // (see the crate's notes on CARGO_HOME) every crate gets the bare
    // licence text
    ensure!(
        text.matches("\n----").count() > MOST_SECTIONS_WITHOUT_FILES,
        "cargo-about found almost no licence files: is a .gitignore hiding CARGO_HOME?"
    );
    if check {
        let committed = std::fs::read_to_string(&file).unwrap_or_default();
        ensure!(
            committed == text,
            "{} is out of date: run cargo xtask licenses and commit it",
            file.display()
        );
        println!("{} is up to date", file.display());
    } else {
        std::fs::write(&file, text)?;
        println!("{}", file.display());
    }
    Ok(())
}

/// cargo-about's list with its sections in the order they read in, by
/// licence and then crates, and each one's crates by name: it lists those
/// of a licence in no order a reader could follow.
fn sorted_sections(text: &str) -> String {
    let rule = format!("\n{}\n", "-".repeat(RULE_WIDTH));
    let mut parts = text.split(rule.as_str());
    let intro = parts.next().unwrap_or_default().to_owned();
    let mut sections: Vec<String> = parts
        .map(|section| {
            let (head, body) = section.split_once("\n\n").unwrap_or((section, ""));
            let mut lines = head.lines();
            let title = lines.next().unwrap_or_default();
            let mut crates: Vec<&str> = lines.collect();
            crates.sort_by_key(|line| line.trim().to_lowercase());
            // a crate with two copies of a licence, as libmimalloc-sys has
            // for each mimalloc it bundles, is named once
            crates.dedup();
            format!("{title}\n{}\n\n{body}", crates.join("\n"))
        })
        .collect();
    sections.sort_by_cached_key(|section| {
        let head = section.split_once("\n\n").map_or(section.as_str(), |(head, _)| head);
        head.lines().map(|line| line.trim().to_lowercase()).collect::<Vec<_>>()
    });
    std::iter::once(intro).chain(sections).collect::<Vec<_>>().join(&rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_published_crates() {
        let names: Vec<String> = published_crates(workspace_root())
            .unwrap()
            .iter()
            .map(|dir| dir.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["cli", "core", "frames", "gui", "tgs"]);
    }
}

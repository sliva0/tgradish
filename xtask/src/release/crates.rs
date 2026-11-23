//! Publishes unpublished crate versions to crates.io in dependency order after
//! interactive confirmation (publishing is permanent).

use std::io::Write;

use anyhow::{Context, Result, bail};

use super::version::{self, Version};
use super::{ensure_agrees, fetch, read_tree};
use crate::{cargo, run, workspace_root};

/// The published crates, each after those it depends on.
const CRATES: [&str; 4] = ["tgradish-tgs", "tgradish-core", "tgradish-gui", "tgradish"];

/// `release crates`: the crates whose version crates.io lacks, published
/// once asked, or packaged and built as published with `--dry-run`.
pub fn crates(args: &[String]) -> Result<()> {
    let dry_run = match args {
        [] => false,
        [flag] if flag == "--dry-run" => true,
        _ => bail!("usage: cargo xtask release crates [--dry-run]"),
    };
    let root = workspace_root();
    let tree = read_tree(root)?;
    ensure_agrees(&tree, None)?;
    let (app, tgs) = (version::app_version(&tree)?, version::tgs_version(&tree)?);
    let mut new = Vec::new();
    for name in CRATES {
        let version = if name == "tgradish-tgs" { tgs } else { app };
        let url = format!("https://index.crates.io/{}", index_path(name));
        let published = fetch(&url)?.is_some_and(|index| has_version(&index, version));
        println!("{name} {version}: {}", if published { "already on crates.io" } else { "new" });
        if !published {
            new.push(name);
        }
    }
    if new.is_empty() {
        println!("All crates are already published at their current versions.");
        return Ok(());
    }
    let mut publish = cargo();
    publish.current_dir(root).args(["publish", "--locked"]);
    for name in &new {
        publish.args(["-p", name]);
    }
    if dry_run {
        publish.arg("--dry-run");
    } else {
        print!("Publish {} permanently to crates.io? [y/N] ", new.join(", "));
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).context("could not read confirmation")?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            println!("Nothing published.");
            return Ok(());
        }
    }
    run(&mut publish)
}

/// Where crates.io's sparse index lists a crate's versions.
fn index_path(name: &str) -> String {
    let name = name.to_lowercase();
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

/// Whether an index file, a JSON object a line, lists `version`.
fn has_version(index: &str, version: Version) -> bool {
    let version = version.to_string();
    index
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|entry| entry["vers"].as_str() == Some(&version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_paths() {
        assert_eq!(index_path("tgradish-tgs"), "tg/ra/tgradish-tgs");
        assert_eq!(index_path("Abc"), "3/a/abc");
        assert_eq!(index_path("ab"), "2/ab");
    }

    #[test]
    fn versions_in_the_index() {
        let index = "{\"name\":\"tgradish\",\"vers\":\"2.0.0\",\"deps\":[]}\n\
                     {\"name\":\"tgradish\",\"vers\":\"2.0.1\",\"yanked\":true}\n";
        assert!(has_version(index, Version::parse("2.0.0").unwrap()));
        // yanked or not, crates.io won't take the version again
        assert!(has_version(index, Version::parse("2.0.1").unwrap()));
        assert!(!has_version(index, Version::parse("2.0.10").unwrap()));
    }
}

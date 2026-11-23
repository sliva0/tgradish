//! Reads, updates, and verifies version numbers across the repository.
//!
//! The main workspace version (`tgradish`, `tgradish-core`, `tgradish-gui`,
//! `xtask`) appears in `[workspace.package]`, `[workspace.dependencies]`, both
//! lock files, each AUR `PKGBUILD`, `CHANGELOG.md`, and
//! `crates/core/licenses/THIRD-PARTY-CRATES.txt` (which lists workspace crates
//! with their versions). `tgradish-tgs` has an independent version in its
//! manifest, `[workspace.dependencies]`, both lock files, and
//! `THIRD-PARTY-CRATES.txt`. Nix reads `Cargo.toml` directly, and built
//! binaries and packages take their version from Cargo at build time.
//!
//! Files are edited line-by-line to preserve surrounding formatting.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use anyhow::{Context, Result, bail, ensure};

use super::changelog;

/// A version as X.Y.Z. Pre-releases are left out: the AUR's `pkgver`
/// can't hold their hyphen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    pub fn parse(text: &str) -> Result<Version> {
        let number = |part: &str| {
            let digits = !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
            // semver has no leading zeros
            (digits && (part == "0" || !part.starts_with('0'))).then(|| part.parse().ok()).flatten()
        };
        let parts: Vec<Option<u64>> = text.split('.').map(number).collect();
        match parts[..] {
            [Some(major), Some(minor), Some(patch)] => Ok(Version { major, minor, patch }),
            _ => bail!("{text:?} is not a valid X.Y.Z version"),
        }
    }

    /// A Rust version, as `rust-version` gives it without the patch.
    fn parse_rust(text: &str) -> Result<Version> {
        match text.matches('.').count() {
            1 => Version::parse(&format!("{text}.0")),
            _ => Version::parse(text),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub const MANIFEST: &str = "Cargo.toml";
pub const TGS_MANIFEST: &str = "crates/tgs/Cargo.toml";
pub const LOCK: &str = "Cargo.lock";
pub const LAB_LOCK: &str = "crates/tgs-lab/Cargo.lock";
pub const CHANGELOG: &str = "CHANGELOG.md";
pub const WORKFLOW: &str = ".github/workflows/release.yml";
/// What `cargo xtask licenses` writes, with cargo-about, which `prepare`
/// can't count on: other versions of it word the list differently, and it
/// skips what a `.gitignore` around cargo's home hides.
pub const CRATE_LICENCES: &str = "crates/core/licenses/THIRD-PARTY-CRATES.txt";

/// The packages in `packaging/aur`, each a repository on the AUR.
pub const AUR_PACKAGES: [&str; 3] = ["tgradish", "tgradish-bin", "tgradish-static-bin"];

/// The crates that share the application's version and are asked for by
/// it in `[workspace.dependencies]`.
const APP_DEPENDENCIES: [&str; 2] = ["tgradish-core", "tgradish-gui"];

const TGS: &str = "tgradish-tgs";

/// The workspace's crates that the list of crates' licences names, each
/// as `  name version` on a line of its own under its licence.
const LICENSED_CRATES: [&str; 4] = ["tgradish", "tgradish-core", "tgradish-gui", TGS];

pub fn pkgbuild(package: &str) -> String {
    format!("packaging/aur/{package}/PKGBUILD")
}

/// Every file a version is written in, by its path from the repository's
/// top.
pub fn paths() -> Vec<String> {
    let mut paths: Vec<String> =
        [MANIFEST, TGS_MANIFEST, LOCK, LAB_LOCK, CHANGELOG, WORKFLOW, CRATE_LICENCES]
            .map(str::to_owned)
            .into();
    for package in AUR_PACKAGES {
        paths.push(pkgbuild(package));
    }
    paths
}

/// The files of [`paths`], read or to be written.
pub type Tree = BTreeMap<String, String>;

/// A value written on one line: the line in `table` (`[workspace.package]`,
/// or anywhere in the file without one) that starts with `start`, once
/// indentation is left aside, and the value after `key` on it, up to a
/// quote or the line's end.
struct Field<'a> {
    table: Option<&'a str>,
    start: &'a str,
    key: &'a str,
}

const WORKSPACE_VERSION: Field =
    Field { table: Some("[workspace.package]"), start: "version = ", key: "version = \"" };
const PACKAGE_VERSION: Field =
    Field { table: Some("[package]"), start: "version = ", key: "version = \"" };
const RUST_VERSION: Field = Field {
    table: Some("[workspace.package]"),
    start: "rust-version = ",
    key: "rust-version = \"",
};
const REPOSITORY: Field =
    Field { table: Some("[workspace.package]"), start: "repository = ", key: "repository = \"" };
const PKGVER: Field = Field { table: None, start: "pkgver=", key: "pkgver=" };
const PKGREL: Field = Field { table: None, start: "pkgrel=", key: "pkgrel=" };
// what release builds are built with, see the workflow
const TOOLCHAIN: Field =
    Field { table: None, start: "RUSTUP_TOOLCHAIN: ", key: "RUSTUP_TOOLCHAIN: \"" };

/// The requirement of `[workspace.dependencies]` on a crate of the
/// workspace, as exact as they all are: the version after its `=`.
fn dependency(name: &str) -> Field<'_> {
    Field { table: Some("[workspace.dependencies]"), start: name, key: "version = \"=" }
}

impl Field<'_> {
    /// The line the value is on, as an index of `lines`, and where on it
    /// the value is. Fails unless exactly one line has it.
    fn find(&self, lines: &[&str]) -> Result<(usize, Range<usize>)> {
        let mut in_table = self.table.is_none();
        let mut found = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(table) = self.table
                && trimmed.starts_with('[')
            {
                in_table = trimmed.trim_end() == table;
                continue;
            }
            // `name = ` alone, not a longer name that starts alike
            let starts = trimmed.strip_prefix(self.start).is_some_and(|rest| {
                self.start.ends_with([' ', '=']) || rest.starts_with([' ', '='])
            });
            if !in_table || !starts {
                continue;
            }
            let Some(at) = line.find(self.key) else { continue };
            let value = at + self.key.len();
            let end = line[value..].find(['"', '\r', '\n']).map_or(line.len(), |end| value + end);
            found.push((index, value..end));
        }
        match found.len() {
            1 => Ok(found.pop().unwrap()),
            0 => bail!("found no {}{}", self.start, self.table.unwrap_or("")),
            _ => bail!("found {} lines of {}", found.len(), self.start),
        }
    }

    fn get<'t>(&self, text: &'t str) -> Result<&'t str> {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let (index, range) = self.find(&lines)?;
        Ok(&lines[index][range])
    }

    fn set(&self, text: &str, value: &str) -> Result<String> {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let (index, range) = self.find(&lines)?;
        let line = &lines[index];
        let changed = format!("{}{value}{}", &line[..range.start], &line[range.end..]);
        let mut out = String::with_capacity(text.len() + value.len());
        for (i, line) in lines.iter().enumerate() {
            out.push_str(if i == index { &changed } else { line });
        }
        Ok(out)
    }
}

fn file<'t>(tree: &'t Tree, path: &str) -> Result<&'t str> {
    tree.get(path).map(String::as_str).with_context(|| format!("{path} is missing"))
}

fn get<'t>(tree: &'t Tree, path: &str, field: &Field) -> Result<&'t str> {
    field.get(file(tree, path)?).with_context(|| format!("in {path}"))
}

fn set(tree: &mut Tree, path: &str, field: &Field, value: &str) -> Result<()> {
    let text = field.set(file(tree, path)?, value).with_context(|| format!("in {path}"))?;
    tree.insert(path.to_owned(), text);
    Ok(())
}

/// The application's version, `[workspace.package]`'s.
pub fn app_version(tree: &Tree) -> Result<Version> {
    Version::parse(get(tree, MANIFEST, &WORKSPACE_VERSION)?)
}

/// tgradish-tgs's own version.
pub fn tgs_version(tree: &Tree) -> Result<Version> {
    Version::parse(get(tree, TGS_MANIFEST, &PACKAGE_VERSION)?)
}

/// Where the repository is, which releases are under.
pub fn repository(tree: &Tree) -> Result<String> {
    Ok(get(tree, MANIFEST, &REPOSITORY)?.trim_end_matches('/').to_owned())
}

/// The Rust release builds are built with, which the workflow pins.
pub fn toolchain(tree: &Tree) -> Result<Version> {
    Version::parse(get(tree, WORKFLOW, &TOOLCHAIN)?)
}

pub fn set_toolchain(tree: &mut Tree, rust: Version) -> Result<()> {
    set(tree, WORKFLOW, &TOOLCHAIN, &rust.to_string())
}

/// A package's `pkgver-pkgrel`, as its PKGBUILD says.
pub fn aur_release(pkgbuild: &str) -> Result<String> {
    Ok(format!("{}-{}", PKGVER.get(pkgbuild)?, PKGREL.get(pkgbuild)?))
}

/// The packages a lock file has from the workspace or beside it, which
/// have no `source`, and their versions.
fn local_packages(lock: &str) -> Vec<(&str, &str)> {
    fn value<'a>(block: &'a str, key: &str) -> Option<&'a str> {
        block
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(" = \"")?.strip_suffix('"'))
    }
    lock.split("[[package]]")
        .skip(1)
        .filter(|block| !block.lines().any(|line| line.starts_with("source = ")))
        .filter_map(|block| Some((value(block, "name")?, value(block, "version")?)))
        .collect()
}

/// The lines of the list of crates' licences that name a crate of the
/// workspace: each line's index, the crate and the version it gives.
fn licensed_crates(text: &str) -> Vec<(usize, &str, &str)> {
    text.split_inclusive('\n')
        .enumerate()
        .filter_map(|(index, line)| {
            let (name, version) = line.strip_prefix("  ")?.trim_end().split_once(' ')?;
            let ours = LICENSED_CRATES.contains(&name) && Version::parse(version).is_ok();
            ours.then_some((index, name, version))
        })
        .collect()
}

/// `tree` with the version `version` everywhere, and tgradish-tgs's `tgs`
/// where given, dated `date` in the changelog; the lock files are left to
/// cargo. Fails unless both rise, or where the changelog says nothing new.
pub fn prepare(tree: &mut Tree, version: Version, tgs: Option<Version>, date: &str) -> Result<()> {
    let previous = app_version(tree)?;
    ensure!(version > previous, "{version} is not greater than current version {previous}");
    if let Some(tgs) = tgs {
        let previous = tgs_version(tree)?;
        ensure!(tgs > previous, "{tgs} is not greater than current {TGS} version {previous}");
        set(tree, TGS_MANIFEST, &PACKAGE_VERSION, &tgs.to_string())?;
        set(tree, MANIFEST, &dependency(TGS), &tgs.to_string())?;
    }
    let changelog = changelog::release(file(tree, CHANGELOG)?, version, previous, date)?;
    tree.insert(CHANGELOG.to_owned(), changelog);
    set(tree, MANIFEST, &WORKSPACE_VERSION, &version.to_string())?;
    for name in APP_DEPENDENCIES {
        set(tree, MANIFEST, &dependency(name), &version.to_string())?;
    }
    for package in AUR_PACKAGES {
        let path = pkgbuild(package);
        set(tree, &path, &PKGVER, &version.to_string())?;
        set(tree, &path, &PKGREL, "1")?;
    }
    // as cargo-about would list them now
    let tgs = tgs_version(tree)?;
    let text = file(tree, CRATE_LICENCES)?;
    let listed = licensed_crates(text);
    let text: String = text
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| match listed.iter().find(|(at, _, _)| *at == index) {
            Some((_, name, _)) => {
                let expected = if *name == TGS { tgs } else { version };
                format!("  {name} {expected}{}", &line[line.trim_end().len()..])
            }
            None => line.to_owned(),
        })
        .collect();
    tree.insert(CRATE_LICENCES.to_owned(), text);
    Ok(())
}

/// Every place the version is written that disagrees with the others, or
/// with `tag`, the release's tag, where given; and what the changelog and
/// the toolchain release builds use lack.
pub fn problems(tree: &Tree, tag: Option<&str>) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let version = app_version(tree)?;
    let tgs = tgs_version(tree)?;
    if let Some(tag) = tag
        && tag != format!("v{version}")
    {
        problems.push(format!("tag is {tag}, but {MANIFEST} has {version}"));
    }
    fn expect(problems: &mut Vec<String>, place: String, found: &str, expected: Version) {
        if found != expected.to_string() {
            problems.push(format!("{place} is {found}, not {expected}"));
        }
    }
    for (name, expected) in
        APP_DEPENDENCIES.map(|name| (name, version)).into_iter().chain([(TGS, tgs)])
    {
        let found = get(tree, MANIFEST, &dependency(name))?;
        expect(&mut problems, format!("{MANIFEST} dependency on {name}"), found, expected);
    }
    for package in AUR_PACKAGES {
        let path = pkgbuild(package);
        let pkgbuild = file(tree, &path)?;
        expect(&mut problems, format!("{path} pkgver"), PKGVER.get(pkgbuild)?, version);
    }
    for path in [LOCK, LAB_LOCK] {
        let lock = file(tree, path)?;
        let packages = local_packages(lock);
        if packages.is_empty() {
            problems.push(format!("{path} contains no workspace crates"));
        }
        // tgs-lab is a workspace of its own, versioned apart
        for (name, found) in packages.into_iter().filter(|(name, _)| *name != "tgs-lab") {
            let expected = if name == TGS { tgs } else { version };
            if found != expected.to_string() {
                problems.push(format!(
                    "{path} has {name} {found}, not {expected}; run `cargo update --workspace`"
                ));
            }
        }
    }
    let listed = licensed_crates(file(tree, CRATE_LICENCES)?);
    for name in LICENSED_CRATES {
        if !listed.iter().any(|(_, found, _)| *found == name) {
            problems
                .push(format!("{CRATE_LICENCES} does not list {name}; run `cargo xtask licenses`"));
        }
    }
    for (_, name, found) in listed {
        let expected = if name == TGS { tgs } else { version };
        if found != expected.to_string() {
            problems.push(format!(
                "{CRATE_LICENCES} lists {name} {found}, not {expected}; run `cargo xtask licenses`"
            ));
        }
    }
    problems.extend(changelog::problems(file(tree, CHANGELOG)?, version));
    let rust = Version::parse_rust(get(tree, MANIFEST, &RUST_VERSION)?)?;
    let built = toolchain(tree)?;
    if built < rust {
        problems
            .push(format!("{WORKFLOW} uses Rust {built}, which is older than rust-version {rust}"));
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// The files as they were at 2.0.0, cut down to what holds versions and
    /// what lies around it.
    fn tree() -> Tree {
        let mut tree = Tree::new();
        let mut add = |path: &str, text: &str| tree.insert(path.to_owned(), text.to_owned());
        add(
            MANIFEST,
            "[workspace]\nmembers = [\"crates/*\", \"xtask\"]\n\n\
             [workspace.package]\nversion = \"2.0.0\"\nedition = \"2024\"\n\
             rust-version = \"1.95\"\nrepository = \"https://github.com/sliva0/tgradish\"\n\n\
             [workspace.dependencies]\n\
             tgradish-core = { path = \"crates/core\", version = \"=2.0.0\" }\n\
             tgradish-corpus = { path = \"crates/corpus\" }\n\
             tgradish-gui = { path = \"crates/gui\", version = \"=2.0.0\" }\n\
             tgradish-tgs = { path = \"crates/tgs\", version = \"=0.1.0\" }\n\
             anyhow = \"1.0.104\"\n\n\
             [profile.release]\nlto = \"thin\"\n",
        );
        add(
            TGS_MANIFEST,
            "[package]\nname = \"tgradish-tgs\"\n# its own\nversion = \"0.1.0\"\n\
             edition.workspace = true\n\n[dependencies]\nflate2 = { version = \"1.1.10\" }\n",
        );
        let lock = |packages: &[(&str, &str)]| {
            let mut lock = String::from("# generated by cargo\nversion = 4\n");
            lock.push_str("\n[[package]]\nname = \"anyhow\"\nversion = \"2.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n");
            for (name, version) in packages {
                lock.push_str(&format!(
                    "\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\ndependencies = [\n \"anyhow\",\n]\n"
                ));
            }
            lock
        };
        add(
            LOCK,
            &lock(&[
                ("tgradish", "2.0.0"),
                ("tgradish-core", "2.0.0"),
                ("tgradish-tgs", "0.1.0"),
                ("xtask", "2.0.0"),
            ]),
        );
        add(
            LAB_LOCK,
            &lock(&[("tgradish-corpus", "2.0.0"), ("tgradish-tgs", "0.1.0"), ("tgs-lab", "0.0.0")]),
        );
        add(
            CHANGELOG,
            "# Changelog\n\n## [Unreleased]\n\n### Fixed\n\n- A bug.\n\n## [2.0.0] - 2026-11-16\n\n\
             Rust.\n\n[Unreleased]: https://github.com/sliva0/tgradish/compare/v2.0.0...HEAD\n\
             [2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0\n",
        );
        add(
            WORKFLOW,
            "jobs:\n  build:\n    env:\n      RUSTUP_TOOLCHAIN: \"1.99.0\"\n    steps:\n      - run: rustup toolchain install \"$RUSTUP_TOOLCHAIN\"\n",
        );
        add(
            CRATE_LICENCES,
            "The crates in tgradish, and their licences.\n\n\
             ----\nApache License 2.0, used by:\n  anyhow 1.0.104\n  tgradish-gui-extra 2.0.0\n\n\
             Apache License\n  tgradish is not a crate here\n\n\
             ----\nMIT License, used by:\n  tgradish 2.0.0\n  tgradish-core 2.0.0\n  \
             tgradish-gui 2.0.0\n  tgradish-tgs 0.1.0\n\nMIT License\n\n\
             ----\nMIT License, used by:\n  tgradish-gui 2.0.0\n\nMIT License\n",
        );
        for package in AUR_PACKAGES {
            add(
                &pkgbuild(package),
                &format!(
                    "pkgname={package}\npkgver=2.0.0\npkgrel=2\nsource=(\"$pkgname-$pkgver.tar.gz::$url/v$pkgver.tar.gz\")\nsha256sums=('SKIP')\n"
                ),
            );
        }
        tree
    }

    /// What `cargo update --workspace` does to the lock files.
    fn update_locks(tree: &mut Tree, from: &str, to: &str) {
        for path in [LOCK, LAB_LOCK] {
            let lock = tree[path].replace(
                &format!("version = \"{from}\"\ndep"),
                &format!("version = \"{to}\"\ndep"),
            );
            tree.insert(path.to_owned(), lock);
        }
    }

    #[test]
    fn versions_are_x_y_z() {
        assert_eq!(version("2.10.0").to_string(), "2.10.0");
        assert!(version("2.10.0") > version("2.9.9"));
        for wrong in ["2.0", "2.0.0-rc.1", "v2.0.0", "02.0.0", "2.0.0.0", "", "2..0"] {
            assert!(Version::parse(wrong).is_err(), "{wrong}");
        }
        assert_eq!(Version::parse_rust("1.95").unwrap(), version("1.95.0"));
    }

    #[test]
    fn the_tree_agrees() {
        let tree = tree();
        assert_eq!(problems(&tree, None).unwrap(), Vec::<String>::new());
        assert_eq!(problems(&tree, Some("v2.0.0")).unwrap(), Vec::<String>::new());
        assert_eq!(problems(&tree, Some("v2.0.1")).unwrap().len(), 1);
        assert_eq!(repository(&tree).unwrap(), "https://github.com/sliva0/tgradish");
        assert_eq!(aur_release(&tree[&pkgbuild("tgradish")]).unwrap(), "2.0.0-2");
    }

    #[test]
    fn prepare_changes_the_version_everywhere_and_nothing_else() {
        let before = tree();
        let mut tree = before.clone();
        prepare(&mut tree, version("2.1.0"), None, "2026-11-20").unwrap();
        // the lock files are cargo's to change
        let problems_now = problems(&tree, Some("v2.1.0")).unwrap();
        assert!(
            problems_now.iter().all(|problem| problem.contains("cargo update")),
            "{problems_now:?}"
        );
        update_locks(&mut tree, "2.0.0", "2.1.0");
        assert_eq!(problems(&tree, Some("v2.1.0")).unwrap(), Vec::<String>::new());
        // every line that changed had the old version in it, but the
        // changelog's and the AUR's release, which starts again at 1
        for (path, text) in &tree {
            let old: Vec<&str> = before[path].lines().collect();
            let new: Vec<&str> = text.lines().collect();
            if path == CHANGELOG {
                continue;
            }
            assert_eq!(old.len(), new.len(), "{path}");
            for (old, new) in old.iter().zip(new) {
                if old != &new {
                    assert!(
                        old.contains("2.0.0") || old.contains("pkgrel"),
                        "{path}: {old:?} became {new:?}"
                    );
                    assert!(!new.contains("2.0.0"), "{path}: {new:?}");
                }
            }
        }
        assert!(
            tree[MANIFEST]
                .contains("tgradish-core = { path = \"crates/core\", version = \"=2.1.0\" }")
        );
        assert!(tree[MANIFEST].contains("[workspace.package]\nversion = \"2.1.0\"\n"));
        // the workspace's crates in the list of licences, each where it is,
        // and nothing that only looks like one
        let licences = &tree[CRATE_LICENCES];
        assert!(licences.contains(
            "  tgradish 2.1.0\n  tgradish-core 2.1.0\n  tgradish-gui 2.1.0\n  tgradish-tgs 0.1.0\n"
        ));
        assert!(licences.contains("used by:\n  tgradish-gui 2.1.0\n\nMIT License\n"));
        assert!(licences.contains("  tgradish-gui-extra 2.0.0\n"));
        assert!(licences.contains("  tgradish is not a crate here\n"));
        // tgs keeps its own
        assert!(tree[MANIFEST].contains("version = \"=0.1.0\""));
        assert_eq!(tree[TGS_MANIFEST], before[TGS_MANIFEST]);
        assert!(tree[&pkgbuild("tgradish")].contains("pkgver=2.1.0\npkgrel=1\n"));
        assert!(
            tree[CHANGELOG].contains("## [Unreleased]\n\n## [2.1.0] - 2026-11-20\n\n### Fixed\n")
        );
    }

    #[test]
    fn prepare_raises_tgs_where_asked() {
        let mut tree = tree();
        prepare(&mut tree, version("2.0.1"), Some(version("0.2.0")), "2026-11-20").unwrap();
        assert!(tree[TGS_MANIFEST].contains("# its own\nversion = \"0.2.0\"\n"));
        assert!(
            tree[MANIFEST]
                .contains("tgradish-tgs = { path = \"crates/tgs\", version = \"=0.2.0\" }")
        );
        assert!(tree[CRATE_LICENCES].contains("  tgradish-gui 2.0.1\n  tgradish-tgs 0.2.0\n"));
        // flate2's isn't the package's
        assert!(tree[TGS_MANIFEST].contains("flate2 = { version = \"1.1.10\" }"));
        let problems = problems(&tree, None).unwrap();
        assert!(
            problems.iter().any(|problem| problem.contains("tgradish-tgs 0.1.0, not 0.2.0")),
            "{problems:?}"
        );
    }

    #[test]
    fn prepare_refuses_what_isnt_a_release() {
        let original = tree();
        let refused = |version_text: &str, tgs: Option<&str>| {
            let mut tree = original.clone();
            let result = prepare(&mut tree, version(version_text), tgs.map(version), "2026-11-20");
            assert!(result.is_err(), "{version_text} {tgs:?}");
        };
        refused("2.0.0", None);
        refused("1.9.9", None);
        refused("2.0.1", Some("0.1.0"));
        let mut tree = original.clone();
        let empty = tree[CHANGELOG].replace("### Fixed\n\n- A bug.\n\n", "");
        tree.insert(CHANGELOG.to_owned(), empty);
        let error = prepare(&mut tree, version("2.0.1"), None, "2026-11-20").unwrap_err();
        assert!(error.to_string().contains("nothing under Unreleased"), "{error}");
    }

    #[test]
    fn problems_find_each_place_left_behind() {
        let original = tree();
        for (path, from, to) in [
            (
                MANIFEST,
                "version = \"=2.0.0\" }\ntgradish-corpus",
                "version = \"=2.0.1\" }\ntgradish-corpus",
            ),
            (MANIFEST, "version = \"=0.1.0\"", "version = \"=0.1.1\""),
            (
                LAB_LOCK,
                "\"tgradish-corpus\"\nversion = \"2.0.0\"",
                "\"tgradish-corpus\"\nversion = \"1.9.0\"",
            ),
            ("packaging/aur/tgradish-bin/PKGBUILD", "pkgver=2.0.0", "pkgver=2.0.1"),
            (CHANGELOG, "## [2.0.0] - 2026-11-16", "## [2.0.0]"),
            (WORKFLOW, "\"1.99.0\"", "\"1.90.0\""),
            (
                CRATE_LICENCES,
                "  tgradish-gui 2.0.0\n\nMIT License\n",
                "  tgradish-gui 1.0.0\n\nMIT License\n",
            ),
            (CRATE_LICENCES, "  tgradish-tgs 0.1.0\n", ""),
        ] {
            let mut tree = original.clone();
            let text = tree[path].replacen(from, to, 1);
            assert_ne!(text, tree[path], "{from}");
            tree.insert(path.to_owned(), text);
            let problems = problems(&tree, None).unwrap();
            assert_eq!(problems.len(), 1, "{path}: {from} -> {to}: {problems:?}");
        }
    }

    #[test]
    fn fields_are_found_by_their_whole_name() {
        let text = "[workspace.dependencies]\ntgradish-core-extra = { version = \"=1.0.0\" }\ntgradish-core = { version = \"=2.0.0\" }\n";
        assert_eq!(dependency("tgradish-core").get(text).unwrap(), "2.0.0");
        // a version under another table isn't the package's
        let text = "[package]\nname = \"a\"\n\n[dependencies.b]\nversion = \"1.0.0\"\n";
        assert!(PACKAGE_VERSION.get(text).is_err());
    }

    /// The packages named here are those in `packaging/aur`, each with a
    /// PKGBUILD, and the files of the version are all there.
    #[test]
    fn the_repository_has_every_file() {
        let root = crate::workspace_root();
        let mut packages: Vec<String> = std::fs::read_dir(root.join("packaging/aur"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        packages.sort();
        assert_eq!(packages, AUR_PACKAGES);
        for path in paths() {
            assert!(root.join(&path).is_file(), "{path}");
        }
        // the list of licences names each crate as this reads it
        let licences = std::fs::read_to_string(root.join(CRATE_LICENCES)).unwrap();
        let listed = licensed_crates(&licences);
        for name in LICENSED_CRATES {
            assert!(listed.iter().any(|(_, found, _)| *found == name), "{name}: {listed:?}");
        }
    }
}

//! The AUR packages: `packaging/aur` holds them as they are released, with
//! `SKIP` for checksums, and each is a git repository on the AUR. The
//! maintainer's clones are kept in `.aur/`, which git ignores; this brings
//! each up to its package in `packaging/aur`, with the checksums of what
//! it downloads, once the release is published, since until then its
//! files aren't there to download.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use super::version::{self, AUR_PACKAGES};
use super::{ensure_agrees, fetch, read_tree};
use crate::{output, run, workspace_root};

/// The branch the AUR builds from, its only one.
const BRANCH: &str = "master";

/// `release aur`: each package's clone brought up to `packaging/aur` and
/// committed, and with `--push` pushed.
pub fn aur(args: &[String]) -> Result<()> {
    let push = match args {
        [] => false,
        [flag] if flag == "--push" => true,
        _ => bail!("usage: cargo xtask release aur [--push]"),
    };
    let root = workspace_root();
    let tree = read_tree(root)?;
    ensure_agrees(&tree, None)?;
    let version = version::app_version(&tree)?;
    let repository = version::repository(&tree)?;
    // a release's files are there to download only once it is published:
    // a draft's aren't
    let release = format!("{repository}/releases/download/v{version}/");
    let sums_url = format!("{release}SHA256SUMS");
    let sums = fetch(&sums_url)?.with_context(|| {
        format!(
            "v{version} isn't published on GitHub yet ({sums_url} is missing), and the \
             packages download it: publish its draft first"
        )
    })?;
    let sums = parse_sums(&sums);
    let downloads = root.join("target/aur-sources");
    std::fs::create_dir_all(&downloads)?;

    let mut unpushed = Vec::new();
    for package in AUR_PACKAGES {
        println!("\n== {package}");
        let dir = root.join(".aur").join(package);
        let git = || {
            let mut git = Command::new("git");
            git.current_dir(&dir);
            git
        };
        if !dir.exists() {
            std::fs::create_dir_all(dir.parent().unwrap())?;
            run(Command::new("git")
                .arg("clone")
                .arg(format!("ssh://aur@aur.archlinux.org/{package}.git"))
                .arg(&dir))?;
        }
        let status = output(git().args(["status", "--porcelain", "--untracked-files=no"]))?;
        ensure!(
            status.is_empty(),
            "{} has changes of its own, which this would mix in:\n{}",
            dir.display(),
            status.trim_end()
        );
        run(git().args(["pull", "--ff-only", "origin", BRANCH]))?;

        // the package's files, but .SRCINFO, which makepkg writes from
        // the PKGBUILD with its checksums
        let source = format!("packaging/aur/{package}");
        let tracked = output(Command::new("git").current_dir(root).args(["ls-files", &source]))?;
        let mut files = Vec::new();
        for path in tracked.lines() {
            let name = path.strip_prefix(&source).unwrap().trim_start_matches('/');
            if name == ".SRCINFO" {
                continue;
            }
            let to = dir.join(name);
            std::fs::create_dir_all(to.parent().unwrap())?;
            std::fs::copy(root.join(path), &to)
                .with_context(|| format!("could not copy {path}"))?;
            files.push(name.to_owned());
        }

        let mut pkgbuild = std::fs::read_to_string(dir.join("PKGBUILD"))?;
        for (arch, sources) in sources(&printsrcinfo(&dir)?) {
            let mut arch_sums = Vec::new();
            for (name, url) in sources {
                let sum = if let Some(file) = url.strip_prefix(&release) {
                    sums.get(file)
                        .with_context(|| {
                            format!("{sums_url} doesn't list {file}, which {package} downloads")
                        })?
                        .clone()
                } else if url.contains("://") {
                    // as the source package's archive of the tag
                    let path = downloads.join(&name);
                    run(Command::new("curl").args(["-fsSL", "-o"]).arg(&path).arg(&url))?;
                    sha256(&path)?
                } else {
                    sha256(&dir.join(&url))?
                };
                arch_sums.push(sum);
            }
            pkgbuild = set_sums(&pkgbuild, &arch, &arch_sums)?;
        }
        std::fs::write(dir.join("PKGBUILD"), &pkgbuild)?;
        std::fs::write(dir.join(".SRCINFO"), printsrcinfo(&dir)?)?;
        files.push(".SRCINFO".to_owned());

        run(git().arg("add").arg("--").args(&files))?;
        let staged = git().args(["diff", "--cached", "--quiet"]).status()?;
        if staged.success() {
            println!("{package} is up to date");
        } else {
            run(git().args(["diff", "--cached", "--stat"]))?;
            let message = format!("tgradish {}", version::aur_release(&pkgbuild)?);
            run(git().args(["commit", "--quiet", "-m", &message]))?;
        }
        let ahead = output(git().args(["rev-list", "--count", &format!("origin/{BRANCH}..HEAD")]))?;
        if ahead.trim() != "0" {
            if push {
                run(git().args(["push", "origin", &format!("HEAD:{BRANCH}")]))?;
            } else {
                unpushed.push(dir);
            }
        }
    }

    if unpushed.is_empty() {
        println!("\nThe AUR has every package as packaging/aur has it.");
    } else {
        println!("\nCommitted, not pushed. Look over each, then push them with --push, or:\n");
        for dir in &unpushed {
            println!("  git -C {} push origin HEAD:{BRANCH}", dir.display());
        }
        println!("\nor drop the commits with git -C <clone> reset --hard origin/{BRANCH}.");
    }
    Ok(())
}

/// What makepkg says a package's PKGBUILD is, as `.SRCINFO` holds it.
fn printsrcinfo(dir: &Path) -> Result<String> {
    output(Command::new("makepkg").current_dir(dir).arg("--printsrcinfo"))
        .context("makepkg, from pacman, writes .SRCINFO")
}

fn sha256(path: &Path) -> Result<String> {
    let out = output(Command::new("sha256sum").arg(path))?;
    Ok(out.split_whitespace().next().context("sha256sum said nothing")?.to_owned())
}

/// A list of sums, as `sha256sum` writes it, by file name.
fn parse_sums(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(sum, name)| (name.trim_start().trim_start_matches('*').to_owned(), sum.to_owned()))
        .collect()
}

/// The sources a .SRCINFO lists, by the architecture they are for, as the
/// end of their key: `_x86_64` of `source_x86_64`, nothing for `source`;
/// each the file's name and where it comes from, in order.
fn sources(srcinfo: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sources: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in srcinfo.lines() {
        let Some((key, value)) = line.trim_start().split_once(" = ") else { continue };
        let Some(arch) = key.strip_prefix("source") else { continue };
        let (name, url) = match value.split_once("::") {
            Some((name, url)) => (name, url),
            None => (value.rsplit('/').next().unwrap_or(value), value),
        };
        let entry = match sources.iter().position(|(found, _)| found == arch) {
            Some(index) => &mut sources[index].1,
            None => {
                sources.push((arch.to_owned(), Vec::new()));
                &mut sources.last_mut().unwrap().1
            }
        };
        entry.push((name.to_owned(), url.to_owned()));
    }
    sources
}

/// `pkgbuild` with the sums of `sha256sums<arch>=(...)`, which must be on
/// one line, as `packaging/aur` writes them, set to `sums`.
fn set_sums(pkgbuild: &str, arch: &str, sums: &[String]) -> Result<String> {
    let start = format!("sha256sums{arch}=(");
    let lines: Vec<&str> = pkgbuild.split_inclusive('\n').collect();
    let found: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].starts_with(&start)).collect();
    let [index] = found[..] else { bail!("the PKGBUILD has {} lines of {start}", found.len()) };
    ensure!(lines[index].trim_end().ends_with(')'), "the PKGBUILD's {start} isn't on one line");
    let quoted: Vec<String> = sums.iter().map(|sum| format!("'{sum}'")).collect();
    let line = format!("{start}{})\n", quoted.join(" "));
    Ok(lines.iter().enumerate().map(|(i, old)| if i == index { &line } else { *old }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRCINFO: &str = "\
pkgbase = tgradish-bin
\tpkgver = 2.0.0
\tsource_x86_64 = tgradish-bin-2.0.0-x86_64.tar.gz::https://github.com/sliva0/tgradish/releases/download/v2.0.0/tgradish-linux-x86_64-system-ffmpeg.tar.gz
\tsha256sums_x86_64 = SKIP
\tsource_aarch64 = tgradish-bin-2.0.0-aarch64.tar.gz::https://github.com/sliva0/tgradish/releases/download/v2.0.0/tgradish-linux-aarch64-system-ffmpeg.tar.gz
\tsha256sums_aarch64 = SKIP

pkgname = tgradish-bin
";

    #[test]
    fn sources_by_architecture() {
        let found = sources(SRCINFO);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "_x86_64");
        assert_eq!(found[0].1[0].0, "tgradish-bin-2.0.0-x86_64.tar.gz");
        assert!(found[1].1[0].1.ends_with("/v2.0.0/tgradish-linux-aarch64-system-ffmpeg.tar.gz"));
        let plain = sources("\tsource = https://example.com/a/b.tar.gz\n\tsource = local.patch\n");
        assert_eq!(
            plain,
            [(
                String::new(),
                vec![
                    ("b.tar.gz".to_owned(), "https://example.com/a/b.tar.gz".to_owned()),
                    ("local.patch".to_owned(), "local.patch".to_owned()),
                ]
            )]
        );
    }

    #[test]
    fn sums_replace_skip_alone() {
        let pkgbuild =
            "source=(a)\nsha256sums_x86_64=('SKIP')\nsha256sums=('SKIP')\npackage() {\n}\n";
        let set = set_sums(pkgbuild, "_x86_64", &["ab".to_owned()]).unwrap();
        let set = set_sums(&set, "", &["cd".to_owned(), "ef".to_owned()]).unwrap();
        assert_eq!(
            set,
            "source=(a)\nsha256sums_x86_64=('ab')\nsha256sums=('cd' 'ef')\npackage() {\n}\n"
        );
        assert!(set_sums(pkgbuild, "_aarch64", &[]).is_err());
        assert!(set_sums("sha256sums=(\n  'SKIP'\n)\n", "", &[]).is_err());
    }

    #[test]
    fn sums_by_name() {
        let sums = parse_sums("abc  tgradish.exe\ndef *b.tar.gz\n");
        assert_eq!(sums["tgradish.exe"], "abc");
        assert_eq!(sums["b.tar.gz"], "def");
    }

    /// The packages' PKGBUILDs keep their sums on one line each, which
    /// [`set_sums`] needs.
    #[test]
    fn the_pkgbuilds_have_sums_to_set() {
        let root = workspace_root();
        for package in AUR_PACKAGES {
            let pkgbuild = std::fs::read_to_string(root.join(version::pkgbuild(package))).unwrap();
            let srcinfo = std::fs::read_to_string(root.join(version::srcinfo(package))).unwrap();
            for (arch, sources) in sources(&srcinfo) {
                let sums = vec!["0".repeat(64); sources.len()];
                set_sums(&pkgbuild, &arch, &sums).unwrap();
            }
        }
    }
}

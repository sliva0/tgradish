//! `CHANGELOG.md`, as Keep a Changelog lays it out: `## [Unreleased]` over
//! a dated `## [X.Y.Z] - YYYY-MM-DD` section for each release, newest
//! first, and at the bottom a link for each heading, to the changes since
//! the release before.

use anyhow::{Context, Result, bail, ensure};

use super::version::Version;

const UNRELEASED: &str = "## [Unreleased]";

/// Whether `line` heads a release's section, or Unreleased.
fn is_heading(line: &str) -> bool {
    line.starts_with("## [")
}

/// Whether `line` is one of the links at the bottom, which end the last
/// section.
fn is_link(line: &str) -> bool {
    line.starts_with('[') && line.contains("]: ")
}

/// Whether `text` is a date as YYYY-MM-DD.
pub fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, byte)| match i {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// The index of the line that heads `version`'s section, and the date it
/// gives, if it gives one.
fn heading(lines: &[&str], version: Version) -> Option<(usize, Option<String>)> {
    let start = format!("## [{version}]");
    lines.iter().enumerate().find_map(|(index, line)| {
        let rest = line.strip_prefix(&start)?;
        let date = rest.strip_prefix(" - ").filter(|date| is_date(date)).map(str::to_owned);
        (rest.is_empty() || rest.starts_with(' ')).then_some((index, date))
    })
}

/// The lines under the heading at `index`, up to the next heading or the
/// links at the bottom.
fn body<'b, 'a>(lines: &'b [&'a str], index: usize) -> &'b [&'a str] {
    let rest = &lines[index + 1..];
    let end = rest.iter().position(|line| is_heading(line) || is_link(line)).unwrap_or(rest.len());
    &rest[..end]
}

/// `lines` without the blank ones they start and end with.
fn trimmed<'a, 'b>(lines: &'b [&'a str]) -> &'b [&'a str] {
    let start = lines.iter().position(|line| !line.trim().is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|line| !line.trim().is_empty()).map_or(start, |end| end + 1);
    &lines[start..end]
}

/// What `version`'s section says, without its heading: the release's
/// notes. Fails where it has no dated section, or an empty one.
pub fn notes(text: &str, version: Version) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (index, date) =
        heading(&lines, version).with_context(|| format!("CHANGELOG.md has no {version}"))?;
    ensure!(
        date.is_some(),
        "CHANGELOG.md's {version} isn't dated as `## [{version}] - YYYY-MM-DD`"
    );
    let body = trimmed(body(&lines, index));
    ensure!(!body.is_empty(), "CHANGELOG.md says nothing under {version}");
    Ok(body.join("\n") + "\n")
}

/// The link of Unreleased, `[Unreleased]: <repository>/compare/vX.Y.Z...HEAD`,
/// split into its line's index, the repository and the version it starts
/// from.
fn unreleased_link(lines: &[&str]) -> Result<(usize, String, String)> {
    let (index, line) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.starts_with("[Unreleased]: "))
        .context("CHANGELOG.md has no [Unreleased] link at the bottom")?;
    let link = &line["[Unreleased]: ".len()..];
    let parsed = link
        .split_once("/compare/v")
        .and_then(|(repository, rest)| Some((repository, rest.strip_suffix("...HEAD")?)));
    let Some((repository, since)) = parsed else {
        bail!("CHANGELOG.md's [Unreleased] links to {link}, not <repository>/compare/vX.Y.Z...HEAD")
    };
    Ok((index, repository.to_owned(), since.to_owned()))
}

/// The changelog with Unreleased become `version`'s section, dated `date`,
/// under an Unreleased of its own, empty, and their links: Unreleased's to
/// the changes since `version`, and `version`'s to those since `previous`.
/// Fails where Unreleased says nothing, as a release with no notes would.
pub fn release(text: &str, version: Version, previous: Version, date: &str) -> Result<String> {
    ensure!(is_date(date), "{date:?} isn't a date as YYYY-MM-DD");
    let lines: Vec<&str> = text.lines().collect();
    ensure!(heading(&lines, version).is_none(), "CHANGELOG.md already has a section for {version}");
    let unreleased = lines
        .iter()
        .position(|line| line.trim_end() == UNRELEASED)
        .with_context(|| format!("CHANGELOG.md has no `{UNRELEASED}` section"))?;
    ensure!(
        !trimmed(body(&lines, unreleased)).is_empty(),
        "CHANGELOG.md says nothing under Unreleased, which would be {version}'s notes"
    );
    let (link, repository, since) = unreleased_link(&lines)?;
    ensure!(
        since == previous.to_string(),
        "CHANGELOG.md's [Unreleased] compares with v{since}, but the version is {previous}"
    );
    let heading = format!("## [{version}] - {date}");
    let unreleased_link = format!("[Unreleased]: {repository}/compare/v{version}...HEAD");
    let version_link = format!("[{version}]: {repository}/compare/v{previous}...v{version}");
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + 3);
    for (index, line) in lines.iter().enumerate() {
        if index == link {
            out.push(&unreleased_link);
            out.push(&version_link);
            continue;
        }
        out.push(line);
        if index == unreleased {
            out.push("");
            out.push(&heading);
        }
    }
    let mut text_out = out.join("\n");
    if text.ends_with('\n') {
        text_out.push('\n');
    }
    Ok(text_out)
}

/// What is wrong with the changelog for a release of `version`: it needs a
/// dated section that says something, a link for it, and Unreleased's
/// link starting from it.
pub fn problems(text: &str, version: Version) -> Vec<String> {
    let mut problems = Vec::new();
    if let Err(error) = notes(text, version) {
        problems.push(error.to_string());
    }
    let lines: Vec<&str> = text.lines().collect();
    let link = format!("[{version}]: ");
    if !lines.iter().any(|line| line.starts_with(&link)) {
        problems.push(format!("CHANGELOG.md has no [{version}] link at the bottom"));
    }
    match unreleased_link(&lines) {
        Ok((_, _, since)) if since == version.to_string() => {}
        Ok((_, _, since)) => problems
            .push(format!("CHANGELOG.md's [Unreleased] compares with v{since}, not v{version}")),
        Err(error) => problems.push(error.to_string()),
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEFORE: &str = "\
# Changelog

Newest first.

## [Unreleased]

### Added

- Undo.

### Fixed

- The accent on Plasma.

## [2.0.0] - 2026-11-16

tgradish rewritten in Rust.

### Added

- A GUI.

## [1.0.4] - 2024-10-28

The last Python release.

[Unreleased]: https://github.com/sliva0/tgradish/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0
";

    const AFTER: &str = "\
# Changelog

Newest first.

## [Unreleased]

## [2.1.0] - 2026-11-20

### Added

- Undo.

### Fixed

- The accent on Plasma.

## [2.0.0] - 2026-11-16

tgradish rewritten in Rust.

### Added

- A GUI.

## [1.0.4] - 2024-10-28

The last Python release.

[Unreleased]: https://github.com/sliva0/tgradish/compare/v2.1.0...HEAD
[2.1.0]: https://github.com/sliva0/tgradish/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0
";

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn release_dates_unreleased_under_an_empty_one() {
        let after = release(BEFORE, version("2.1.0"), version("2.0.0"), "2026-11-20").unwrap();
        assert_eq!(after, AFTER);
        assert!(
            problems(&after, version("2.1.0")).is_empty(),
            "{:?}",
            problems(&after, version("2.1.0"))
        );
    }

    #[test]
    fn release_refuses_nothing_to_say() {
        let empty = AFTER.replace("2.1.0", "2.0.9");
        let error = release(&empty, version("2.2.0"), version("2.0.9"), "2026-11-21").unwrap_err();
        assert!(error.to_string().contains("nothing under Unreleased"), "{error}");
    }

    #[test]
    fn release_refuses_a_version_it_has_or_another_start() {
        let error = release(BEFORE, version("2.0.0"), version("2.0.0"), "2026-11-20").unwrap_err();
        assert!(error.to_string().contains("already has"), "{error}");
        let error = release(BEFORE, version("2.1.0"), version("1.9.0"), "2026-11-20").unwrap_err();
        assert!(error.to_string().contains("compares with v2.0.0"), "{error}");
        assert!(release(BEFORE, version("2.1.0"), version("2.0.0"), "20.11.2026").is_err());
    }

    #[test]
    fn notes_are_the_section_alone() {
        assert_eq!(
            notes(AFTER, version("2.1.0")).unwrap(),
            "### Added\n\n- Undo.\n\n### Fixed\n\n- The accent on Plasma.\n"
        );
        assert_eq!(
            notes(AFTER, version("2.0.0")).unwrap(),
            "tgradish rewritten in Rust.\n\n### Added\n\n- A GUI.\n"
        );
        // the last section ends before the links
        assert_eq!(notes(AFTER, version("1.0.4")).unwrap(), "The last Python release.\n");
        assert!(notes(AFTER, version("2.1.1")).is_err());
        // Unreleased isn't a release
        let undated = AFTER.replace("## [2.1.0] - 2026-11-20", "## [2.1.0]");
        assert!(notes(&undated, version("2.1.0")).is_err());
    }

    #[test]
    fn problems_name_what_a_release_lacks() {
        assert!(problems(BEFORE, version("2.0.0")).is_empty());
        let problems = problems(BEFORE, version("2.1.0"));
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn dates() {
        assert!(is_date("2026-11-17"));
        assert!(!is_date("2026-11-7"));
        assert!(!is_date("2026/11/17"));
    }
}

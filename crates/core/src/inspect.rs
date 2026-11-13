//! What a sticker file is and holds, whichever of the three kinds it is,
//! and what Telegram would say of it: what the command line's `inspect`
//! prints and the window's Inspect shows.

use std::path::Path;

use serde::Serialize;

use tgradish_tgs::check;

use crate::error::Result;
use crate::input::{FileKind, sniff};
use crate::telegram::{self, Target};
use crate::webm::container::{self, WebmInfo};
use crate::{tgs, webp};

/// What a sticker file holds, as `info`, and what Telegram would refuse
/// of it, as `issues`, tagged by its `format`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "format", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Inspected {
    /// A WebP picture, judged as `target`.
    Webp { target: Target, info: webp::Info, issues: Vec<telegram::Issue> },
    /// A `.tgs`, whose limits are the same for stickers and emoji.
    Tgs { info: Box<check::Stats>, issues: Vec<check::Issue> },
    /// A WebM video, judged as `target`.
    Webm { target: Target, info: Box<WebmInfo>, issues: Vec<telegram::Issue> },
}

impl Inspected {
    /// Whether Telegram would refuse the file: any issue of WebM or WebP,
    /// and of a `.tgs` those it refuses rather than frowns on.
    pub fn refused(&self) -> bool {
        match self {
            Inspected::Webp { issues, .. } | Inspected::Webm { issues, .. } => !issues.is_empty(),
            Inspected::Tgs { issues, .. } => {
                issues.iter().any(|issue| issue.severity == check::Severity::Error)
            }
        }
    }
}

/// What the sticker at `path` holds, judged as `target`, or else as what
/// its size says it is.
pub fn inspect(path: &Path, target: Option<Target>) -> Result<Inspected> {
    Ok(match sniff(path) {
        FileKind::Webp { .. } => {
            let info = webp::inspect_file(path)?;
            let target = target.unwrap_or_else(|| info.target());
            let issues = info.issues(target);
            Inspected::Webp { target, info, issues }
        }
        FileKind::Tgs => {
            let (info, issues) = tgs::inspect_file(path)?;
            Inspected::Tgs { info: Box::new(info), issues }
        }
        // anything else is taken for WebM, which says what it isn't
        _ => {
            let info = container::inspect_file(path)?;
            let target = target.unwrap_or_else(|| info.target());
            let issues = telegram::check(&info, target);
            Inspected::Webm { target, info: Box::new(info), issues }
        }
    })
}

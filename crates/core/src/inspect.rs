//! What a sticker file is and holds, whichever of the three kinds it is,
//! and what Telegram would say of it: what the command line's `inspect`
//! prints and the window's Inspect shows.

use std::path::Path;

use serde::Serialize;

use crate::error::Result;
use crate::telegram::{self, Target};
use crate::webm::{self, WebmInfo};
use crate::{still, tgs};

/// The kinds of sticker file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A WebP picture, by its extension.
    Image,
    /// A `.tgs`, by the gzip it is packed in.
    Tgs,
    /// Anything else is taken for WebM.
    Webm,
}

impl Kind {
    /// The kind of sticker at `path`, as tgradish tells them apart.
    pub fn of(path: &Path) -> Kind {
        if still::is_image(path) {
            Kind::Image
        } else if tgs::is_sticker(path) {
            Kind::Tgs
        } else {
            Kind::Webm
        }
    }
}

/// What a sticker file holds, and what Telegram would refuse of it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum Inspected {
    /// A WebP picture, judged as `target`.
    Image { target: Target, info: still::Info, issues: Vec<telegram::Issue> },
    /// A `.tgs`, whose limits are the same for stickers and emoji.
    Tgs { stats: Box<tgs::Stats>, issues: Vec<tgs::Issue> },
    /// A WebM video, judged as `target`.
    Webm { target: Target, info: Box<WebmInfo>, issues: Vec<telegram::Issue> },
}

/// What the sticker at `path` holds, judged as `target`, or else as what
/// its size says it is.
pub fn inspect(path: &Path, target: Option<Target>) -> Result<Inspected> {
    Ok(match Kind::of(path) {
        Kind::Image => {
            let info = still::inspect_file(path)?;
            let target = target.unwrap_or_else(|| info.target());
            let issues = info.issues(target);
            Inspected::Image { target, info, issues }
        }
        Kind::Tgs => {
            let (stats, issues) = tgs::inspect_file(path)?;
            Inspected::Tgs { stats: Box::new(stats), issues }
        }
        Kind::Webm => {
            let info = webm::inspect_file(path)?;
            let target = target.unwrap_or_else(|| info.target());
            let issues = telegram::check(&info, target);
            Inspected::Webm { target, info: Box::new(info), issues }
        }
    })
}

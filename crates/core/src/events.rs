//! What a conversion of any format reports and makes: its [`Outcome`],
//! the same of every format but for its [`Details`].

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;
use tgradish_tgs::sticker::Step;

use crate::presets::Format;
use crate::preview::Preview;
use crate::telegram::{Issue, Target};
use crate::webm::Params;
use crate::webp::Encoding;

/// What converting made, of any format, which its `finished` event says
/// whether or not events were followed: the sticker or emoji written, how
/// it measures up to Telegram's limit for it, and what Telegram would
/// still have a problem with, beside what only its format has.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Outcome {
    /// The input, the first picture of a sequence, as every event about
    /// one names it.
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub input: PathBuf,
    /// Where the result was written.
    #[serde(serialize_with = "crate::protocol::serialize_path")]
    pub output: PathBuf,
    /// What it is.
    pub format: Format,
    /// What it was made as.
    pub target: Target,
    /// Its size in bytes.
    pub bytes: u64,
    /// The most Telegram takes of it, in bytes.
    pub limit: u64,
    /// Whether anything visible was given up to fit the limit: for WebM, a
    /// lower frame rate or a shorter length than planned, as fitting by
    /// them makes; for `.tgs`, reductions, a forced pixel scale or the end
    /// of a long input; for WebP, saving it lossy where lossless didn't
    /// fit.
    pub changed: bool,
    /// What Telegram would still have a problem with; it refuses the
    /// result for any error, as [`crate::telegram::refuses`] says.
    pub issues: Vec<Issue>,
    /// What only its format has, beside the rest.
    #[serde(flatten)]
    pub details: Details,
}

/// What only one format's [`Outcome`] has, serialised beside the rest of
/// it, its `format` saying which.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged, rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Details {
    /// A WebM's: the encode fitting kept, and how.
    Webm {
        /// The attempt that was kept.
        attempt: u32,
        /// Its settings.
        params: Params,
        /// Whether its header's duration was spoofed.
        spoofed: bool,
        /// Whether fitting was asked to stop early and kept the best
        /// encode that fit so far, see
        /// [`CancelToken::settle`](crate::CancelToken::settle).
        settled: bool,
        /// Where the intermediate files are, when they were kept.
        #[serde(serialize_with = "crate::protocol::serialize_optional_path")]
        temp_dir: Option<PathBuf>,
    },
    /// A `.tgs`'s: what its Lottie holds, and what fitting did.
    Tgs {
        /// Size of the Lottie JSON inside.
        json_bytes: u64,
        /// Reductions fitting applied, in order.
        steps: Vec<Step>,
        /// Lottie layers in it.
        layers: u64,
        /// Rectangles drawn in all of them.
        rectangles: u64,
        /// What the sticker shows, for previews; not serialised.
        #[serde(skip)]
        preview: Preview,
    },
    /// A WebP's: the picture, and how it was encoded.
    Webp {
        /// Its width in pixels.
        width: u32,
        /// Its height in pixels.
        height: u32,
        /// How it was encoded to fit.
        encoding: Encoding,
    },
}

impl Outcome {
    /// What a `.tgs` shows, drawn from what it was made of, as reading it
    /// back with [`crate::tgs::preview_file`] draws it; `None` for the
    /// other formats, which are read from their files for previews, as
    /// [`crate::webp::preview`] and
    /// [`Backend::preview`](crate::backend::Backend::preview) read them.
    pub fn preview(&self) -> Option<Preview> {
        match &self.details {
            Details::Tgs { preview, .. } => Some(preview.clone()),
            _ => None,
        }
    }
}

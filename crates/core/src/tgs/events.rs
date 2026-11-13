//! What a `.tgs` conversion reports as it goes.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;
pub use tgradish_tgs::check::Issue;
use tgradish_tgs::normalise::Report;
use tgradish_tgs::sticker::Step;

/// Progress of a `.tgs` conversion, printed by `convert --json` one per
/// line.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum TgsEvent {
    /// The input is read: what normalising found and did.
    Started {
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        input: PathBuf,
        report: Box<Report>,
    },
    /// Unchanged, the sticker would come to about this many bytes, more
    /// than fits; fitting starts, unless `exact` keeps the art as it is.
    TooLarge { bytes: u64 },
    /// Fitting applied a reduction.
    Reduced { step: Step },
    /// The result is being compressed.
    Packing,
    /// Something about the art worth knowing, as normalising found it.
    Warning { message: String },
    Finished {
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        output: PathBuf,
        bytes: u64,
        /// Size of the Lottie JSON inside.
        json_bytes: u64,
        /// Whether anything visible was given up to fit: reductions, a
        /// forced pixel scale, the end of a long input.
        changed: bool,
        /// Reductions fitting applied, in order.
        steps: Vec<Step>,
        layers: u64,
        rectangles: u64,
        /// Problems Telegram would still have with the result.
        issues: Vec<Issue>,
    },
    /// A conversion, or the whole command, failed. Only printed by the
    /// CLI; the library returns errors instead.
    Error(crate::protocol::Failure),
}

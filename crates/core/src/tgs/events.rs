//! What a `.tgs` conversion reports as it goes.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;
use tgradish_tgs::normalise::Report;
use tgradish_tgs::sticker::Step;

/// Progress of a `.tgs` conversion, printed by `convert --json` one per
/// line.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
// `describe` titles the schema as it always has
#[schemars(rename = "TgsEvent")]
#[non_exhaustive]
pub enum Event {
    /// The input is read: what normalising found and did.
    Started {
        /// The input, the first file of a sequence.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        input: PathBuf,
        /// What normalising found and did.
        report: Box<Report>,
    },
    /// Unchanged, the sticker would come to about this many bytes, more
    /// than fits; fitting starts, unless `exact` keeps the art as it is.
    TooLarge {
        /// The estimate, in bytes.
        bytes: u64,
    },
    /// Fitting applied a reduction.
    Reduced {
        /// Which, and what it came to.
        step: Step,
    },
    /// The result is being compressed.
    Packing,
    /// Something about the art worth knowing, as normalising found it.
    Warning {
        /// What, in English.
        message: String,
    },
    /// The sticker is written.
    Finished {
        /// The input, the first picture of a sequence, as every format's
        /// result names it.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        input: PathBuf,
        /// Where.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        output: PathBuf,
        /// Its size in bytes.
        bytes: u64,
        /// Size of the Lottie JSON inside.
        json_bytes: u64,
        /// Whether anything visible was given up to fit: reductions, a
        /// forced pixel scale, the end of a long input.
        changed: bool,
        /// Reductions fitting applied, in order.
        steps: Vec<Step>,
        /// Lottie layers in it.
        layers: u64,
        /// Rectangles drawn in all of them.
        rectangles: u64,
        /// Problems Telegram would still have with the result.
        issues: Vec<crate::telegram::Issue>,
    },
    /// A conversion, or the whole command, failed. Only printed by the
    /// CLI; the library returns errors instead.
    Error(crate::protocol::Failure),
}

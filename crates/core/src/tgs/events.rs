//! What a `.tgs` conversion reports as it goes.

use schemars::JsonSchema;
use serde::Serialize;
use tgradish_tgs::sticker::Step;

/// Progress of a `.tgs` conversion, printed by `convert --json` one per
/// line.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
// `describe` titles the schema as it always has
#[schemars(rename = "TgsEvent")]
#[non_exhaustive]
pub enum Event {
    /// The input is read, with what normalising found and did.
    Started(crate::Started),
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
    /// The sticker is written: what was made, as converting returns it.
    Finished(Box<crate::Outcome>),
    /// A conversion, or the whole command, failed. Only printed by the
    /// CLI; the library returns errors instead.
    Error(crate::protocol::Failure),
}

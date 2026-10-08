//! Machine-readable description of tgradish for front-ends that run the
//! CLI, printed by `tgradish describe`. See `docs/protocol.md`.

use schemars::{JsonSchema, Schema, schema_for};
use serde::Serialize;

use crate::events::Event;
use crate::options::Options;
use crate::presets::Presets;

/// Bumped on incompatible changes to the description, events or CLI flags
/// used by front-ends.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct Description {
    pub protocol: u32,
    pub tool: &'static str,
    pub version: &'static str,
    /// Extension of produced files.
    pub output_extension: &'static str,
    pub default_preset: String,
    pub presets: Vec<PresetDescription>,
    /// JSON Schema of the options accepted by `convert --options-json`.
    /// Every property also has a `--<property>` flag.
    pub options: Schema,
    /// JSON Schema of the events printed by `convert --json`, one per line.
    pub events: Schema,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PresetDescription {
    pub name: String,
    pub description: String,
    pub builtin: bool,
    /// Options with everything the preset extends applied.
    pub options: Option<Options>,
    /// Why the preset cannot be used, if it is broken.
    pub error: Option<String>,
}

pub fn describe(presets: &Presets, default_preset: &str) -> Description {
    let presets = presets
        .iter()
        .map(|(name, preset)| {
            let resolved = presets.resolve(name);
            PresetDescription {
                name: name.to_string(),
                description: preset.map(|p| p.description.clone()).unwrap_or_default(),
                builtin: preset.is_ok_and(|p| p.path.is_none()),
                error: resolved.as_ref().err().map(ToString::to_string),
                options: resolved.ok(),
            }
        })
        .collect();
    Description {
        protocol: PROTOCOL_VERSION,
        tool: "tgradish",
        version: env!("CARGO_PKG_VERSION"),
        output_extension: "webm",
        default_preset: default_preset.to_string(),
        presets,
        options: schema_for!(Options),
        events: schema_for!(Event),
    }
}

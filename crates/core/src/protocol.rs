//! Machine-readable description of tgradish for front-ends that run the
//! CLI, printed by `tgradish describe`. See `docs/protocol.md`.

use schemars::{JsonSchema, Schema, schema_for};
use serde::Serialize;

use crate::events::Event;
use crate::options::Options;
use crate::presets::{Format, Presets};
use crate::tgs::{TgsEvent, TgsOptions};

/// Bumped on incompatible changes to the description, events or CLI flags
/// used by front-ends. 2: `.tgs` output, formats and preset formats. 3:
/// presets hold options for both formats, and say nothing about the target.
pub const PROTOCOL_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct Description {
    pub protocol: u32,
    pub tool: &'static str,
    pub version: &'static str,
    /// The format conversions make when nothing asks for another.
    pub default_format: Format,
    pub formats: Vec<FormatDescription>,
    pub presets: Vec<PresetDescription>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormatDescription {
    pub format: Format,
    pub description: &'static str,
    /// Extension of produced files.
    pub output_extension: &'static str,
    /// The preset used for this format when none is given.
    pub default_preset: String,
    /// JSON Schema of the options accepted by `convert --options-json` for
    /// this format. Every property also has a `--<property>` flag.
    pub options: Schema,
    /// JSON Schema of the events printed by `convert --json`, one per line.
    pub events: Schema,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PresetDescription {
    pub name: String,
    pub description: String,
    pub builtin: bool,
    /// Options for WebM results, with everything the preset extends
    /// applied; `None` if broken.
    pub webm: Option<Options>,
    /// The same for `.tgs` results.
    pub tgs: Option<TgsOptions>,
    /// Why the preset cannot be used, if it is broken for either format.
    pub error: Option<String>,
}

/// `default_presets` are the presets used when none is given, for WebM and
/// `.tgs`.
pub fn describe(presets: &Presets, default_presets: [&str; 2]) -> Description {
    let presets = presets
        .iter()
        .map(|(name, preset)| {
            let (webm, tgs) = (presets.webm(name), presets.tgs(name));
            let error = match (&webm, &tgs) {
                (Err(err), _) | (_, Err(err)) => Some(err.to_string()),
                _ => None,
            };
            PresetDescription {
                name: name.to_string(),
                description: preset.map(|p| p.description.clone()).unwrap_or_default(),
                builtin: preset.is_ok_and(|p| p.path.is_none()),
                webm: webm.ok(),
                tgs: tgs.ok(),
                error,
            }
        })
        .collect();
    let [webm, tgs] = default_presets;
    Description {
        protocol: PROTOCOL_VERSION,
        tool: "tgradish",
        version: env!("CARGO_PKG_VERSION"),
        default_format: Format::default(),
        formats: vec![
            FormatDescription {
                format: Format::Webm,
                description: "Video sticker or emoji, from any video or image",
                output_extension: Format::Webm.extension(),
                default_preset: webm.to_string(),
                options: schema_for!(Options),
                events: schema_for!(Event),
            },
            FormatDescription {
                format: Format::Tgs,
                description: "Animated sticker, from pixel art",
                output_extension: Format::Tgs.extension(),
                default_preset: tgs.to_string(),
                options: schema_for!(TgsOptions),
                events: schema_for!(TgsEvent),
            },
        ],
        presets,
    }
}

//! Machine-readable CLI schema printed by `tgradish describe` and the wire
//! conventions followed by every `--json` command:
//!
//! - All JSON keys and enum values use kebab-case.
//! - Every `--json` command prints JSON Lines (one compact JSON object per
//!   line). `convert` and `watch` emit [`Event`]s (`started` first, `finished`
//!   last, carrying `input`, `output`, `format`, `target`, `limit`, plus
//!   format-specific fields). `inspect` prints one file per line, `preset list`
//!   one preset per line, `licenses` one notice per line, `spoof` one
//!   [`SpoofEvent`], and other commands one object.
//! - Tagged unions name their discriminator by role: `event` for events,
//!   `format` for inspection results, `kind` for issues, WebP encodings and
//!   ffmpeg backends, and `mode` for rate control.
//! - Paths are serialised lossily as UTF-8 strings; per-input objects put the
//!   input path under `input`.
//! - Telegram validation issues use [`crate::telegram::Issue`] uniformly across
//!   all formats (`kind`, `severity`, `message`, plus variant fields). Any
//!   `error`-severity issue means Telegram rejects the file and causes the CLI
//!   to exit with status 3.
//! - Failures emit an `error` event with `message` and `input` (`null` for
//!   command-wide errors).

use std::path::{Path, PathBuf};

use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, schema_for};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::events::Event;
use crate::presets::{Format, Presets};
use crate::tgs;
use crate::webm::Options;

/// Protocol version for `describe`, `--json` output and machine-facing CLI
/// flags. Incremented only on breaking changes (renamed/removed keys or
/// flags); additive changes (new events, keys or flags) keep the same version,
/// so front-ends should ignore anything they do not recognise.
pub const PROTOCOL_VERSION: u32 = 1;

/// Top-level document printed by `tgradish describe`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Description {
    /// Protocol version ([`PROTOCOL_VERSION`]).
    pub protocol: u32,
    /// Tool name (`"tgradish"`).
    pub tool: &'static str,
    /// Package version.
    pub version: &'static str,
    /// Default output format when none is specified.
    pub default_format: Format,
    /// Instructions for front-ends to cancel or settle `convert` or `watch`
    /// over stdin when they cannot send Ctrl-C.
    pub cancel: &'static str,
    /// Supported output formats and their option schemas.
    pub formats: Vec<FormatDescription>,
    /// JSON Schema for [`Event`] lines emitted by `convert --json` and
    /// `watch --json`.
    pub events: Schema,
    /// All built-in and user presets (including broken ones with `error` set).
    pub presets: Vec<PresetDescription>,
    /// JSON Schema for [`SpoofEvent`] emitted by `spoof --json`.
    pub spoof_events: Schema,
}

/// JSON event emitted by `tgradish spoof --json`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum SpoofEvent {
    /// Spoofed file written successfully.
    Finished {
        /// Source WebM path.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        input: PathBuf,
        /// Destination WebM path.
        #[serde(serialize_with = "crate::protocol::serialize_path")]
        output: PathBuf,
        /// Summary of container edits.
        report: crate::webm::container::PatchReport,
        /// Remaining Telegram validation issues (e.g. if duration still
        /// exceeds 3 s).
        issues: Vec<crate::telegram::Issue>,
    },
    /// Spoofing failed.
    Error(Failure),
}

/// Output format entry inside [`Description`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct FormatDescription {
    /// Format identifier.
    pub format: Format,
    /// One-sentence summary of the format.
    pub description: &'static str,
    /// Output file extension without a dot.
    pub output_extension: &'static str,
    /// Effective default preset name for this format.
    pub default_preset: String,
    /// JSON Schema for `convert --options-json`. Every property also maps to
    /// `--<property>`.
    pub options: Schema,
}

/// Preset entry inside [`Description`].
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct PresetDescription {
    /// Preset name.
    pub name: String,
    /// Preset description (empty if omitted or broken).
    pub description: String,
    /// True for built-in presets; false for user preset files.
    pub builtin: bool,
    /// Resolved WebM/WebP options (with `extends` applied), or `None` if
    /// broken.
    pub webm: Option<Options>,
    /// Resolved `.tgs` options (with `extends` applied), or `None` if broken.
    pub tgs: Option<tgs::Options>,
    /// Error message if the preset is invalid for either format.
    pub error: Option<String>,
}

/// Error event payload emitted by CLI `--json` commands (`convert`, `watch`,
/// `inspect`, `spoof`). Library functions return `Err` instead.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Failure {
    /// Human-readable error message.
    pub message: String,
    /// Failing input path, or `None` for command-wide errors.
    #[serde(serialize_with = "crate::protocol::serialize_optional_path")]
    pub input: Option<PathBuf>,
}

/// Lossily serialises `path` as a UTF-8 string (replacing invalid UTF-8 with
/// U+FFFD) so non-Unicode paths never fail JSON serialisation. Every printed
/// path goes through this via `serialize_with`.
pub fn serialize_path<S: serde::Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&path.to_string_lossy())
}

/// Optional version of [`serialize_path`].
pub fn serialize_optional_path<S: serde::Serializer>(
    path: &Option<PathBuf>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match path {
        Some(path) => serializer.serialize_some(&*path.to_string_lossy()),
        None => serializer.serialize_none(),
    }
}

/// Generates a JSON Schema matching how `T` serialises (rather than
/// deserialises).
fn printed<T: JsonSchema>() -> Schema {
    SchemaSettings::default().for_serialize().into_generator().into_root_schema_for::<T>()
}

/// Filters `schema` to keep only properties listed in `keys` and the `$defs`
/// reachable from them (used to derive the WebP option schema from WebM's,
/// omitting unused `$defs` such as `Fit` and `Spoof`), so front-ends building
/// options from it never send one WebP refuses.
fn only_properties(mut schema: Schema, keys: &[&str]) -> Schema {
    let Some(root) = schema.as_object_mut() else { return schema };
    if let Some(Value::Object(properties)) = root.get_mut("properties") {
        properties.retain(|key, _| keys.contains(&key.as_str()));
    }
    if let Some(Value::Array(required)) = root.get_mut("required") {
        required.retain(|key| key.as_str().is_some_and(|key| keys.contains(&key)));
    }
    // Definitions can reference other definitions, so follow $ref transitively.
    let mut wanted = Vec::new();
    let mut unread = vec![root.get("properties").cloned().unwrap_or_default()];
    while let Some(value) = unread.pop() {
        for reference in references(&value) {
            let name = reference.trim_start_matches("#/$defs/").to_owned();
            if !wanted.contains(&name) {
                unread.extend(root.get("$defs").and_then(|defs| defs.get(&name)).cloned());
                wanted.push(name);
            }
        }
    }
    if let Some(Value::Object(definitions)) = root.get_mut("$defs") {
        definitions.retain(|name, _| wanted.contains(name));
    }
    schema
}

/// Every `$ref` in `value`, at any depth.
fn references(value: &Value) -> Vec<&str> {
    match value {
        Value::Object(map) => map
            .iter()
            .flat_map(|(key, value)| match (key.as_str(), value) {
                ("$ref", Value::String(reference)) => vec![reference.as_str()],
                _ => references(value),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(references).collect(),
        _ => Vec::new(),
    }
}

/// `config` says which preset each format uses when none is given.
pub fn describe(presets: &Presets, config: &Config) -> Description {
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
    let default_preset = |format: Format| config.preset_for(format).to_owned();
    Description {
        protocol: PROTOCOL_VERSION,
        tool: "tgradish",
        version: env!("CARGO_PKG_VERSION"),
        default_format: Format::default(),
        cancel: "Pass --cancel-on-stdin and write a line `cancel` to stdin to stop as Ctrl-C does \
                 (convert exits with 130; watch exits with 0). A line `settle` during WebM \
                 fitting, once an encode fits (progress events report can-settle: true), keeps the \
                 best encode so far (finished reports settled: true). Without this flag, stdin is \
                 not read.",
        formats: vec![
            FormatDescription {
                format: Format::Webm,
                description: "Video sticker or custom emoji from any video or image",
                output_extension: Format::Webm.extension(),
                default_preset: default_preset(Format::Webm),
                options: schema_for!(Options),
            },
            FormatDescription {
                format: Format::Tgs,
                description: "Animated sticker or custom emoji from pixel art (videos are read as \
                              pixel art)",
                output_extension: Format::Tgs.extension(),
                default_preset: default_preset(Format::Tgs),
                options: schema_for!(tgs::Options),
            },
            FormatDescription {
                format: Format::Webp,
                description: "Static sticker or custom emoji from an image or a single video \
                              frame; takes the WebM options that apply to still images, with start \
                              choosing the frame",
                output_extension: Format::Webp.extension(),
                default_preset: default_preset(Format::Webp),
                options: only_properties(schema_for!(Options), &crate::webp::PICTURE_OPTIONS),
            },
        ],
        events: printed::<Event>(),
        presets,
        spoof_events: printed::<SpoofEvent>(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Failure;
    use super::*;
    use crate::events::{Details, Outcome, StartDetails, Started};
    use crate::ffmpeg::Probe;
    use crate::telegram::{Issue, Target};
    use crate::webm::{Decision, Params, Rate};
    use crate::webm::{Plan, Request};
    use crate::webp::Encoding;

    fn probe() -> Probe {
        Probe {
            format: "mov,mp4".into(),
            codec: "h264".into(),
            width: 640,
            height: 480,
            fps: Some(30.0),
            base_fps: Some(30.0),
            duration: Some(2.0),
            start: 0.0,
            alpha: false,
            still_image: false,
            orientation: crate::ffmpeg::Orientation::FlipHorizontal,
            decoder: None,
        }
    }

    fn plan() -> Plan {
        crate::webm::plan(&Request::new(crate::input::Input::File("in.mp4".into())), probe())
            .unwrap()
            .0
    }

    /// One of each event of each stream, as `convert --json` prints them.
    pub(crate) fn samples() -> Vec<Value> {
        let params = Params { fps: 30.0, length: 2.0, rate: Rate::Bitrate(500.0) };
        let too_big = Issue::TooBig { bytes: 300_000, max: 262_144 };
        let webm = [
            Event::Started(Started {
                input: "in.mp4".into(),
                output: "in.sticker.webm".into(),
                format: Format::Webm,
                target: Target::Sticker,
                limit: 262_144,
                details: StartDetails::Webm { plan: Box::new(plan()) },
            }),
            Event::AttemptStarted { attempt: 1, params },
            Event::Progress { attempt: 1, pass: 1, passes: 2, fraction: 0.5, can_settle: false },
            Event::AttemptFinished {
                attempt: 1,
                params,
                bytes: 300_000,
                fits: false,
                can_settle: false,
            },
            Event::Scored { attempt: 1, ssim: 0.95 },
            {
                let decision = Decision::InBand { attempt: 1, share: 0.99, least: 0.96 };
                Event::Decided { decision, message: decision.to_string() }
            },
            Event::Warning { message: "a warning".into() },
            Event::Log { line: "a line".into() },
            Event::Finished(Outcome {
                input: "in.mp4".into(),
                output: "in.sticker.webm".into(),
                format: Format::Webm,
                target: Target::Sticker,
                bytes: 250_000,
                limit: 262_144,
                changed: false,
                issues: vec![too_big.clone()],
                details: Details::Webm {
                    attempt: 2,
                    params: Params { rate: Rate::Crf(32), ..params },
                    spoofed: false,
                    settled: false,
                    temp_dir: Some("/tmp/tgradish-x".into()),
                },
            }),
            Event::Error(Failure { message: "it failed".into(), input: Some("in.mp4".into()) }),
        ]
        .map(|event| serde_json::to_value(event).unwrap());
        let animation = tgradish_tgs::frames::Animation::new(
            2,
            2,
            vec![tgradish_tgs::frames::Frame {
                rgba: vec![255; 16],
                duration: std::time::Duration::from_millis(100),
            }],
        )
        .unwrap();
        let (art, report) =
            tgradish_tgs::normalise(animation.clone(), &Default::default()).unwrap();
        let step = tgradish_tgs::sticker::Step::new(
            tgradish_tgs::reduce::Reduction::Despeckle,
            60_000,
            0.01,
        );
        let tgs = [
            Event::Started(Started {
                input: "art.gif".into(),
                output: "art.sticker.tgs".into(),
                format: Format::Tgs,
                target: Target::Sticker,
                limit: 65_536,
                details: StartDetails::Tgs { report: Box::new(report) },
            }),
            Event::TooLarge { bytes: 70_000 },
            Event::Reduced { step: step.clone() },
            Event::Packing,
            Event::Missed { bytes: Some(70_000), aim: 62_914 },
            Event::Advanced { fraction: 0.5, stage_fraction: 0.25 },
            Event::Warning { message: "a warning".into() },
            Event::Finished(Outcome {
                input: "art.gif".into(),
                output: "art.sticker.tgs".into(),
                format: Format::Tgs,
                target: Target::Sticker,
                bytes: 60_000,
                limit: 65_536,
                changed: true,
                issues: Vec::new(),
                details: Details::Tgs {
                    json_bytes: 300_000,
                    steps: vec![step],
                    layers: 3,
                    rectangles: 40,
                    drawing: crate::tgs::Drawing::new(art, false),
                },
            }),
            Event::Error(Failure { message: "it failed".into(), input: None }),
        ]
        .map(|event| serde_json::to_value(event).unwrap());
        let still = [
            Event::Started(Started {
                input: "in.png".into(),
                output: "in.sticker.webp".into(),
                format: Format::Webp,
                target: Target::Emoji,
                limit: 524_288,
                details: StartDetails::Webp {},
            }),
            Event::Finished(Outcome {
                input: "in.png".into(),
                output: "in.sticker.webp".into(),
                format: Format::Webp,
                target: Target::Emoji,
                bytes: 100_000,
                limit: 524_288,
                changed: true,
                issues: vec![Issue::WrongSize { width: 512, height: 384, side: 100, exact: true }],
                details: Details::Webp {
                    width: 512,
                    height: 384,
                    encoding: Encoding::Lossy { quality: 80 },
                },
            }),
            Event::Warning { message: "a warning".into() },
            Event::Error(Failure { message: "it failed".into(), input: None }),
        ]
        .map(|event| serde_json::to_value(event).unwrap());
        webm.into_iter().chain(tgs).chain(still).collect()
    }

    /// What `inspect`, `spoof` and `ffmpeg status` print besides events.
    fn other_samples() -> Vec<Value> {
        let info = crate::webm::container::WebmInfo {
            doc_type: "webm".into(),
            file_size: 1000,
            timestamp_scale_ns: 1_000_000,
            header_duration: Some(0.5),
            content_duration: Some(5.0),
            title: None,
            muxing_app: Some("tgradish".into()),
            writing_app: None,
            duration_tags: vec!["00:00:05.000000000".into()],
            signature: None,
            video: Some(crate::webm::container::VideoTrack {
                number: 1,
                codec_id: "V_VP9".into(),
                width: 512,
                height: 512,
                alpha: true,
                default_duration_ns: Some(33_333_333),
                uid: None,
            }),
            video_frames: 150,
            audio_tracks: 0,
            other_tracks: 0,
            truncated: false,
        };
        let issues = crate::telegram::check(&info, crate::telegram::Target::Emoji);
        let target = crate::telegram::Target::Emoji;
        let inspected = crate::inspect::Inspected::Webm { target, info: Box::new(info), issues };
        let report = crate::webm::container::PatchReport {
            old_duration: Some(5.0),
            new_duration: Some(0.5),
            duration_tags_patched: 1,
            duration_tags_skipped: 0,
            info_moved: false,
            signature_written: true,
        };
        let ffmpeg = crate::ffmpeg::Ffmpeg {
            ffmpeg: "/usr/bin/ffmpeg".into(),
            ffprobe: "/usr/bin/ffprobe".into(),
            source: crate::ffmpeg::FfmpegSource::System,
        };
        let capabilities =
            crate::ffmpeg::Capabilities { version: "ffmpeg 8".into(), libvpx_vp9: true };
        let spoofed = SpoofEvent::Finished {
            input: "in.webm".into(),
            output: "in.spoofed.webm".into(),
            report: report.clone(),
            issues: vec![Issue::TooLong { seconds: 5.0, max: 3.0 }],
        };
        vec![
            serde_json::to_value(inspected).unwrap(),
            serde_json::to_value(report).unwrap(),
            serde_json::to_value(crate::backend::BackendInfo::Process { ffmpeg }).unwrap(),
            serde_json::to_value(capabilities).unwrap(),
            serde_json::to_value(spoofed).unwrap(),
        ]
    }

    fn kebab(name: &str) -> bool {
        !name.contains('_') && !name.chars().any(char::is_uppercase)
    }

    /// Every key of `value`'s objects, at any depth.
    fn keys(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    found.push(key.clone());
                    keys(value, found);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| keys(item, found)),
            _ => {}
        }
    }

    /// The names a JSON Schema gives what it describes: its properties'
    /// and its values', but not the names of its definitions.
    fn schema_names(schema: &Value, found: &mut Vec<String>) {
        let strings = |value: &Value| -> Vec<String> {
            let items = value.as_array().map_or(&[][..], Vec::as_slice);
            items.iter().chain([value]).filter_map(Value::as_str).map(str::to_owned).collect()
        };
        match schema {
            Value::Object(map) => {
                for (key, value) in map {
                    match key.as_str() {
                        "properties" => {
                            for (name, property) in value.as_object().into_iter().flatten() {
                                found.push(name.clone());
                                schema_names(property, found);
                            }
                        }
                        "required" | "const" | "enum" => found.extend(strings(value)),
                        _ => schema_names(value, found),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| schema_names(item, found)),
            _ => {}
        }
    }

    /// `spoof --json` says what it wrote as conversions do, with the issue
    /// that makes it exit 3.
    #[test]
    fn prints_what_spoofing_did_as_an_event() {
        let line = serde_json::to_string(other_samples().last().unwrap()).unwrap();
        let start =
            r#"{"event":"finished","input":"in.webm","output":"in.spoofed.webm","report":{"#;
        assert!(line.starts_with(start), "{line}");
        let issues = r#""issues":[{"kind":"too-long","seconds":5.0,"max":3.0,"severity":"error","message":"metadata duration is 5.000 s (limit is 3 s)"}]}"#;
        assert!(line.ends_with(issues), "{line}");
    }

    #[test]
    fn describes_exactly_the_options_each_format_takes() {
        let description = describe(&Presets::builtin(), &Config::default());
        for format in description.formats {
            let schema = serde_json::to_value(&format.options).unwrap();
            let properties: Vec<&str> =
                schema["properties"].as_object().unwrap().keys().map(String::as_str).collect();
            let refused = format.format.unused(properties.iter().copied());
            assert!(refused.is_empty(), "{:?} refuses {refused:?}", format.format);
            let all: Vec<String> = [schema_for!(Options), schema_for!(tgs::Options)]
                .iter()
                .flat_map(|schema| schema.get("properties").unwrap().as_object().unwrap().keys())
                .cloned()
                .collect();
            for key in all.iter().filter(|key| format.format.uses(key)) {
                assert!(properties.contains(&key.as_str()), "{:?} lacks {key}", format.format);
            }
            // every reference still leads somewhere
            for reference in references(&schema) {
                let name = reference.trim_start_matches("#/$defs/");
                assert!(schema["$defs"].get(name).is_some(), "{:?}: {reference}", format.format);
            }
        }
    }

    #[test]
    fn names_everything_in_kebab_case() {
        let description = describe(&Presets::builtin(), &Config::default());
        let description = serde_json::to_value(&description).unwrap();
        let mut names = Vec::new();
        for format in description["formats"].as_array().unwrap() {
            schema_names(&format["options"], &mut names);
        }
        schema_names(&description["events"], &mut names);
        // the description itself, as presets, and what each stream prints
        schema_names(&description["spoof-events"], &mut names);
        let mut described = json!({ "description": description });
        described["description"]["formats"] = json!([]);
        described["description"]["events"] = json!({});
        described["description"]["spoof-events"] = json!({});
        keys(&described, &mut names);
        for sample in samples().into_iter().chain(other_samples()) {
            keys(&sample, &mut names);
            for value in ["event", "format", "kind"].map(|tag| &sample[tag]) {
                names.extend(value.as_str().map(str::to_owned));
            }
            for issue in sample["issues"].as_array().into_iter().flatten() {
                names.extend(
                    ["kind", "severity"]
                        .map(|key| issue[key].as_str().map(str::to_owned))
                        .into_iter()
                        .flatten(),
                );
            }
        }
        assert!(names.len() > 100, "{names:?}");
        let wrong: Vec<_> = names.iter().filter(|name| !kebab(name)).collect();
        assert!(wrong.is_empty(), "not kebab-case: {wrong:?}");
    }

    #[test]
    fn prints_each_event_as_front_ends_read_it() {
        // the shapes the README and front-ends follow; a start's plan and
        // report are pinned by their keys, below
        let expected = [
            None,
            Some(
                r#"{"event":"attempt-started","attempt":1,"params":{"fps":30.0,"length":2.0,"rate":{"mode":"bitrate","kbps":500.0}}}"#,
            ),
            Some(
                r#"{"event":"progress","attempt":1,"pass":1,"passes":2,"fraction":0.5,"can-settle":false}"#,
            ),
            Some(
                r#"{"event":"attempt-finished","attempt":1,"params":{"fps":30.0,"length":2.0,"rate":{"mode":"bitrate","kbps":500.0}},"bytes":300000,"fits":false,"can-settle":false}"#,
            ),
            Some(r#"{"event":"scored","attempt":1,"ssim":0.95}"#),
            Some(
                r#"{"event":"decided","decision":{"reason":"in-band","attempt":1,"share":0.99,"least":0.96},"message":"stopped: attempt 1 uses 99.0% of the limit (target >= 96.0%)"}"#,
            ),
            Some(r#"{"event":"warning","message":"a warning"}"#),
            Some(r#"{"event":"log","line":"a line"}"#),
            Some(
                r#"{"event":"finished","input":"in.mp4","output":"in.sticker.webm","format":"webm","target":"sticker","bytes":250000,"limit":262144,"changed":false,"issues":[{"kind":"too-big","bytes":300000,"max":262144,"severity":"error","message":"293.0 KiB exceeds the 256 KiB limit"}],"attempt":2,"params":{"fps":30.0,"length":2.0,"rate":{"mode":"crf","crf":32}},"spoofed":false,"settled":false,"temp-dir":"/tmp/tgradish-x"}"#,
            ),
            Some(r#"{"event":"error","message":"it failed","input":"in.mp4"}"#),
            None,
            Some(r#"{"event":"too-large","bytes":70000}"#),
            Some(
                r#"{"event":"reduced","step":{"reduction":"despeckle","bytes":60000,"distortion":0.01}}"#,
            ),
            Some(r#"{"event":"packing"}"#),
            Some(r#"{"event":"missed","bytes":70000,"aim":62914}"#),
            Some(r#"{"event":"advanced","fraction":0.5,"stage-fraction":0.25}"#),
            Some(r#"{"event":"warning","message":"a warning"}"#),
            Some(
                r#"{"event":"finished","input":"art.gif","output":"art.sticker.tgs","format":"tgs","target":"sticker","bytes":60000,"limit":65536,"changed":true,"issues":[],"json-bytes":300000,"steps":[{"reduction":"despeckle","bytes":60000,"distortion":0.01}],"layers":3,"rectangles":40}"#,
            ),
            Some(r#"{"event":"error","message":"it failed","input":null}"#),
            Some(
                r#"{"event":"started","input":"in.png","output":"in.sticker.webp","format":"webp","target":"emoji","limit":524288}"#,
            ),
            Some(
                r#"{"event":"finished","input":"in.png","output":"in.sticker.webp","format":"webp","target":"emoji","bytes":100000,"limit":524288,"changed":true,"issues":[{"kind":"wrong-size","width":512,"height":384,"side":100,"exact":true,"severity":"error","message":"size is 512×384, expected 100×100"}],"width":512,"height":384,"encoding":{"kind":"lossy","quality":80}}"#,
            ),
            Some(r#"{"event":"warning","message":"a warning"}"#),
            Some(r#"{"event":"error","message":"it failed","input":null}"#),
        ];
        let samples = samples();
        assert_eq!(samples.len(), expected.len());
        for (sample, expected) in samples.iter().zip(expected) {
            match expected {
                Some(line) => assert_eq!(serde_json::to_string(sample).unwrap(), line),
                None => assert_eq!(sample["event"], "started"),
            }
        }
        let keys =
            |value: &Value| -> Vec<String> { value.as_object().unwrap().keys().cloned().collect() };
        let head = ["event", "input", "output", "format", "target", "limit"];
        assert_eq!(keys(&samples[0]), [&head[..], &["plan"]].concat());
        assert_eq!(samples[0]["input"], "in.mp4");
        for key in ["input", "output", "source", "target", "scaled-width", "fit-range", "watermark"]
        {
            assert!(samples[0]["plan"].get(key).is_some(), "{key}");
        }
        let tgs = samples.iter().filter(|sample| sample["event"] == "started").nth(1).unwrap();
        assert_eq!(keys(tgs), [&head[..], &["report"]].concat());
    }
}

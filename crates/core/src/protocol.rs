//! Machine-readable description of tgradish for front-ends that run the
//! CLI, printed by `tgradish describe`.

use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, schema_for};
use serde::Serialize;

use crate::events::Event;
use crate::options::Options;
use crate::presets::{Format, Presets};
use crate::tgs::{TgsEvent, TgsOptions};

/// Bumped on incompatible changes to the description, events or CLI flags
/// used by front-ends. 2: `.tgs` output, formats and preset formats. 3:
/// presets hold options for both formats, and say nothing about the target.
/// 4: image output, with WebM's options.
pub const PROTOCOL_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
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
#[serde(rename_all = "kebab-case")]
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
#[serde(rename_all = "kebab-case")]
pub struct PresetDescription {
    pub name: String,
    pub description: String,
    pub builtin: bool,
    /// Options for WebM and image results, with everything the preset
    /// extends applied; `None` if broken.
    pub webm: Option<Options>,
    /// The same for `.tgs` results.
    pub tgs: Option<TgsOptions>,
    /// Why the preset cannot be used, if it is broken for either format.
    pub error: Option<String>,
}

/// The JSON Schema of `T` as it is written, rather than read: of what
/// tgradish prints.
fn printed<T: JsonSchema>() -> Schema {
    SchemaSettings::default().for_serialize().into_generator().into_root_schema_for::<T>()
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
                events: printed::<Event>(),
            },
            FormatDescription {
                format: Format::Tgs,
                description: "Animated sticker, from pixel art",
                output_extension: Format::Tgs.extension(),
                default_preset: tgs.to_string(),
                options: schema_for!(TgsOptions),
                events: printed::<TgsEvent>(),
            },
            FormatDescription {
                format: Format::Image,
                description: "Image sticker or emoji, from one frame of a video or an image: \
                              WebM's options where a still picture has them, start being \
                              the frame's time",
                output_extension: Format::Image.extension(),
                default_preset: webm.to_string(),
                options: schema_for!(Options),
                events: printed::<crate::still::StillEvent>(),
            },
        ],
        presets,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::convert::{Plan, Request};
    use crate::events::{Failure, Params, Rate};
    use crate::ffmpeg::Probe;
    use crate::still::{Encoding, Outcome, StillEvent};
    use crate::telegram::Issue;

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
        crate::convert::plan(&Request::new("in.mp4".into()), probe()).unwrap().0
    }

    /// One of each event of each stream, as `convert --json` prints them.
    pub(crate) fn samples() -> Vec<Value> {
        let params = Params { fps: 30.0, length: 2.0, rate: Rate::Bitrate(500.0) };
        let too_big = Issue::TooBig { bytes: 300_000, max: 262_144 };
        let webm = [
            Event::Started { plan: Box::new(plan()) },
            Event::AttemptStarted { attempt: 1, params },
            Event::Progress { attempt: 1, pass: 1, passes: 2, fraction: 0.5 },
            Event::AttemptFinished { attempt: 1, params, bytes: 300_000, fits: false },
            Event::Scored { attempt: 1, ssim: 0.95 },
            Event::Warning { message: "a warning".into() },
            Event::Log { line: "a line".into() },
            Event::Finished {
                output: "in.sticker.webm".into(),
                bytes: 250_000,
                attempt: 2,
                params: Params { rate: Rate::Crf(32), ..params },
                spoofed: false,
                issues: vec![too_big.clone()],
            },
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
        let report = tgradish_tgs::normalise(&animation, &Default::default()).unwrap().1;
        let step = tgradish_tgs::sticker::Step::new(crate::tgs::Reduction::Despeckle, 60_000, 0.01);
        let tgs = [
            TgsEvent::Started { input: "art.gif".into(), report: Box::new(report) },
            TgsEvent::TooLarge { bytes: 70_000 },
            TgsEvent::Reduced { step: step.clone() },
            TgsEvent::Packing,
            TgsEvent::Warning { message: "a warning".into() },
            TgsEvent::Finished {
                output: "art.sticker.tgs".into(),
                bytes: 60_000,
                json_bytes: 300_000,
                lossy: true,
                steps: vec![step],
                layers: 3,
                rectangles: 40,
                issues: Vec::new(),
            },
            TgsEvent::Error(Failure { message: "it failed".into(), input: None }),
        ]
        .map(|event| serde_json::to_value(event).unwrap());
        let still = [
            StillEvent::Finished(Outcome {
                output: "in.sticker.webp".into(),
                bytes: 100_000,
                width: 512,
                height: 384,
                encoding: Encoding::Lossy { quality: 80 },
                issues: vec![Issue::WrongSize { width: 512, height: 384, side: 100, exact: true }],
                warnings: vec!["a warning".into()],
            }),
            StillEvent::Error(Failure { message: "it failed".into(), input: None }),
        ]
        .map(|event| serde_json::to_value(event).unwrap());
        webm.into_iter().chain(tgs).chain(still).collect()
    }

    /// What `inspect`, `spoof` and `ffmpeg status` print besides events.
    fn other_samples() -> Vec<Value> {
        let info = crate::webm::WebmInfo {
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
            video: Some(crate::webm::VideoTrack {
                number: 1,
                codec_id: "V_VP9".into(),
                width: 512,
                height: 512,
                alpha: true,
                default_duration_ns: Some(33_333_333),
                uid: Some(1),
            }),
            video_frames: 150,
            audio_tracks: 0,
            other_tracks: 0,
            truncated: false,
        };
        let issues = crate::telegram::check(&info, crate::telegram::Target::Emoji);
        let target = crate::telegram::Target::Emoji;
        let inspected = crate::inspect::Inspected::Webm { target, info: Box::new(info), issues };
        let report = crate::webm::PatchReport {
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
        vec![
            serde_json::to_value(inspected).unwrap(),
            serde_json::to_value(report).unwrap(),
            serde_json::to_value(crate::backend::BackendInfo::Process { ffmpeg }).unwrap(),
            serde_json::to_value(capabilities).unwrap(),
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

    #[test]
    fn names_everything_in_kebab_case() {
        let description = describe(&Presets::builtin(), ["balanced", "best"]);
        let description = serde_json::to_value(&description).unwrap();
        let mut names = Vec::new();
        for format in description["formats"].as_array().unwrap() {
            schema_names(&format["options"], &mut names);
            schema_names(&format["events"], &mut names);
        }
        // the description itself, as presets, and what each stream prints
        let mut described = json!({ "description": description });
        described["description"]["formats"] = json!([]);
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
}

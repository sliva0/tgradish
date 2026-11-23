//! The types and path serialisers `--json` output needs beyond the library's
//! own, and the conventions every `--json` command follows:
//!
//! - All JSON keys and enum values use kebab-case.
//! - Every `--json` command prints JSON Lines (one compact JSON object per
//!   line). `convert` and `watch` emit [`Event`](crate::Event)s (`started`
//!   first, `finished` last, carrying `input`, `output`, `format`, `target`,
//!   `limit`, plus format-specific fields). `inspect` prints one file per line, `preset list`
//!   one [`Listed`](crate::presets::Listed) preset per line, `licenses` one
//!   notice per line, `spoof` one [`SpoofEvent`], and other commands one
//!   object.
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

use serde::Serialize;

/// JSON event emitted by `tgradish spoof --json`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum SpoofEvent {
    /// Spoofed file written successfully.
    Finished {
        /// Source WebM path.
        #[serde(serialize_with = "crate::wire::serialize_path")]
        input: PathBuf,
        /// Destination WebM path.
        #[serde(serialize_with = "crate::wire::serialize_path")]
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

/// Error event payload emitted by CLI `--json` commands (`convert`, `watch`,
/// `inspect`, `spoof`). Library functions return `Err` instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Failure {
    /// Human-readable error message.
    pub message: String,
    /// Failing input path, or `None` for command-wide errors.
    #[serde(serialize_with = "crate::wire::serialize_optional_path")]
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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::Event;
    use crate::events::{Details, Outcome, StartDetails, Started};
    use crate::ffmpeg::Probe;
    use crate::presets::Format;
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
    fn names_everything_in_kebab_case() {
        // every option, set or not, and the presets as they are listed
        let mut names: Vec<String> =
            crate::webm::Options::keys().chain(crate::tgs::Options::keys()).collect();
        let listed = serde_json::to_value(crate::presets::Presets::builtin().listed()).unwrap();
        keys(&listed, &mut names);
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
        // the shapes the README and scripts follow; a start's plan and
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

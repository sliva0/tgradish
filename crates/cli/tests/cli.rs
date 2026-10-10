//! Runs the built `tgradish` binary. Conversion tests skip themselves when
//! ffmpeg or the local `references/` media is missing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn tgradish(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(args)
        // don't pick up the developer's own config
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .output()
        .unwrap()
}

fn json_lines(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn reference(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references").join(name);
    let ffmpeg = Command::new("ffmpeg").arg("-version").output().is_ok();
    if !(ffmpeg && path.exists()) {
        eprintln!("skipped: needs ffmpeg on PATH and references/{name}");
        return None;
    }
    Some(path)
}

#[test]
fn describes_protocol() {
    let output = tgradish(&["describe"]);
    assert!(output.status.success());
    let description: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(description["protocol"], 3);
    assert_eq!(description["default_format"], "webm");
    let formats = description["formats"].as_array().unwrap();
    let format = |name: &str| formats.iter().find(|f| f["format"] == name).unwrap();
    assert_eq!(format("webm")["default_preset"], "balanced");
    assert!(format("webm")["options"]["properties"]["fit"].is_object());
    assert!(format("webm")["options"]["properties"]["crop"].is_object());
    assert_eq!(format("tgs")["default_preset"], "best");
    assert_eq!(format("tgs")["output_extension"], "tgs");
    assert!(format("tgs")["options"]["properties"]["reductions"].is_object());
    let presets = description["presets"].as_array().unwrap();
    let preset = |name: &str| presets.iter().find(|p| p["name"] == name).unwrap();
    // presets say how, for both formats, and nothing about the target
    assert_eq!(preset("fast")["webm"]["speed"], "fast");
    assert_eq!(preset("fast")["webm"]["fit"], "bitrate");
    assert_eq!(preset("fast")["tgs"]["speed"], "fast");
    assert!(preset("best")["webm"]["target"].is_null());
    assert!(preset("best")["error"].is_null());
}

#[test]
fn reports_errors_as_json() {
    let output = tgradish(&["--json", "convert", "missing.mp4", "--preset", "nope"]);
    assert_eq!(output.status.code(), Some(1));
    let lines = json_lines(&output);
    assert_eq!(lines[0]["event"], "error");
    assert!(lines[0]["message"].as_str().unwrap().contains("unknown preset"));
}

#[test]
fn rejects_bad_flags() {
    let output = tgradish(&["convert", "a.mp4", "--crf", "64"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn converts_with_json_events() {
    let Some(input) = reference("uhh.mp4") else { return };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.webm");
    let output = tgradish(&[
        "--json",
        "convert",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--preset",
        "fast",
        "--options-json",
        r#"{"length": 1.5, "title": "from json"}"#,
        "--title",
        "from flag",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));

    let events = json_lines(&output);
    assert_eq!(events.first().unwrap()["event"], "started");
    let plan = &events[0]["plan"];
    assert_eq!(plan["length"], 1.5);
    assert_eq!(plan["title"], "from flag", "flags win over --options-json");
    let finished = events.last().unwrap();
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["issues"], Value::Array(vec![]));

    let inspected = tgradish(&["--json", "inspect", out.to_str().unwrap()]);
    let info = &json_lines(&inspected)[0];
    assert_eq!(info["info"]["title"], "from flag");
    assert_eq!(info["issues"], Value::Array(vec![]));
}

#[test]
fn cancels_from_stdin() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;

    let Some(input) = reference("pig.mp4") else { return };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.webm");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--json", "convert", input.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut last = Value::Null;
    for line in stdout.lines() {
        last = serde_json::from_str(&line.unwrap()).unwrap();
        if last["event"] == "progress" {
            // ignore errors: the process may already be gone
            let _ = writeln!(stdin, "cancel");
        }
    }
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));
    assert_eq!(last["event"], "error");
    assert!(!out.exists());
}

#[test]
fn events_schema_includes_errors() {
    let output = tgradish(&["describe"]);
    let description: Value = serde_json::from_slice(&output.stdout).unwrap();
    for format in description["formats"].as_array().unwrap() {
        let events = format["events"]["oneOf"].as_array().unwrap();
        assert!(events.iter().any(|e| e["properties"]["event"]["const"] == "error"));
    }
}

/// A 4x4 PNG: a 2x2 square of `colour` on transparency.
fn square_png(path: &Path, colour: [u8; 4]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, 4, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let pixels: Vec<u8> = (0..16)
        .flat_map(|i| {
            if (1..3).contains(&(i % 4)) && (1..3).contains(&(i / 4)) { colour } else { [0; 4] }
        })
        .collect();
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

#[test]
fn makes_animated_stickers() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("2.png"), dir.path().join("10.png"));
    square_png(&a, [255, 0, 0, 255]);
    square_png(&b, [0, 0, 255, 255]);
    let out = dir.path().join("out.tgs");
    let output = tgradish(&[
        "--json",
        "convert",
        dir.path().to_str().unwrap(),
        "--sequence",
        "-o",
        out.to_str().unwrap(),
        "--fps",
        "4",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let lines = json_lines(&output);
    assert_eq!(lines[0]["event"], "started");
    // 2x2 visible pixels, two frames of 15 ticks; 2.png comes first
    assert_eq!(lines[0]["report"]["frames"], 2);
    assert_eq!(lines[0]["report"]["ticks"], 30);
    let finished = lines.last().unwrap();
    assert_eq!(
        (finished["event"].as_str(), finished["lossy"].as_bool()),
        (Some("finished"), Some(false))
    );
    assert_eq!(finished["issues"], serde_json::json!([]));

    let inspected = tgradish(&["--json", "inspect", out.to_str().unwrap()]);
    let lines = json_lines(&inspected);
    assert_eq!(lines[0]["format"], "tgs");
    assert_eq!(lines[0]["stats"]["frames"], 30.0);
    assert_eq!(lines[0]["issues"], serde_json::json!([]));

    // WebM flags don't mix with .tgs output, nor the reverse
    let mixed = tgradish(&["convert", a.to_str().unwrap(), "-o", "x.tgs", "--crf", "20"]);
    assert_eq!(mixed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&mixed.stderr).contains("--crf"));
    let sequence = tgradish(&["convert", a.to_str().unwrap(), "--sequence"]);
    let stderr = String::from_utf8_lossy(&sequence.stderr);
    assert!(stderr.contains("--format tgs"), "{stderr}");
}

#[test]
fn reports_batch_failures_per_input() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.webm");
    let other = dir.path().join("other.webm");
    std::fs::write(&other, b"not a webm").unwrap();

    let output =
        tgradish(&["--json", "inspect", missing.to_str().unwrap(), other.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let lines = json_lines(&output);
    assert_eq!(lines.len(), 2, "only per-file lines on stdout");
    assert!(lines.iter().all(|line| line["error"].is_string() && line["file"].is_string()));
}

/// An "ffmpeg" without the VP9 encoder must give one JSON document.
#[cfg(unix)]
#[test]
fn status_without_vp9_is_one_document() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    for name in ["ffmpeg", "ffprobe"] {
        let path = dir.path().join(name);
        std::fs::write(&path, "#!/bin/sh\necho 'ffmpeg version fake'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output =
        tgradish(&["--json", "--ffmpeg", dir.path().to_str().unwrap(), "ffmpeg", "status"]);
    assert_eq!(output.status.code(), Some(1));
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["capabilities"]["libvpx_vp9"], false);
    assert_eq!(status["capabilities"]["version"], "ffmpeg version fake");
}

#[cfg(unix)]
#[test]
fn watch_converts_new_files() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let Some(input) = reference("uhh.mp4") else { return };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("old.mp4"), b"there before watching").unwrap();
    let out = dir.path().join("out");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--json", "watch", dir.path().to_str().unwrap(), "--interval", "0.2", "-r"])
        .args(["--output-dir", out.to_str().unwrap(), "--preset", "fast", "--length", "0.5"])
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // files there when watching starts are left alone, so let it start first
    std::thread::sleep(std::time::Duration::from_secs(2));
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::copy(&input, dir.path().join("sub/new.mp4")).unwrap();

    let stdout = BufReader::new(child.stdout.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            let event: Value = serde_json::from_str(&line).unwrap();
            let done = matches!(event["event"].as_str(), Some("finished" | "error"));
            let _ = tx.send(event);
            if done {
                break;
            }
        }
    });
    let mut events = Vec::new();
    while let Ok(event) = rx.recv_timeout(std::time::Duration::from_secs(60)) {
        events.push(event);
    }
    Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap();
    let status = child.wait().unwrap();

    let last = events.last().expect("no events within a minute");
    assert_eq!(last["event"], "finished", "{last}");
    // subdirectories are kept under --output-dir
    assert!(out.join("sub/new.sticker.webm").is_file());
    assert!(!out.join("old.sticker.webm").exists(), "files from before are left alone");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn refuses_to_merge_outputs_of_same_named_inputs() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for sub in ["a", "b"] {
        std::fs::create_dir(dir.path().join(sub)).unwrap();
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=64x64:d=1", "-frames:v", "1"])
            .arg(dir.path().join(sub).join("same.png"))
            .status()
            .unwrap();
        assert!(status.success());
    }
    let out = dir.path().join("out");
    let output = tgradish(&[
        "--json",
        "convert",
        dir.path().join("a/same.png").to_str().unwrap(),
        dir.path().join("b/same.png").to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
        "--overwrite",
        "--preset",
        "fast",
        "--length",
        "0.2",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let events = json_lines(&output);
    let errors: Vec<_> = events.iter().filter(|e| e["event"] == "error").collect();
    assert_eq!(errors.len(), 1, "{events:?}");
    assert!(errors[0]["input"].as_str().unwrap().ends_with("b/same.png"));
    assert!(out.join("same.sticker.webm").is_file());
}

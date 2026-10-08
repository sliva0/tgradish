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
    assert_eq!(description["protocol"], 1);
    assert_eq!(description["default_preset"], "sticker");
    assert!(description["options"]["properties"]["fit"].is_object());
    let presets = description["presets"].as_array().unwrap();
    assert!(presets.iter().any(|p| p["name"] == "emoji" && p["options"]["target"] == "emoji"));
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

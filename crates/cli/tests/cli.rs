//! Runs the built `tgradish` binary. Conversion tests skip themselves when
//! ffmpeg is missing, or fail where `TGRADISH_REQUIRE_FFMPEG` says it must
//! be there.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tgradish_corpus::{Canvas, Clip};

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

/// A clip like phone footage, made once with ffmpeg and kept for later
/// runs, or `None` without ffmpeg to convert it with.
fn clip(kind: Clip) -> Option<PathBuf> {
    // a clip made by an earlier run is there without ffmpeg
    if !tgradish_corpus::ffmpeg_on_path() {
        return None;
    }
    tgradish_corpus::clip(kind, Path::new(env!("CARGO_TARGET_TMPDIR")))
}

#[test]
fn describes_protocol() {
    let output = tgradish(&["describe"]);
    assert!(output.status.success());
    let description: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(description["protocol"], 1);
    assert_eq!(description["default-format"], "webm");
    assert!(description["cancel"].as_str().unwrap().contains("--cancel-on-stdin"));
    let formats = description["formats"].as_array().unwrap();
    let format = |name: &str| formats.iter().find(|f| f["format"] == name).unwrap();
    assert_eq!(format("webm")["default-preset"], "balanced");
    assert!(format("webm")["options"]["properties"]["fit"].is_object());
    assert!(format("webm")["options"]["properties"]["crop"].is_object());
    assert_eq!(format("tgs")["default-preset"], "best");
    assert_eq!(format("tgs")["output-extension"], "tgs");
    assert!(format("tgs")["options"]["properties"]["reductions"].is_object());
    // images take WebM's options and presets
    assert_eq!(format("webp")["output-extension"], "webp");
    assert_eq!(format("webp")["default-preset"], "balanced");
    assert!(format("webp")["options"]["properties"]["start"].is_object());
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
fn carries_licences() {
    let output = tgradish(&["licenses"]);
    assert!(output.status.success());
    // prose says licences, and the command takes it too
    let alias = tgradish(&["licences"]);
    assert!(alias.status.success() && alias.stdout == output.stdout);
    let text = String::from_utf8_lossy(&output.stdout);
    for title in ["tgradish", "Font: Ubuntu Light, a subset", "libwebp: licence", "Rust crates"] {
        assert!(text.lines().any(|line| line == title), "no {title:?}");
    }
    // each crate's own license file, with who holds its copyright
    assert!(text.matches("used by:").count() > 40);
}

/// A reader that stops early, as `tgradish licenses | head -2`, ends the
/// command without a panic's message.
#[test]
fn stops_quietly_when_stdout_closes() {
    use std::io::Read;
    use std::process::Stdio;

    for args in [&["licenses"][..], &["--json", "describe"], &["preset", "list"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(args)
            .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let _ = stdout.read(&mut [0; 16]).unwrap();
        drop(stdout);
        let output = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.is_empty(), "{args:?}: {stderr}");
        assert_ne!(output.status.code(), Some(101), "{args:?}");
    }
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
    let Some(input) = clip(Clip::Square) else { return };
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

    let Some(input) = clip(Clip::Tall) else { return };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.webm");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--json", "convert", input.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .arg("--cancel-on-stdin")
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

/// Without `--cancel-on-stdin`, stdin is the caller's: a loop reading
/// files from it converts every one.
#[cfg(unix)]
#[test]
fn leaves_stdin_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = String::new();
    for name in ["a", "b", "c"] {
        let path = dir.path().join(format!("{name}.png"));
        square_png(&path, [255, 0, 0, 255]);
        list.push_str(&format!("{}\n", path.display()));
    }
    std::fs::write(dir.path().join("list"), list).unwrap();
    let script = r#"while read f; do "$0" --json convert "$f" --format tgs; done < "$1""#;
    let output = Command::new("sh")
        .args(["-c", script, env!("CARGO_BIN_EXE_tgradish")])
        .arg(dir.path().join("list"))
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .output()
        .unwrap();
    let finished = json_lines(&output).iter().filter(|e| e["event"] == "finished").count();
    assert_eq!(finished, 3, "{}", String::from_utf8_lossy(&output.stderr));
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
    let mut canvas = Canvas::new(4, 4);
    canvas.rect(1, 1, 2, 2, colour);
    std::fs::write(path, tgradish_corpus::encode::png(&canvas)).unwrap();
}

/// `frames` frames of 24x24 noise in four levels of each colour, in a
/// folder of their own: as a `.tgs` kept exact, far over 64 KiB.
fn noise(dir: &Path, frames: u32) -> PathBuf {
    let folder = dir.join("noise");
    std::fs::create_dir_all(&folder).unwrap();
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut level = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 4) as u8 * 85
    };
    for frame in 0..frames {
        let mut canvas = Canvas::new(24, 24);
        for y in 0..24 {
            for x in 0..24 {
                canvas.set(x, y, [level(), level(), level(), 255]);
            }
        }
        std::fs::write(folder.join(format!("{frame}.png")), tgradish_corpus::encode::png(&canvas))
            .unwrap();
    }
    folder
}

#[test]
fn exits_3_when_telegram_would_refuse_a_result() {
    let dir = tempfile::tempdir().unwrap();
    let art = noise(dir.path(), 24);
    let tgs = dir.path().join("noise.tgs");
    let made = |extra: &[&str]| {
        let mut args = vec!["convert", art.to_str().unwrap(), "--sequence"];
        args.extend(["-o", tgs.to_str().unwrap(), "-y", "--speed", "fast", "--fps", "24"]);
        args.extend(extra);
        tgradish(&args)
    };
    // made, as exact keeps it, and too large
    let output = made(&[]);
    assert_eq!(output.status.code(), Some(3), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(tgs.is_file());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("too large; --no-exact lets fitting change the art"), "{stderr}");
    assert!(!stderr.contains("Fitting"), "nothing is fitted: {stderr}");
    let output = made(&["--json"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(json_lines(&output).last().unwrap()["event"], "finished");
    // read, and refused
    assert_eq!(tgradish(&["inspect", tgs.to_str().unwrap()]).status.code(), Some(3));
    assert_eq!(tgradish(&["--json", "inspect", tgs.to_str().unwrap()]).status.code(), Some(3));
    // a failure beats a refusal
    let missing = dir.path().join("missing.tgs");
    let both = tgradish(&["inspect", tgs.to_str().unwrap(), missing.to_str().unwrap()]);
    assert_eq!(both.status.code(), Some(1));
    let png = dir.path().join("noise").join("0.png");
    let both = tgradish(&[
        "convert",
        png.to_str().unwrap(),
        missing.to_str().unwrap(),
        "--format",
        "tgs",
        "-O",
        dir.path().join("out").to_str().unwrap(),
    ]);
    assert_eq!(both.status.code(), Some(1));

    // a picture is judged as what it was made for
    if !tgradish_corpus::ffmpeg_on_path() {
        return;
    }
    let webp = dir.path().join("still.webp");
    let output = tgradish(&[
        "--ffmpeg-from",
        "system",
        "convert",
        png.to_str().unwrap(),
        "-o",
        webp.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(tgradish(&["inspect", webp.to_str().unwrap()]).status.code(), Some(0));
    let as_emoji = tgradish(&["inspect", webp.to_str().unwrap(), "--target", "emoji"]);
    assert_eq!(as_emoji.status.code(), Some(3));
}

/// WebM fitted to nothing, larger than an emoji may be.
#[test]
fn exits_3_for_webm_over_the_limit() {
    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("big.webm");
    let output = tgradish(&[
        "--ffmpeg-from",
        "system",
        "convert",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--target",
        "emoji",
        "--fit",
        "off",
        "--crf",
        "0",
        "--length",
        "1",
        "--speed",
        "fast",
    ]);
    assert_eq!(output.status.code(), Some(3), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Telegram will refuse it"));
    assert_eq!(tgradish(&["inspect", out.to_str().unwrap()]).status.code(), Some(3));
}

/// Every format's result is a line on stdout, which --quiet keeps, and
/// the rest goes to stderr or nowhere.
#[test]
fn reports_results_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("dot.png");
    square_png(&png, [255, 0, 0, 255]);
    let mut formats = vec!["tgs"];
    if tgradish_corpus::ffmpeg_on_path() {
        formats.extend(["webp", "webm"]);
    }
    for format in formats {
        let run = |quiet: bool| {
            let mut args = vec!["--ffmpeg-from", "system", "convert", png.to_str().unwrap()];
            args.extend(["--format", format, "-y", "--preset", "fast"]);
            if quiet {
                args.push("-q");
            }
            tgradish(&args)
        };
        for quiet in [false, true] {
            let output = run(quiet);
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let stdout = String::from_utf8(output.stdout).unwrap();
            let line = format!("dot.sticker.{format}");
            assert!(stdout.trim().starts_with("Finished") && stdout.contains(&line), "{stdout}");
            assert_eq!(stdout.lines().count(), 1, "{format}: {stdout}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!quiet || stderr.is_empty(), "{format}: {stderr}");
        }
    }
}

/// `-o` names a file of the format made, which is checked before any work.
#[test]
fn refuses_outputs_that_cant_hold_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("dot.png");
    square_png(&png, [255, 0, 0, 255]);
    let png = png.to_str().unwrap();
    let outdir = dir.path().join("outdir");
    std::fs::create_dir(&outdir).unwrap();
    let refused = |args: &[&str], why: &str| {
        let output = tgradish(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(why), "{args:?}: {stderr}");
        assert!(!stderr.contains("Converting"), "{args:?}: {stderr}");
    };
    let outdir = outdir.to_str().unwrap();
    for overwrite in [&[][..], &["-y"]] {
        let mut args = vec!["convert", png, "--format", "tgs", "-o", outdir];
        args.extend(overwrite);
        refused(&args, "is a directory: -o names the result's file, -O/--output-dir");
    }
    let mp4 = dir.path().join("x.mp4");
    refused(&["convert", png, "-o", mp4.to_str().unwrap()], "doesn't end in .webm");
    let tgs = dir.path().join("x.tgs");
    refused(
        &["convert", png, "--format", "webm", "-o", tgs.to_str().unwrap()],
        "is a .tgs file, but this converts to webm",
    );
    assert!(!mp4.exists() && !tgs.exists());
    // the extension still picks the format, in any case
    let upper = dir.path().join("x.TGS");
    let made = tgradish(&["convert", png, "-o", upper.to_str().unwrap()]);
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    refused(&["spoof", upper.to_str().unwrap(), "-o", outdir], "is a directory");
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
        (finished["event"].as_str(), finished["changed"].as_bool()),
        (Some("finished"), Some(false))
    );
    assert_eq!(finished["issues"], serde_json::json!([]));

    let inspected = tgradish(&["--json", "inspect", out.to_str().unwrap()]);
    let lines = json_lines(&inspected);
    assert_eq!(lines[0]["format"], "tgs");
    assert_eq!(lines[0]["info"]["frames"], 30.0);
    assert_eq!(lines[0]["issues"], serde_json::json!([]));

    // WebM flags don't mix with .tgs output, nor the reverse
    let mixed = tgradish(&["convert", a.to_str().unwrap(), "-o", "x.tgs", "--crf", "20"]);
    assert_eq!(mixed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&mixed.stderr).contains("--crf"));
}

#[test]
fn makes_video_and_image_stickers_of_sequences() {
    if !tgradish_corpus::ffmpeg_on_path() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let frames = dir.path().join("walk");
    std::fs::create_dir(&frames).unwrap();
    square_png(&frames.join("2.png"), [255, 0, 0, 255]);
    square_png(&frames.join("10.png"), [0, 0, 255, 255]);
    // two frames at 4 a second: half a second
    let webm = dir.path().join("walk.webm");
    let made = tgradish(&[
        "--ffmpeg-from",
        "system",
        "convert",
        frames.to_str().unwrap(),
        "--sequence",
        "-o",
        webm.to_str().unwrap(),
        "--fps",
        "4",
        "--preset",
        "fast",
    ]);
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let inspected = tgradish(&["--json", "inspect", webm.to_str().unwrap()]);
    let info = &json_lines(&inspected)[0];
    assert_eq!(info["info"]["video-frames"], 2, "{info}");
    // a picture of the first, 2.png
    let webp = dir.path().join("walk.webp");
    let made = tgradish(&[
        "--json",
        "--ffmpeg-from",
        "system",
        "convert",
        frames.to_str().unwrap(),
        "--sequence",
        "-o",
        webp.to_str().unwrap(),
    ]);
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    assert_eq!(&std::fs::read(&webp).unwrap()[8..12], b"WEBP");
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
    let failed = |line: &Value| line["event"] == "error" && line["message"].is_string();
    assert!(lines.iter().all(|line| failed(line) && line["input"].is_string()));
    // one input fails as one of several does
    let output = tgradish(&["--json", "inspect", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let lines = json_lines(&output);
    assert_eq!(lines.len(), 1);
    assert!(failed(&lines[0]) && lines[0]["input"] == missing.to_str().unwrap());
    let output = tgradish(&["--json", "convert", missing.to_str().unwrap(), "--format", "tgs"]);
    let lines = json_lines(&output);
    assert_eq!((output.status.code(), lines.len()), (Some(1), 1));
    assert!(failed(&lines[0]) && lines[0]["input"] == missing.to_str().unwrap());
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
    assert_eq!(status["capabilities"]["libvpx-vp9"], false);
    assert_eq!(status["capabilities"]["version"], "ffmpeg version fake");
}

#[cfg(unix)]
#[test]
fn watch_converts_new_files() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let Some(input) = clip(Clip::Square) else { return };
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

/// `tgradish watch` on a directory, its JSON events read as they come.
#[cfg(unix)]
struct Watching {
    child: std::process::Child,
    events: std::sync::mpsc::Receiver<Value>,
}

#[cfg(unix)]
impl Watching {
    fn start(dir: &Path, args: &[&str]) -> Watching {
        use std::io::{BufRead, BufReader};
        use std::process::Stdio;

        let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(["--json", "watch", dir.to_str().unwrap(), "--interval", "0.1"])
            .args(args)
            .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, events) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                let _ = tx.send(serde_json::from_str::<Value>(&line).unwrap());
            }
        });
        // files there when watching starts are left alone, so let it start
        std::thread::sleep(std::time::Duration::from_millis(500));
        Watching { child, events }
    }

    /// The events up to the first that `until` takes, within 30 seconds.
    fn until(&self, until: impl Fn(&Value) -> bool) -> Vec<Value> {
        let mut seen = Vec::new();
        while let Ok(event) = self.events.recv_timeout(std::time::Duration::from_secs(30)) {
            let done = until(&event);
            seen.push(event);
            if done {
                return seen;
            }
        }
        panic!("not within 30 seconds, after {seen:?}");
    }

    /// The events in the next `seconds`.
    fn during(&self, seconds: f64) -> Vec<Value> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
        let mut seen = Vec::new();
        while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(event) => seen.push(event),
                Err(_) => break,
            }
        }
        seen
    }

    /// Stops it with Ctrl-C, as a user would.
    fn stop(self) -> Output {
        Command::new("kill").args(["-INT", &self.child.id().to_string()]).status().unwrap();
        self.child.wait_with_output().unwrap()
    }
}

#[cfg(unix)]
fn named(event: &Value, kind: &str, name: &str) -> bool {
    let input = event["input"].as_str().or(event["output"].as_str()).unwrap_or_default();
    event["event"] == kind && input.ends_with(name)
}

/// What watching converts: new files and links to them; never its own
/// results, nor hidden or half-downloaded files.
#[cfg(unix)]
#[test]
fn watch_converts_what_appears_and_only_that() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    // its results are pictures, which are inputs too but for their names
    let format = if tgradish_corpus::ffmpeg_on_path() { "webp" } else { "tgs" };
    let watching = Watching::start(dir.path(), &["--format", format, "--ffmpeg-from", "system"]);

    square_png(&dir.path().join("a.png"), [255, 0, 0, 255]);
    square_png(&dir.path().join(".hidden.png"), [255, 0, 0, 255]);
    square_png(&dir.path().join("b.png.part"), [255, 0, 0, 255]);
    square_png(&elsewhere.path().join("target.png"), [0, 0, 255, 255]);
    std::os::unix::fs::symlink(elsewhere.path().join("target.png"), dir.path().join("link.png"))
        .unwrap();
    let mut events = watching.until(|e| named(e, "finished", &format!("a.sticker.{format}")));
    events.extend(watching.until(|e| named(e, "finished", &format!("link.sticker.{format}"))));
    // nothing more: not the results, which scans see from now on
    events.extend(watching.during(1.5));
    let finished: Vec<&str> = events
        .iter()
        .filter(|e| e["event"] == "finished")
        .map(|e| e["output"].as_str().unwrap().rsplit('/').next().unwrap())
        .collect();
    let results = [format!("a.sticker.{format}"), format!("link.sticker.{format}")];
    assert_eq!(finished, results, "{events:?}");
    assert_eq!(watching.stop().status.code(), Some(0));
}

/// A file that changes while it is converted is converted again.
#[cfg(unix)]
#[test]
fn watch_converts_a_file_again_that_changed_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let watching = Watching::start(dir.path(), &["--format", "tgs"]);
    // noise takes a second or so as a .tgs
    let slow = noise(elsewhere.path(), 1).join("0.png");
    std::fs::copy(&slow, dir.path().join("c.png")).unwrap();
    watching.until(|e| named(e, "started", "c.png"));
    square_png(&dir.path().join("c.png"), [0, 255, 0, 255]);
    let events = watching.until(|e| e["event"] == "started");
    let again = events.last().unwrap();
    assert!(named(again, "started", "c.png"), "{events:?}");
    // the square's single colour, not the noise's
    assert_eq!(again["report"]["colours"], 1, "{again}");
    watching.until(|e| named(e, "finished", "c.sticker.tgs"));
    assert_eq!(watching.stop().status.code(), Some(0));
}

/// Stopping is the only way out of a watch, so it ends as done, waiting
/// or converting.
#[cfg(unix)]
#[test]
fn watch_stops_as_done() {
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    let watching = Watching::start(dir.path(), &["--format", "tgs"]);
    let stopped = watching.stop();
    assert_eq!(stopped.status.code(), Some(0));

    // converting, which takes long enough with --speed best
    let art = noise(dir.path(), 24);
    let watching = Watching::start(dir.path(), &["--format", "tgs", "--speed", "best"]);
    std::fs::rename(art.join("0.png"), dir.path().join("slow.png")).unwrap();
    watching.until(|e| named(e, "started", "slow.png"));
    let stopped = watching.stop();
    assert_eq!(stopped.status.code(), Some(0), "{}", String::from_utf8_lossy(&stopped.stderr));
    assert!(!dir.path().join("slow.sticker.tgs").exists());

    // and says so
    let child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["watch", dir.path().to_str().unwrap(), "--interval", "0.1"])
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Stopped"));
}

#[test]
fn refuses_to_merge_outputs_of_same_named_inputs() {
    if !tgradish_corpus::ffmpeg_on_path() {
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

#[test]
fn makes_image_stickers() {
    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    // the output's extension asks for an image
    let out = dir.path().join("frame.webp");
    let output = tgradish(&[
        "--json",
        "--ffmpeg-from",
        "system",
        "convert",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--start",
        "1",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let event: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(event["event"], "finished");
    assert_eq!((event["width"].as_u64(), event["height"].as_u64()), (Some(512), Some(512)));
    assert_eq!(event["encoding"]["kind"], "lossless");
    assert!(event["issues"].as_array().unwrap().is_empty());
    assert_eq!(&std::fs::read(&out).unwrap()[8..12], b"WEBP");
    // what an image doesn't use is refused, asked for by its kind too
    for format in ["webp", "image"] {
        let output =
            tgradish(&["convert", input.to_str().unwrap(), "--format", format, "--crf", "30"]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--crf can't be used for webp output"), "{stderr}");
    }
    // as is what JSON sets, which went past the flags' check before
    let json = r#"{"crf":30,"spoof":"always"}"#;
    let output =
        tgradish(&["convert", input.to_str().unwrap(), "--format", "webp", "--options-json", json]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"crf\" in --options-json, \"spoof\" in --options-json"), "{stderr}");
}

/// serde refuses paths that aren't Unicode, which every line names with
/// what isn't replaced, rather than the command failing.
#[cfg(unix)]
#[test]
fn names_files_whose_names_are_not_unicode() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join(std::ffi::OsStr::from_bytes(b"caf\xe9.png"));
    square_png(&input, [255, 0, 0, 255]);
    let output = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--json", "convert", "--format", "tgs"])
        .arg(&input)
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let lines = json_lines(&output);
    let named = |path: &Value| path.as_str().unwrap().ends_with("caf\u{fffd}.png");
    assert_eq!(lines[0]["event"], "started");
    assert!(named(&lines[0]["input"]), "{}", lines[0]);
    let finished = lines.last().unwrap();
    assert!(finished["output"].as_str().unwrap().ends_with("caf\u{fffd}.sticker.tgs"));
    // as is an error about the file
    let missing = dir.path().join(std::ffi::OsStr::from_bytes(b"gone\xe9.png"));
    let output = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--json", "inspect"])
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let lines = json_lines(&output);
    assert!(lines[0]["input"].as_str().unwrap().ends_with("gone\u{fffd}.png"), "{}", lines[0]);
}

/// `tgradish` with the user's presets in `dir`'s `tgradish/presets`.
#[cfg(target_os = "linux")]
fn with_presets(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(args)
        .env("TGRADISH_CONFIG", "/nonexistent/tgradish-config.toml")
        .env("XDG_CONFIG_HOME", dir)
        .output()
        .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn shows_a_preset_as_a_file_could_hold_it() {
    let dir = tempfile::tempdir().unwrap();
    let presets = dir.path().join("tgradish").join("presets");
    std::fs::create_dir_all(&presets).unwrap();
    let preset = "description = 'Mine'\nextends = 'fast'\n\
                  [webm]\nfit = 'crf'\nfit-range = { min = 20, max = 40 }\n\
                  encoder-options = { aq-mode = '2' }\n";
    std::fs::write(presets.join("mine.toml"), preset).unwrap();
    let shown = with_presets(dir.path(), &["preset", "show", "mine"]);
    assert!(shown.status.success(), "{}", String::from_utf8_lossy(&shown.stderr));
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(
        text.contains("description = \"Mine\"") && text.contains("extends = \"fast\""),
        "{text}"
    );
    // pasted into a file, it is the same preset
    std::fs::write(presets.join("copy.toml"), &text).unwrap();
    let json = |name: &str| {
        let shown = with_presets(dir.path(), &["--json", "preset", "show", name]);
        let mut value: Value = serde_json::from_slice(&shown.stdout).unwrap();
        value.as_object_mut().unwrap().remove("description");
        value
    };
    assert_eq!(json("copy"), json("mine"));
    assert_eq!(json("mine")["webm"]["fit-range"]["max"], 40.0);
    assert_eq!(json("mine")["webm"]["speed"], "fast");

    // each preset says which formats it is the default for
    let listed = with_presets(dir.path(), &["preset", "list"]);
    let listed = console::strip_ansi_codes(&String::from_utf8_lossy(&listed.stdout)).into_owned();
    assert!(listed.contains("balanced (default for webm and webp)"), "{listed}");
    assert!(listed.contains("best (default for tgs)"), "{listed}");
}

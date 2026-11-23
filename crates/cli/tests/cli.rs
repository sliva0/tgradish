//! Integration tests running the `tgradish` binary. Conversion tests requiring
//! ffmpeg skip when ffmpeg is absent, or fail when `TGRADISH_REQUIRE_FFMPEG`
//! is set.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tgradish_corpus::{Canvas, Clip};

fn tgradish(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(args)
        // Isolate tests from the developer's user config.
        .env("TGRADISH_CONFIG", no_config())
        .output()
        .unwrap()
}

/// Returns an empty config file path for `TGRADISH_CONFIG` so tests run with
/// default settings instead of the developer's user config.
fn no_config() -> &'static Path {
    static FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("empty-config.toml");
        std::fs::write(&path, "").unwrap();
        path
    })
}

fn json_lines(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Returns a cached synthetic video clip generated with ffmpeg, or `None` if
/// ffmpeg is unavailable to convert it.
fn clip(kind: Clip) -> Option<PathBuf> {
    // Require ffmpeg on PATH even if a cached clip exists from an earlier run.
    if !tgradish_corpus::ffmpeg_on_path() {
        return None;
    }
    tgradish_corpus::clip(kind, Path::new(env!("CARGO_TARGET_TMPDIR")))
}

#[test]
fn lists_presets_as_json() {
    let presets = json_lines(&tgradish(&["--json", "preset", "list"]));
    let preset = |name: &str| presets.iter().find(|p| p["name"] == name).unwrap();
    // Presets configure encoding for both formats without overriding target.
    assert_eq!(preset("fast")["webm"]["effort"], "fast");
    assert_eq!(preset("fast")["webm"]["fit"], "bitrate");
    assert_eq!(preset("fast")["tgs"]["effort"], "fast");
    assert_eq!(preset("fast")["builtin"], true);
    assert!(preset("best")["webm"]["target"].is_null());
    assert!(preset("best")["error"].is_null());
}

#[test]
fn carries_licences() {
    let output = tgradish(&["licenses"]);
    assert!(output.status.success());
    // Both `licenses` and the British `licences` alias are accepted.
    let alias = tgradish(&["licences"]);
    assert!(alias.status.success() && alias.stdout == output.stdout);
    let text = String::from_utf8_lossy(&output.stdout);
    for title in [
        "tgradish",
        "Font: Ubuntu Light and Medium derivative tgradish, subsets",
        "libwebp: licence",
        "Rust crates",
    ] {
        assert!(text.lines().any(|line| line == title), "no {title:?}");
    }
    // Includes per-crate licence notices and copyright attributions.
    assert!(text.matches("used by:").count() > 40);
    // In JSON mode, emits one notice object per line.
    let json = json_lines(&tgradish(&["--json", "licenses"]));
    assert_eq!(json[0]["title"], "tgradish");
    assert!(json.iter().all(|notice| notice["text"].is_string()));
    for notice in &json {
        let title = notice["title"].as_str().unwrap();
        assert!(text.lines().any(|line| line == title), "{title}");
    }
}

/// Completion scripts for each shell, and man pages without the help's colours,
/// one per command.
#[test]
fn writes_completions_and_man_pages() {
    for (shell, marker) in [
        ("bash", "complete -F"),
        ("zsh", "#compdef tgradish"),
        ("fish", "complete -c tgradish"),
        ("powershell", "Register-ArgumentCompleter"),
        ("nushell", "export extern tgradish ["),
    ] {
        let output = tgradish(&["completions", shell]);
        assert!(output.status.success(), "{shell}");
        let script = String::from_utf8(output.stdout).unwrap();
        assert!(script.contains(marker) && script.contains("convert"), "{shell}");
        assert!(!script.contains("__man") && !script.contains(" man "), "{shell}");
    }
    let dir = tempfile::tempdir().unwrap();
    let output = tgradish(&["__man", dir.path().to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for page in ["tgradish.1", "tgradish-convert.1", "tgradish-config-example.1"] {
        let text = std::fs::read_to_string(dir.path().join(page)).unwrap();
        assert!(text.starts_with(".ie") || text.contains(".TH"), "{page}");
        assert!(!text.contains('\x1b'), "{page} has the help's colours");
    }
    assert!(!dir.path().join("tgradish-__man.1").exists());
}

/// Closing stdout early (e.g. `tgradish licenses | head -2`) terminates
/// quietly without a broken-pipe panic message.
#[test]
fn stops_quietly_when_stdout_closes() {
    use std::io::Read;
    use std::process::Stdio;

    for args in [&["licenses"][..], &["--json", "preset", "list"], &["preset", "list"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(args)
            .env("TGRADISH_CONFIG", no_config())
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
    assert_eq!(output.status.code(), Some(2));
    let lines = json_lines(&output);
    assert_eq!(lines[0]["event"], "error");
    let message = lines[0]["message"].as_str().unwrap();
    assert_eq!(message, r#"unknown --preset "nope"; available: balanced, best, fast"#);
}

#[test]
fn rejects_bad_flags() {
    let output = tgradish(&["convert", "a.mp4", "--crf", "64"]);
    assert_eq!(output.status.code(), Some(2));
    // Boolean flags reject trailing `true`/`false` arguments instead of
    // treating them as positional input files.
    let output = tgradish(&["convert", "a.png", "--watermark", "false"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("boolean flags take no value"));
    // `-` is rejected as an input or output path because stdin/stdout streaming
    // is not supported.
    for args in [
        &["convert", "-", "-f", "tgs"][..],
        &["convert", "a.png", "-o", "-"],
        &["spoof", "-"],
        &["spoof", "a.webm", "-o", "-"],
        &["inspect", "a.webm", "-"],
    ] {
        let output = tgradish(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let said = "error: reading from stdin or writing to stdout via '-' is not supported; pass a file path";
        assert!(String::from_utf8_lossy(&output.stderr).starts_with(said), "{args:?}");
    }
    // The GUI does not emit JSON output.
    if cfg!(feature = "gui") {
        let output = tgradish(&["--json", "gui"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("--json is not supported by gui"));
    }
}

/// Without a display server, `tgradish gui` reports the missing display clearly
/// and bare `tgradish` prints CLI usage instead of failing to open a window.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn says_when_there_is_no_display() {
    if !cfg!(feature = "gui") {
        return;
    }
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(args)
            .env("TGRADISH_CONFIG", no_config())
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DISPLAY")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let gui = run(&["gui"]);
    assert_eq!(gui.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&gui.stderr).trim_end(),
        "error: could not start the GUI: no display found (neither WAYLAND_DISPLAY nor DISPLAY is set)"
    );
    let alone = run(&[]);
    assert_eq!(alone.status.code(), Some(2));
    assert!(alone.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&alone.stderr);
    assert!(stderr.contains("tgradish gui") && stderr.contains("Usage:"), "{stderr}");
}

/// Missing input files produce the same `<path>: no such file` message across
/// all commands and formats, naming the path once.
#[test]
fn says_an_input_is_missing_alike_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    for (name, args) in [
        ("nope.mp4", &["convert"][..]),
        ("nope.mp4", &["convert", "-f", "tgs"]),
        ("nope.png", &["convert", "-f", "tgs"]),
        ("nope.mp4", &["convert", "-f", "webp"]),
        ("nope.webm", &["inspect"]),
        ("nope.webm", &["spoof"]),
    ] {
        let missing = dir.path().join(name);
        let missing = missing.to_str().unwrap();
        let mut args = args.to_vec();
        args.push(missing);
        let output = tgradish(&args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let stderr =
            console::strip_ansi_codes(&String::from_utf8_lossy(&output.stderr)).into_owned();
        let said = format!("{missing}: no such file\n");
        assert!(stderr.ends_with(&said), "{args:?}: {stderr}");
        assert_eq!(stderr.matches(missing).count(), 1, "{args:?}: {stderr}");
        args.insert(0, "--json");
        let json = json_lines(&tgradish(&args));
        assert_eq!(json[0]["message"], format!("{missing}: no such file"), "{args:?}");
    }
}

/// Options that do not apply to a given input emit a warning rather than being
/// silently ignored or hard-rejected.
#[test]
fn says_what_goes_unused() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png");
    square_png(&png, [255, 0, 0, 255]);
    let png = png.to_str().unwrap();
    let output = tgradish(&["convert", png, "-f", "tgs", "--sheet-frames", "3"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim_end(),
        "error: --sheet-frames requires --sheet"
    );
    // Single-file `.tgs` inputs use their own frame timing rather than `--fps`.
    let unused = "--fps only sets frame rate for image sequences and sprite sheets in .tgs output";
    let output = tgradish(&["convert", png, "-f", "tgs", "--fps", "30", "-y"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches(unused).count(), 1, "{stderr}");
    let output = tgradish(&["--json", "convert", png, "-f", "tgs", "--fps", "30", "-y"]);
    let lines = json_lines(&output);
    assert!(lines.iter().any(|line| line["event"] == "warning" && line["message"] == unused));
    // Image sequences use `--fps` without warning.
    let output = tgradish(&["convert", png, "--sequence", "-f", "tgs", "--fps", "30", "-y"]);
    assert!(!String::from_utf8_lossy(&output.stderr).contains(unused));
}

/// Passing the wrong file format reports the mismatch and suggests the command
/// to convert it.
#[test]
fn says_a_file_is_of_the_wrong_kind() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png");
    square_png(&png, [255, 0, 0, 255]);
    let png = png.to_str().unwrap();
    let stderr = |args: &[&str]| {
        let output = tgradish(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        String::from_utf8_lossy(&output.stderr).trim_end().to_owned()
    };
    assert_eq!(
        stderr(&["spoof", png]),
        format!("error: {png} is not a WebM file; run `tgradish convert {png}` to create one")
    );
    // A truncated file with a valid EBML header prints its parse error once.
    let broken = dir.path().join("broken.webm");
    std::fs::write(&broken, b"\x1a\x45\xdf\xa3\x00\x00").unwrap();
    let said = stderr(&["spoof", broken.to_str().unwrap()]);
    let why = said.rsplit(": ").next().unwrap();
    assert_eq!(said.matches(why).count(), 1, "{said}");
}

/// Unwritable output directories fail before conversion starts and report the
/// target output path without temporary filenames or OS error numbers.
#[cfg(unix)]
#[test]
fn says_when_the_output_cant_be_written() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png");
    square_png(&png, [255, 0, 0, 255]);
    let locked = dir.path().join("ro");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = locked.join("a.sticker.tgs");
    let output = tgradish(&["convert", png.to_str().unwrap(), "-o", out.to_str().unwrap()]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Root can write to read-only directories.
    if output.status.success() {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!("failed to write {}: permission denied\n", out.display());
    assert!(stderr.ends_with(&expected), "{stderr}");
    assert!(!stderr.contains("Converting"), "{stderr}");
}

/// Invalid CLI flags and option combinations exit with status 2 before reading
/// any input file and name the offending flag once.
#[test]
fn refuses_mistakes_in_the_command_line_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
    square_png(&a, [255, 0, 0, 255]);
    square_png(&b, [0, 255, 0, 255]);
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());
    let out = dir.path().join("out.webm");
    let out = out.to_str().unwrap();
    for (args, said) in [
        (
            &["convert", a, b, "-o", out][..],
            "-o/--output only accepts a single input or --sequence",
        ),
        (&["convert", a, "-o", "out.gif"], "out.gif must end in .webm"),
        (&["convert", a, "--crf", "30", "-f", "tgs"], "--crf cannot be used with tgs output"),
        (&["convert", a, "--keep-temp", "-f", "webp"], "--keep-temp only applies to WebM output"),
        (&["convert", a, "--preset", "nope"], r#"unknown --preset "nope"; available: "#),
        (&["convert", a, "--options-json", "{"], "--options-json must be a JSON object"),
        (&["convert", a, "--options-json", r#"{"tempo":50}"#], r#""tempo" in --options-json must"#),
        (&["convert", a, b, "--tempo", "50"], "must be from 0.1 to 10"),
        (&["convert", a, b, "--fps", "0"], "must be greater than 0 and at most 60"),
        (&["convert", a, b, "-t", "0"], "must be greater than 0 seconds"),
        (&["convert", a, b, "-f", "tgs", "-s=-1"], "must be 0 seconds or greater"),
        (&["convert", a, "--attempts", "0"], "--attempts"),
        (&["convert", a, "--threads", "0"], r#"0 threads: expected at least 1, or "auto""#),
        (
            &["watch", a, "--threads", "all"],
            r#""all": expected "auto" or an integer of at least 1"#,
        ),
    ] {
        let output = tgradish(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(said), "{args:?}: {stderr}");
        assert_eq!(stderr.matches("error:").count(), 1, "{args:?}: {stderr}");
        assert!(!stderr.contains("Converting"), "{args:?}: {stderr}");
    }
    assert!(!Path::new(out).exists());
}

/// `--threads` overrides the `threads` setting from the config file.
#[test]
fn converts_in_as_many_threads_as_asked() {
    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "threads = 2\n").unwrap();
    let threads = |flags: &[&str]| {
        let out = dir.path().join("out.webm");
        let mut args = vec!["--json", "--config", config.to_str().unwrap(), "convert"];
        args.extend([input.to_str().unwrap(), "-o", out.to_str().unwrap(), "-y"]);
        args.extend(["--preset", "fast", "--fit", "off", "--length", "0.5"]);
        args.extend(flags);
        let output = tgradish(&args);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        json_lines(&output)[0]["plan"]["threads"].clone()
    };
    assert_eq!(threads(&[]), 2);
    assert_eq!(threads(&["--threads", "1"]), 1);
    assert_eq!(threads(&["--threads", "auto"]), "auto");
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
        // Legacy `speed` alias for `effort`.
        r#"{"length": 1.5, "title": "from json", "speed": "balanced"}"#,
        "--title",
        "from flag",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));

    let events = json_lines(&output);
    assert_eq!(events.first().unwrap()["event"], "started");
    let plan = &events[0]["plan"];
    assert_eq!(plan["length"], 1.5);
    assert_eq!(plan["title"], "from flag", "flags win over --options-json");
    assert_eq!(plan["effort"], "balanced");
    let finished = events.last().unwrap();
    assert_eq!(finished["event"], "finished");
    // Every format's finished event includes the input path.
    assert!(finished["input"].is_string(), "{finished}");
    assert_eq!(finished["issues"], Value::Array(vec![]));

    let inspected = tgradish(&["--json", "inspect", out.to_str().unwrap()]);
    let info = &json_lines(&inspected)[0];
    assert_eq!(info["info"]["title"], "from flag");
    assert_eq!(info["issues"], Value::Array(vec![]));
    // Reports video track properties without exposing its track UID.
    let video = &info["info"]["video"];
    assert_eq!(video["codec-id"], "V_VP9", "{info}");
    assert!(video.get("uid").is_none(), "{info}");
}

/// Closing stdout mid-conversion cancels cleanly (removing temporary files and
/// stopping ffmpeg) before exiting with status 141 (SIGPIPE).
#[cfg(target_os = "linux")]
#[test]
fn stops_cleanly_when_stdout_closes() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    // Dedicated temporary directory to verify cleanup after exit.
    let temp = dir.path().join("tmp");
    std::fs::create_dir(&temp).unwrap();
    let out = dir.path().join("out.webm");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--ffmpeg-from", "system", "--json", "convert", input.to_str().unwrap(), "-o"])
        .arg(&out)
        .env("TGRADISH_CONFIG", no_config())
        .env("TMPDIR", &temp)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Read until ffmpeg starts encoding, then close stdout as `head` would.
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let encoding = lines.by_ref().map_while(Result::ok).any(|line| line.contains("progress"));
    assert!(encoding, "never encoded");
    drop(lines);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(141));
    let left: Vec<_> = std::fs::read_dir(&temp).unwrap().map(|e| e.unwrap().path()).collect();
    assert!(left.is_empty(), "left behind: {left:?}");
    // ffmpeg's command line includes the pass-log path inside `temp`.
    let temp = temp.to_str().unwrap();
    for process in std::fs::read_dir("/proc").unwrap().filter_map(Result::ok) {
        let Ok(command) = std::fs::read(process.path().join("cmdline")) else { continue };
        let command = String::from_utf8_lossy(&command);
        assert!(!command.contains(temp), "still running: {command}");
    }
    assert!(!out.exists());
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
        .env("TGRADISH_CONFIG", no_config())
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
            // Ignore write errors if the child has already exited.
            let _ = writeln!(stdin, "cancel");
        }
    }
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));
    assert_eq!(last["event"], "error");
    assert!(!out.exists());
}

/// Starts `convert` on `inputs` with bitrate fitting far below the limit so
/// the search stays active, emitting JSON events with piped stdio. On Unix,
/// runs in its own process group (shared with child ffmpeg processes) so
/// group signals simulate terminal Ctrl-C.
fn fitting(inputs: &[&Path], dir: &Path, args: &[&str]) -> std::process::Child {
    fitting_command(inputs, dir, args).spawn().unwrap()
}

/// Builds the `Command` launched by [`fitting`].
fn fitting_command(inputs: &[&Path], dir: &Path, args: &[&str]) -> Command {
    use std::process::Stdio;
    let mut command = Command::new(env!("CARGO_BIN_EXE_tgradish"));
    command
        .args(["--json", "convert", "-y", "-t", "2", "--fit", "bitrate", "-b", "50"])
        .args(["--effort", "fast", "-O"])
        .arg(dir)
        .args(inputs)
        .args(args)
        .env("TGRADISH_CONFIG", no_config())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    in_a_group_of_its_own(&mut command);
    command
}

/// Places `command` in its own Unix process group and registers this test
/// process as a subreaper on Linux (see [`keep_orphans`]).
fn in_a_group_of_its_own(command: &mut Command) {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(target_os = "linux")]
    keep_orphans();
    #[cfg(not(unix))]
    let _ = command;
}

/// Marks this test process as a child subreaper so any orphaned ffmpeg process
/// that `tgradish` fails to reap remains as a visible zombie in its process
/// group for [`left_in_group`] to catch (instead of being reparented to and
/// reaped by PID 1).
#[cfg(target_os = "linux")]
fn keep_orphans() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        rustix::process::set_child_subreaper(Some(rustix::process::getpid())).unwrap();
    });
}

/// Sends `signal` to `child`'s entire process group, simulating a terminal
/// signal such as Ctrl-C.
#[cfg(target_os = "linux")]
fn signal_group(child: &std::process::Child, signal: &str) {
    let group = format!("-{}", child.id());
    let sent = Command::new("kill").args([&format!("-{signal}"), "--", &group]).status();
    assert!(sent.unwrap().success());
}

/// Polls `/proc` for up to 5 seconds and returns any remaining processes or
/// unreaped zombies in `child`'s process group as `"pid state (name) command"`.
#[cfg(target_os = "linux")]
fn left_in_group(child: &std::process::Child) -> Vec<String> {
    let group = child.id().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let mut left = Vec::new();
        for process in std::fs::read_dir("/proc").unwrap().filter_map(Result::ok) {
            let Ok(stat) = std::fs::read_to_string(process.path().join("stat")) else { continue };
            // Fields after `(comm) `: state, ppid, pgrp.
            let Some((_, rest)) = stat.rsplit_once(") ") else { continue };
            let fields: Vec<&str> = rest.split(' ').collect();
            if fields.get(2) == Some(&group.as_str()) {
                // Zombies have an empty `cmdline`, leaving only the comm name
                // in parentheses.
                let command = std::fs::read(process.path().join("cmdline")).unwrap_or_default();
                let name = stat.split_once(" (").map_or("", |(_, rest)| rest);
                let name = name.rsplit_once(") ").map_or("", |(name, _)| name);
                let pid = process.file_name().to_string_lossy().into_owned();
                let state = fields.first().unwrap_or(&"?");
                let command = String::from_utf8_lossy(&command);
                left.push(format!("{pid} {state} ({name}) {command}"));
            }
        }
        if left.is_empty() || std::time::Instant::now() > deadline {
            return left;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn keeps_the_best_so_far_when_asked_on_stdin() {
    use std::io::{BufRead, BufReader, Write};

    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut child = fitting(&[&input], dir.path(), &["--cancel-on-stdin"]);
    let mut stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut events = Vec::new();
    for line in stdout.lines() {
        let event: Value = serde_json::from_str(&line.unwrap()).unwrap();
        if event["event"] == "progress" && event["can-settle"] == true {
            let _ = writeln!(stdin, "settle");
        }
        events.push(event);
    }
    assert!(child.wait().unwrap().success());
    let finished = events.last().unwrap();
    assert_eq!((&finished["event"], &finished["settled"]), (&"finished".into(), &true.into()));
    let kept = &finished["attempt"];
    assert!(
        events
            .iter()
            .any(|e| e["decision"]["reason"] == "settled" && e["decision"]["attempt"] == *kept)
    );
}

/// Pressing Ctrl-C once an encode fits keeps the best attempt so far and
/// advances to the next input; pressing Ctrl-C before any attempt fits cancels
/// immediately. Sending SIGINT to the process group reaches ffmpeg too and
/// leaves no stray processes.
#[cfg(target_os = "linux")]
#[test]
fn keeps_the_best_so_far_on_ctrl_c() {
    use std::io::{BufRead, BufReader};

    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first.mp4"), dir.path().join("second.mp4"));
    std::fs::copy(&input, &first).unwrap();
    std::fs::copy(&input, &second).unwrap();
    let out = dir.path().join("out");
    let mut child = fitting(&[&first, &second], &out, &[]);
    let interrupt = |child: &std::process::Child| signal_group(child, "INT");
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let (mut settled, mut cancelled, mut finished) = (false, false, Vec::new());
    for line in stdout.lines() {
        let event: Value = serde_json::from_str(&line.unwrap()).unwrap();
        match event["event"].as_str() {
            Some("progress") if event["can-settle"] == true && !settled => {
                interrupt(&child);
                settled = true;
            }
            // Second input before any attempt fits.
            Some("progress") if finished.len() == 1 && !cancelled => {
                interrupt(&child);
                cancelled = true;
            }
            Some("finished") => finished.push(event),
            _ => {}
        }
    }
    let status = child.wait().unwrap();
    assert_eq!(finished.len(), 1, "{finished:?}");
    assert_eq!(finished[0]["settled"], true);
    assert!(out.join("first.sticker.webm").exists());
    assert!(!out.join("second.sticker.webm").exists());
    assert_eq!(status.code(), Some(130));
    assert_eq!(left_in_group(&child), Vec::<String>::new());
}

/// Terminal Ctrl-C reaches both `tgradish` and ffmpeg: once an encode fits, the
/// interrupted in-flight encode is abandoned and the best attempt so far is
/// kept without leaving stray processes.
#[cfg(target_os = "linux")]
#[test]
fn keeps_the_best_so_far_as_ctrl_c_reaches_ffmpeg_too() {
    use std::io::{BufRead, BufReader};

    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut child = fitting(&[&input], dir.path(), &[]);
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let (mut sent, mut last) = (false, Value::Null);
    for line in stdout.lines() {
        last = serde_json::from_str(&line.unwrap()).unwrap();
        // Wait until the next attempt is actively encoding in ffmpeg.
        let encoding = last["fraction"].as_f64().is_some_and(|done| done > 0.0 && done < 1.0);
        if last["event"] == "progress" && last["can-settle"] == true && encoding && !sent {
            signal_group(&child, "INT");
            sent = true;
        }
    }
    let status = child.wait().unwrap();
    assert!(sent);
    assert_eq!((&last["event"], &last["settled"]), (&"finished".into(), &true.into()), "{last}");
    assert_eq!(status.code(), Some(0));
    assert_eq!(left_in_group(&child), Vec::<String>::new());
}

/// Sending SIGTERM or SIGHUP to `tgradish` alone while ffmpeg is fitting a WebM
/// cancels the conversion, stops and reaps ffmpeg, removes temporary files and
/// partial outputs, and exits by that signal.
#[cfg(target_os = "linux")]
#[test]
fn stops_cleanly_when_killed() {
    let Some(input) = clip(Clip::Square) else { return };
    for signal in ["TERM", "HUP"] {
        terminated_while_fitting(&input, signal, false);
    }
}

/// Sending SIGTERM or SIGHUP to the entire process group (as on terminal close
/// or service stop) stops both ffmpeg and `tgradish`, cleans up all temporary
/// files and outputs, and exits by that signal.
#[cfg(target_os = "linux")]
#[test]
fn stops_cleanly_when_the_terminal_ends_it() {
    let Some(input) = clip(Clip::Square) else { return };
    for signal in ["TERM", "HUP"] {
        terminated_while_fitting(&input, signal, true);
    }
}

/// Sends `signal` to a running conversion of `input` once ffmpeg is actively
/// encoding (either to the whole process group when `group` is true, or to
/// `tgradish` alone) and asserts that it exits by that signal with no leftover
/// files or child processes.
#[cfg(target_os = "linux")]
fn terminated_while_fitting(input: &Path, signal: &str, group: bool) {
    use std::io::{BufRead, BufReader};

    let dir = tempfile::tempdir().unwrap();
    let (temp, out) = (dir.path().join("tmp"), dir.path().join("out"));
    std::fs::create_dir(&temp).unwrap();
    let mut child = fitting_command(&[input], &out, &[]).env("TMPDIR", &temp).spawn().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok);
    let encoding = lines.by_ref().any(|line| {
        let event: Value = serde_json::from_str(&line).unwrap();
        let done = event["fraction"].as_f64();
        event["event"] == "progress" && done.is_some_and(|done| done > 0.0 && done < 1.0)
    });
    assert!(encoding, "{signal}: never encoded");
    match group {
        true => signal_group(&child, signal),
        false => signal_one(&child, signal),
    }
    // Drain stdout until EOF as a caller would.
    lines.for_each(drop);
    assert_terminated(child, signal, &temp, &out);
}

/// Asserts that `child` exited due to `signal` and left no files in `temp`
/// (`TMPDIR`) or `out` and no processes in its process group.
#[cfg(target_os = "linux")]
fn assert_terminated(mut child: std::process::Child, signal: &str, temp: &Path, out: &Path) {
    use std::os::unix::process::ExitStatusExt;

    let status = child.wait().unwrap();
    let number = match signal {
        "TERM" => 15,
        "HUP" => 1,
        _ => unreachable!("{signal}"),
    };
    assert_eq!(status.signal(), Some(number), "{signal}: {status}");
    let left = |dir: &Path| -> Vec<_> {
        let files = std::fs::read_dir(dir).into_iter().flatten();
        files.map(|entry| entry.unwrap().path()).collect()
    };
    assert_eq!(left(temp), Vec::<PathBuf>::new(), "{signal}: temporary files left");
    assert_eq!(left(out), Vec::<PathBuf>::new(), "{signal}: results left");
    assert_eq!(left_in_group(&child), Vec::<String>::new(), "{signal}: left in its group");
}

/// Sends `signal` to `child` alone (`kill -SIGNAL PID`).
#[cfg(target_os = "linux")]
fn signal_one(child: &std::process::Child, signal: &str) {
    let sent = Command::new("kill").args([&format!("-{signal}"), &child.id().to_string()]).status();
    assert!(sent.unwrap().success());
}

/// Sending SIGTERM to `watch` cancels any in-flight conversion, cleans up
/// temporary files and partial outputs, and exits by SIGTERM (or exits
/// immediately when idle).
#[cfg(target_os = "linux")]
#[test]
fn watch_stops_cleanly_when_killed() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let Some(input) = clip(Clip::Square) else { return };
    for converting in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (watched, temp, out) =
            (dir.path().join("in"), dir.path().join("tmp"), dir.path().join("out"));
        std::fs::create_dir(&watched).unwrap();
        std::fs::create_dir(&temp).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_tgradish"));
        command
            .args(["--json", "watch", watched.to_str().unwrap(), "--interval", "0.1"])
            .args(["-t", "2", "--fit", "bitrate", "-b", "50", "--effort", "fast", "-O"])
            .arg(&out)
            .env("TGRADISH_CONFIG", no_config())
            .env("TMPDIR", &temp)
            .stdout(Stdio::piped());
        in_a_group_of_its_own(&mut command);
        let mut child = command.spawn().unwrap();
        // Files already present when watching starts are ignored, so let the
        // initial scan finish.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok);
        if converting {
            std::fs::copy(&input, watched.join("clip.mp4")).unwrap();
            let encoding = lines.by_ref().any(|line| {
                let event: Value = serde_json::from_str(&line).unwrap();
                let done = event["fraction"].as_f64();
                event["event"] == "progress" && done.is_some_and(|done| done > 0.0 && done < 1.0)
            });
            assert!(encoding, "never encoded");
        }
        signal_one(&child, "TERM");
        lines.for_each(drop);
        assert_terminated(child, "TERM", &temp, &out);
    }
}

/// Without `--cancel-on-stdin`, stdin is left untouched so a shell loop reading
/// filenames from stdin can convert every entry.
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
        .env("TGRADISH_CONFIG", no_config())
        .output()
        .unwrap();
    let finished = json_lines(&output).iter().filter(|e| e["event"] == "finished").count();
    assert_eq!(finished, 3, "{}", String::from_utf8_lossy(&output.stderr));
}

/// Writes a 4×4 PNG containing a 2×2 square of `colour` on a transparent
/// background.
fn square_png(path: &Path, colour: [u8; 4]) {
    let mut canvas = Canvas::new(4, 4);
    canvas.rect(1, 1, 2, 2, colour);
    std::fs::write(path, tgradish_corpus::encode::png(&canvas)).unwrap();
}

/// Writes `frames` 24×24 4-level RGB noise PNGs into a `noise` subdirectory
/// (far exceeding 64 KiB when encoded as an exact `.tgs`).
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
        args.extend(["-o", tgs.to_str().unwrap(), "-y", "--effort", "fast", "--fps", "24"]);
        args.extend(extra);
        tgradish(&args)
    };
    // Exact mode writes the oversize `.tgs` without lossy fitting and exits 3.
    let output = made(&[]);
    assert_eq!(output.status.code(), Some(3), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(tgs.is_file());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("(too large); pass --no-exact to allow lossy fitting"), "{stderr}");
    assert!(!stderr.contains("Fitting"), "nothing is fitted: {stderr}");
    let output = made(&["--json"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(json_lines(&output).last().unwrap()["event"], "finished");
    // `inspect` also exits 3 for the non-compliant file.
    assert_eq!(tgradish(&["inspect", tgs.to_str().unwrap()]).status.code(), Some(3));
    assert_eq!(tgradish(&["--json", "inspect", tgs.to_str().unwrap()]).status.code(), Some(3));
    // A hard failure (exit 1) beats a Telegram refusal (exit 3).
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

    // A WebP is judged as the target it was made for unless --target says
    // otherwise.
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
    let inspected = tgradish(&["--json", "inspect", webp.to_str().unwrap()]);
    assert_eq!(json_lines(&inspected)[0]["format"], "webp");
    let as_emoji = tgradish(&["inspect", webp.to_str().unwrap(), "--target", "emoji"]);
    assert_eq!(as_emoji.status.code(), Some(3));
}

/// A WebM encoded with `--fit off` that exceeds the emoji byte limit exits 3.
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
        "--effort",
        "fast",
    ]);
    assert_eq!(output.status.code(), Some(3), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Telegram will reject it"));
    assert_eq!(tgradish(&["inspect", out.to_str().unwrap()]).status.code(), Some(3));
}

/// Every format writes one result line per output to stdout (reduced to just
/// the output path under `--quiet`), while progress and diagnostics go to
/// stderr (or are suppressed under `--quiet`).
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
            if quiet {
                // Bare path on stdout for scripting.
                assert_eq!(stdout, format!("{}\n", dir.path().join(&line).display()));
            } else {
                assert!(
                    stdout.trim().starts_with("Finished") && stdout.contains(&line),
                    "{stdout}"
                );
            }
            assert_eq!(stdout.lines().count(), 1, "{format}: {stdout}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!quiet || stderr.is_empty(), "{format}: {stderr}");
            // Every format begins with `Converting <in> → <out>` on stderr.
            let start = stderr.lines().next().unwrap_or_default();
            let said = start.contains("Converting")
                && start.contains(&format!("→ {}", dir.path().join(&line).display()));
            assert!(quiet || said, "{format}: {stderr}");
        }
    }
}

/// `inspect` also inspects source media files, reporting their properties, that
/// Telegram will not accept them as-is, and which limits their dimensions or
/// duration would violate if kept unchanged.
#[test]
fn inspects_what_stickers_are_made_of() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("dot.png");
    square_png(&png, [255, 0, 0, 255]);
    let png = png.to_str().unwrap();
    let output = tgradish(&["inspect", png]);
    assert_eq!(output.status.code(), Some(3), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("picture (not a sticker yet)"), "{stdout}");
    assert!(stdout.contains(&format!("run `tgradish convert {png}` to convert it")), "{stdout}");
    assert!(stdout.contains("expected one side to be 512 px"), "{stdout}");
    let line = &json_lines(&tgradish(&["--json", "inspect", png]))[0];
    assert_eq!((&line["format"], &line["accepted"]), (&json!("input"), &json!(false)));
    assert_eq!((&line["info"]["kind"], &line["info"]["frames"]), (&json!("picture"), &json!(1)));
    assert_eq!(line["issues"][0]["kind"], "wrong-size");
    // Inspected against emoji target rules.
    let line = &json_lines(&tgradish(&["--json", "inspect", png, "--target", "emoji"]))[0];
    assert_eq!(line["target"], "emoji");
    // Video source probed via ffprobe includes duration and frame rate.
    if tgradish_corpus::ffmpeg_on_path() {
        let video = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=512x288:d=4:r=30"])
            .args(["-pix_fmt", "yuv420p"])
            .arg(&video)
            .status()
            .unwrap();
        assert!(made.success());
        let video = video.to_str().unwrap();
        let inspected = tgradish(&["--ffmpeg-from", "system", "--json", "inspect", video]);
        assert_eq!(inspected.status.code(), Some(3));
        let info = &json_lines(&inspected)[0]["info"];
        assert_eq!(
            (&info["kind"], &info["width"], &info["fps"]),
            (&json!("video"), &json!(512), &json!(30.0))
        );
        assert_eq!((&info["duration"], &info["frames"]), (&json!(4.0), &json!(120)));
        let issues = &json_lines(&inspected)[0]["issues"];
        assert_eq!((issues.as_array().unwrap().len(), &issues[0]["kind"]), (1, &json!("too-long")));
    }
    // A converted sticker reports `accepted: true`.
    let tgs = dir.path().join("dot.sticker.tgs");
    let made = tgradish(&["convert", png, "-f", "tgs", "-o", tgs.to_str().unwrap()]);
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let line = &json_lines(&tgradish(&["--json", "inspect", tgs.to_str().unwrap()]))[0];
    assert_eq!((&line["format"], &line["accepted"]), (&json!("tgs"), &json!(true)));
}

/// `-o` validates that the output path is not a directory and has an extension
/// matching the target format before doing any conversion work.
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
        // CLI usage error (exit 2).
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(why), "{args:?}: {stderr}");
        assert!(!stderr.contains("Converting"), "{args:?}: {stderr}");
    };
    let outdir = outdir.to_str().unwrap();
    for overwrite in [&[][..], &["-y"]] {
        let mut args = vec!["convert", png, "--format", "tgs", "-o", outdir];
        args.extend(overwrite);
        refused(&args, "is a directory; use -o for a single output file or -O/--output-dir");
    }
    let mp4 = dir.path().join("x.mp4");
    refused(&["convert", png, "-o", mp4.to_str().unwrap()], "must end in .webm");
    let tgs = dir.path().join("x.tgs");
    refused(
        &["convert", png, "--format", "webm", "-o", tgs.to_str().unwrap()],
        "has a .tgs extension, which does not match webm output",
    );
    assert!(!mp4.exists() && !tgs.exists());
    // Case-insensitive extension inference still selects the format.
    let upper = dir.path().join("x.TGS");
    let made = tgradish(&["convert", png, "-o", upper.to_str().unwrap()]);
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    refused(&["spoof", upper.to_str().unwrap(), "-o", outdir], "is a directory");
}

/// `--resize contain` preserves the input aspect ratio, which for 512×512
/// `.tgs` output requires a square input or crop.
#[test]
fn keeps_only_a_square_tgs_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("dot.png");
    square_png(&png, [255, 0, 0, 255]);
    let png = png.to_str().unwrap();
    let out = dir.path().join("dot.tgs");
    let out = out.to_str().unwrap();
    let convert = |crop: &str| {
        tgradish(&["convert", png, "-o", out, "-y", "--resize", "contain", "--crop", crop])
    };
    let made = convert("4x4+0+0");
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let refused = convert("4x3+0+0");
    assert_eq!(refused.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("resize contain keeps the 4×3 aspect ratio"), "{stderr}");
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
    // 2×2 visible pixels, two frames of 15 ticks each, sorted naturally
    // (`2.png` before `10.png`).
    assert_eq!(lines[0]["report"]["frames"], 2);
    assert_eq!(lines[0]["report"]["ticks"], 30);
    let finished = lines.last().unwrap();
    assert_eq!(
        (finished["event"].as_str(), finished["changed"].as_bool()),
        (Some("finished"), Some(false))
    );
    assert_eq!(finished["issues"], serde_json::json!([]));
    assert_eq!(finished["input"], lines[0]["input"]);

    let inspected = tgradish(&["--json", "inspect", out.to_str().unwrap()]);
    let lines = json_lines(&inspected);
    assert_eq!(lines[0]["format"], "tgs");
    assert_eq!(lines[0]["info"]["frames"], 30.0);
    assert_eq!(lines[0]["issues"], serde_json::json!([]));

    // WebM-only flags are rejected for `.tgs` output.
    let mixed = tgradish(&["convert", a.to_str().unwrap(), "-o", "x.tgs", "--crf", "20"]);
    assert_eq!(mixed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&mixed.stderr).contains("--crf"));
}

#[test]
fn makes_video_and_static_stickers_of_sequences() {
    if !tgradish_corpus::ffmpeg_on_path() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let frames = dir.path().join("walk");
    std::fs::create_dir(&frames).unwrap();
    square_png(&frames.join("2.png"), [255, 0, 0, 255]);
    square_png(&frames.join("10.png"), [0, 0, 255, 255]);
    // Two frames at 4 fps produce a 0.5 s WebM.
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
    // Static WebP of the first frame (`2.png`).
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
    // Single-input failures emit the same per-input error event structure.
    let output = tgradish(&["--json", "inspect", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let lines = json_lines(&output);
    assert_eq!(lines.len(), 1);
    assert!(failed(&lines[0]) && lines[0]["input"] == missing.to_str().unwrap());
    let output = tgradish(&["--json", "convert", missing.to_str().unwrap(), "--format", "tgs"]);
    let lines = json_lines(&output);
    assert_eq!((output.status.code(), lines.len()), (Some(1), 1));
    assert!(failed(&lines[0]) && lines[0]["input"] == missing.to_str().unwrap());
    // Image sequences identify errors by their first input path, as `spoof`
    // does.
    let (missing, other) = (missing.to_str().unwrap(), other.to_str().unwrap());
    for args in [
        &["--json", "convert", "--sequence", missing, other, "--format", "tgs"][..],
        &["--json", "spoof", missing],
    ] {
        let output = tgradish(args);
        let lines = json_lines(&output);
        assert_eq!((output.status.code(), lines.len()), (Some(1), 1), "{args:?}");
        assert!(failed(&lines[0]) && lines[0]["input"] == missing, "{args:?}: {}", lines[0]);
    }
}

/// `ffmpeg status` against an ffmpeg build without `libvpx-vp9` emits a single
/// JSON status document (and exits 1).
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
    assert_eq!(status["from"], "flag");
    // Plain-text output aligns columns, shows the source, and reports missing
    // libvpx.
    let text = tgradish(&["--ffmpeg", dir.path().to_str().unwrap(), "ffmpeg"]);
    assert_eq!(text.status.code(), Some(1));
    let ffmpeg = dir.path().join("ffmpeg");
    let expected = format!(
        "ffmpeg    {} (from --ffmpeg)\nffprobe   {}\nversion   fake\nlibvpx    no, does not support \
         video stickers\n",
        ffmpeg.display(),
        dir.path().join("ffprobe").display()
    );
    assert_eq!(String::from_utf8_lossy(&text.stdout), expected);
}

/// Verifies ffmpeg discovery precedence: `--ffmpeg` / `--ffmpeg-from`
/// (mutually exclusive) override `TGRADISH_FFMPEG`, which overrides the config
/// file.
#[cfg(unix)]
#[test]
fn ranks_where_ffmpeg_comes_from() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    for name in ["ffmpeg", "ffprobe"] {
        let path = dir.path().join(name);
        std::fs::write(&path, "#!/bin/sh\necho 'ffmpeg version fake'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let config = dir.path().join("config.toml");
    std::fs::write(&config, format!("[ffmpeg]\npath = {:?}\n", dir.path().join("nowhere")))
        .unwrap();
    let status = |env: Option<&Path>, args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tgradish"));
        command.args(["--json", "--config", config.to_str().unwrap()]).args(args);
        command.args(["ffmpeg", "status"]);
        if let Some(env) = env {
            command.env("TGRADISH_FFMPEG", env);
        }
        let output = command.output().unwrap();
        let line = json_lines(&output).pop().unwrap();
        (output.status.code(), line)
    };
    let used = |line: &Value| line["ffmpeg"]["ffmpeg"].as_str().unwrap_or("").to_owned();
    let fake = dir.path().join("ffmpeg");
    let fake = fake.to_str().unwrap();
    // Environment variable overrides the config file.
    let (_, line) = status(Some(dir.path()), &[]);
    assert_eq!(used(&line), fake, "{line}");
    assert_eq!(line["from"], "environment");
    // Missing path from config file reports the config path and key.
    let (code, line) = status(None, &[]);
    assert_eq!((code, line["event"].as_str()), (Some(1), Some("error")), "{line}");
    let nowhere = dir.path().join("nowhere");
    let said = format!(
        "ffmpeg not found: {} (from {}'s [ffmpeg] path) does not exist; point it to ffmpeg or its \
         directory",
        nowhere.display(),
        config.display()
    );
    assert_eq!(line["message"], said);
    let (_, line) = status(Some(&nowhere), &[]);
    let said = format!(
        "ffmpeg not found: {} (from TGRADISH_FFMPEG) does not exist; unset it or point it to \
         ffmpeg or its directory",
        nowhere.display()
    );
    assert_eq!(line["message"], said);
    let (_, line) = status(None, &["--ffmpeg", nowhere.to_str().unwrap()]);
    assert!(line["message"].as_str().unwrap().contains("(from --ffmpeg) does not exist"), "{line}");
    if tgradish_corpus::ffmpeg_on_path() {
        // CLI flag overrides the environment variable.
        let (code, line) = status(Some(dir.path()), &["--ffmpeg-from", "system"]);
        assert_eq!(code, Some(0), "{line}");
        assert_ne!(used(&line), fake, "{line}");
    }
    let (_, line) =
        status(Some(Path::new("/nowhere")), &["--ffmpeg", dir.path().to_str().unwrap()]);
    assert_eq!(used(&line), fake, "{line}");
    let both = tgradish(&["--ffmpeg", "/x", "--ffmpeg-from", "system", "ffmpeg", "status"]);
    assert_eq!(both.status.code(), Some(2));
    // Bare `tgradish ffmpeg` defaults to `ffmpeg status`.
    let alone = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--json", "--ffmpeg", dir.path().to_str().unwrap(), "ffmpeg"])
        .output()
        .unwrap();
    assert_eq!(json_lines(&alone)[0]["capabilities"]["version"], "ffmpeg version fake");
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
        .env("TGRADISH_CONFIG", no_config())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Wait for the initial baseline scan so pre-existing files are ignored.
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
    // Relative subdirectory structure is preserved under `--output-dir`.
    assert!(out.join("sub/new.sticker.webm").is_file());
    assert!(!out.join("old.sticker.webm").exists(), "files from before are left alone");
    assert_eq!(status.code(), Some(0));
}

/// Running `tgradish watch` child process with a channel of parsed JSON events.
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
            .env("TGRADISH_CONFIG", no_config())
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
        // Wait for the initial baseline scan so pre-existing files are ignored.
        std::thread::sleep(std::time::Duration::from_millis(500));
        Watching { child, events }
    }

    /// Collects events up to and including the first one matching `until`
    /// (failing after 30 seconds).
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

    /// Collects all events arriving within the next `seconds`.
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

    /// Stops the watch process with SIGINT (Ctrl-C) and waits for exit.
    fn stop(self) -> Output {
        Command::new("kill").args(["-INT", &self.child.id().to_string()]).status().unwrap();
        self.child.wait_with_output().unwrap()
    }
}

#[cfg(unix)]
fn named(event: &Value, kind: &str, name: &str) -> bool {
    let named = |key: &str| event[key].as_str().is_some_and(|path| path.ends_with(name));
    event["event"] == kind && (named("input") || named("output"))
}

/// `watch` converts newly created files and symlinks to files, while ignoring
/// its own output files, hidden dotfiles, and partial download files.
#[cfg(unix)]
#[test]
fn watch_converts_what_appears_and_only_that() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    // WebP outputs are image files that would look like inputs if not filtered
    // by suffix.
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
    // Wait across subsequent scans to verify that outputs are not re-ingested.
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

/// Modifying an input file while its conversion is still running triggers a
/// second conversion with the updated contents.
#[cfg(unix)]
#[test]
fn watch_converts_a_file_again_that_changed_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let watching = Watching::start(dir.path(), &["--format", "tgs"]);
    // 24×24 noise takes around a second to encode as `.tgs`.
    let slow = noise(elsewhere.path(), 1).join("0.png");
    std::fs::copy(&slow, dir.path().join("c.png")).unwrap();
    watching.until(|e| named(e, "started", "c.png"));
    square_png(&dir.path().join("c.png"), [0, 255, 0, 255]);
    let events = watching.until(|e| e["event"] == "started");
    let again = events.last().unwrap();
    assert!(named(again, "started", "c.png"), "{events:?}");
    // Second run sees the 1-colour square rather than the multi-colour noise.
    assert_eq!(again["report"]["colours"], 1, "{again}");
    watching.until(|e| named(e, "finished", "c.sticker.tgs"));
    assert_eq!(watching.stop().status.code(), Some(0));
}

/// Once an input file is deleted, another input with the same stem can claim
/// its output path; while both inputs exist simultaneously, the second is
/// rejected.
#[cfg(unix)]
#[test]
fn watch_gives_a_result_whose_input_is_gone_to_another() {
    let dir = tempfile::tempdir().unwrap();
    let watching = Watching::start(dir.path(), &["--format", "tgs", "-y"]);
    let files = tgradish_corpus::files();
    let walker = &files.iter().find(|file| file.name == "walker_2x.gif").unwrap().bytes;
    square_png(&dir.path().join("cat.png"), [255, 0, 0, 255]);
    watching.until(|e| named(e, "finished", "cat.sticker.tgs"));
    std::fs::remove_file(dir.path().join("cat.png")).unwrap();
    // Wait for a scan to observe that `cat.png` is gone.
    watching.during(0.5);
    std::fs::write(dir.path().join("cat.gif"), walker).unwrap();
    let events = watching.until(|e| matches!(e["event"].as_str(), Some("finished" | "error")));
    let last = events.last().unwrap();
    assert!(named(last, "finished", "cat.sticker.tgs") && named(last, "finished", "cat.gif"));
    // When both `dog.gif` and `dog.png` exist, the second to convert is
    // rejected.
    std::fs::write(dir.path().join("dog.gif"), walker).unwrap();
    square_png(&dir.path().join("dog.png"), [0, 0, 255, 255]);
    let events = watching.until(|e| e["event"] == "error");
    let error = events.last().unwrap();
    assert!(error.to_string().contains("already written from"), "{error}");
    // Removing the first input lets the remaining unmodified input claim the
    // output path.
    let refused = error["input"].as_str().unwrap();
    let (made, refused) =
        if refused.ends_with("dog.png") { ("dog.gif", "dog.png") } else { ("dog.png", "dog.gif") };
    std::fs::remove_file(dir.path().join(made)).unwrap();
    let events = watching.until(|e| matches!(e["event"].as_str(), Some("finished" | "error")));
    let last = events.last().unwrap();
    assert!(named(last, "finished", refused), "{last}");
    assert_eq!(watching.stop().status.code(), Some(0));
}

/// Atomically replacing an input symlink with a symlink to a directory treats
/// the old file input as gone even though the path never disappeared between
/// scans.
#[cfg(unix)]
#[test]
fn watch_takes_an_input_turned_folder_as_gone() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let watching = Watching::start(dir.path(), &["--format", "tgs", "-y"]);
    square_png(&elsewhere.path().join("cat.png"), [255, 0, 0, 255]);
    std::os::unix::fs::symlink(elsewhere.path().join("cat.png"), dir.path().join("cat.png"))
        .unwrap();
    watching.until(|e| named(e, "finished", "cat.sticker.tgs"));
    std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(".swap")).unwrap();
    std::fs::rename(dir.path().join(".swap"), dir.path().join("cat.png")).unwrap();
    watching.during(0.5);
    let files = tgradish_corpus::files();
    let walker = &files.iter().find(|file| file.name == "walker_2x.gif").unwrap().bytes;
    std::fs::write(dir.path().join("cat.gif"), walker).unwrap();
    let events = watching.until(|e| matches!(e["event"].as_str(), Some("finished" | "error")));
    let last = events.last().unwrap();
    assert!(named(last, "finished", "cat.gif"), "{last}");
    assert_eq!(watching.stop().status.code(), Some(0));
}

/// Stopping `watch` with Ctrl-C exits with status 0 whether idle or actively
/// converting (since Ctrl-C is the normal way to end `watch`).
#[cfg(unix)]
#[test]
fn watch_stops_as_done() {
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    let watching = Watching::start(dir.path(), &["--format", "tgs"]);
    let stopped = watching.stop();
    assert_eq!(stopped.status.code(), Some(0));

    // Interrupt while actively converting (`--effort best` keeps encoding in
    // flight).
    let art = noise(dir.path(), 24);
    let watching = Watching::start(dir.path(), &["--format", "tgs", "--effort", "best"]);
    std::fs::rename(art.join("0.png"), dir.path().join("slow.png")).unwrap();
    watching.until(|e| named(e, "started", "slow.png"));
    let stopped = watching.stop();
    assert_eq!(stopped.status.code(), Some(0), "{}", String::from_utf8_lossy(&stopped.stderr));
    assert!(!dir.path().join("slow.sticker.tgs").exists());

    // Text mode prints "Stopped" on stderr when interrupted.
    let child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["watch", dir.path().to_str().unwrap(), "--interval", "0.1"])
        .env("TGRADISH_CONFIG", no_config())
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
fn makes_static_stickers() {
    let Some(input) = clip(Clip::Square) else { return };
    let dir = tempfile::tempdir().unwrap();
    // `.webp` output extension selects static WebP output automatically.
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
    // Emits `started` followed by `finished`, matching all other formats.
    let events = json_lines(&output);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event"], "started");
    assert_eq!(events[0]["output"].as_str(), out.to_str());
    let event = &events[1];
    assert_eq!(event["event"], "finished");
    assert_eq!((event["width"].as_u64(), event["height"].as_u64()), (Some(512), Some(512)));
    assert_eq!(event["encoding"]["kind"], "lossless");
    assert!(event["issues"].as_array().unwrap().is_empty());
    assert_eq!(&std::fs::read(&out).unwrap()[8..12], b"WEBP");
    // Video-only flags are rejected for WebP output under all format aliases.
    for format in ["webp", "static", "image"] {
        let output =
            tgradish(&["convert", input.to_str().unwrap(), "--format", format, "--crf", "30"]);
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--crf cannot be used with webp output"), "{stderr}");
    }
    // Video-only keys in `--options-json` are rejected too.
    let json = r#"{"crf":30,"spoof":"always"}"#;
    let output =
        tgradish(&["convert", input.to_str().unwrap(), "--format", "webp", "--options-json", json]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"crf\" in --options-json, \"spoof\" in --options-json"), "{stderr}");
}

/// Non-UTF-8 file paths are lossily formatted with U+FFFD in JSON and text
/// output instead of causing serialisation failures.
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
        .env("TGRADISH_CONFIG", no_config())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let lines = json_lines(&output);
    let named = |path: &Value| path.as_str().unwrap().ends_with("caf\u{fffd}.png");
    assert_eq!(lines[0]["event"], "started");
    assert!(named(&lines[0]["input"]), "{}", lines[0]);
    let finished = lines.last().unwrap();
    assert!(finished["output"].as_str().unwrap().ends_with("caf\u{fffd}.sticker.tgs"));
    // Plain-text output formats the non-UTF-8 filename lossily as well.
    let output = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["convert", "--format", "tgs", "-y"])
        .arg(&input)
        .env("TGRADISH_CONFIG", no_config())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("caf\u{fffd}.sticker.tgs"), "{stdout}");
    let inspected = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .arg("inspect")
        .arg(input.with_file_name(std::ffi::OsStr::from_bytes(b"caf\xe9.sticker.tgs")))
        .output()
        .unwrap();
    assert!(inspected.status.success(), "{}", String::from_utf8_lossy(&inspected.stderr));
    // Error events for non-UTF-8 paths also format lossily.
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

/// Runs `tgradish` with `XDG_CONFIG_HOME` pointing at `dir` (so user presets
/// are loaded from `dir/tgradish/presets`).
#[cfg(target_os = "linux")]
fn with_presets(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(args)
        .env("TGRADISH_CONFIG", no_config())
        .env("XDG_CONFIG_HOME", dir)
        .output()
        .unwrap()
}

/// `--integer-scale=`, an empty string in `--options-json`, or an empty string
/// in a child preset clears an inherited preset's `integer-scale` and omits it
/// from serialised preset output.
#[cfg(target_os = "linux")]
#[test]
fn clears_a_presets_integer_scale() {
    if !tgradish_corpus::ffmpeg_on_path() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let presets = dir.path().join("tgradish").join("presets");
    std::fs::create_dir_all(&presets).unwrap();
    // 1/3 of 4 px is not an integer, so converting with `third` fails unless
    // cleared.
    std::fs::write(presets.join("third.toml"), "[webm]\ninteger-scale = '1/3'\n").unwrap();
    let cleared = "extends = 'third'\n[webm]\ninteger-scale = ''\n";
    std::fs::write(presets.join("cleared.toml"), cleared).unwrap();
    let input = dir.path().join("square.png");
    square_png(&input, [255, 0, 0, 255]);
    let convert = |preset: &str, more: &[&str]| {
        let mut args = vec!["--ffmpeg-from", "system", "convert", input.to_str().unwrap()];
        args.extend(["--format", "webp", "--preset", preset, "-y"]);
        args.extend(more);
        with_presets(dir.path(), &args)
    };
    let refused = convert("third", &[]);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success() && stderr.contains("multiples of 3"), "{stderr}");
    assert!(stderr.contains("integer-scale"), "{stderr}");
    let json = r#"{"integer-scale":""}"#;
    // Legacy `exact-scale` alias is also accepted.
    let old_json = r#"{"exact-scale":""}"#;
    for (preset, more) in [
        ("third", &["--integer-scale="][..]),
        ("third", &["--options-json", json]),
        ("third", &["--exact-scale="]),
        ("third", &["--options-json", old_json]),
        ("cleared", &[]),
    ] {
        let made = convert(preset, more);
        assert!(
            made.status.success(),
            "{preset} {more:?}: {}",
            String::from_utf8_lossy(&made.stderr)
        );
    }
    let shown = with_presets(dir.path(), &["--json", "preset", "show", "cleared"]);
    let shown: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert!(shown["webm"].get("integer-scale").is_none(), "{shown}");
    let listed = json_lines(&with_presets(dir.path(), &["--json", "preset", "list"]));
    let preset =
        |name: &str| listed.iter().find(|preset| preset["name"] == name).unwrap()["webm"].clone();
    assert_eq!(preset("third")["integer-scale"], "1/3");
    assert!(preset("cleared").get("integer-scale").is_none(), "{}", preset("cleared"));
}

// How a conversion ends for options the GUI sets aside or never checks.
// The scenarios are pinned value by value in crates/cli/src/convert.rs and
// crates/gui/src/item.rs, which number each divergence as these do.

/// Values the GUI sets aside are refused, or kept and warned of, as each
/// input is planned.
#[test]
fn plans_with_values_the_gui_sets_aside() {
    if !tgradish_corpus::ffmpeg_on_path() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png");
    square_png(&png, [255, 0, 0, 255]);
    let convert = |format: &str, more: &[&str]| {
        let mut args = vec!["--json", "--ffmpeg-from", "system", "convert", png.to_str().unwrap()];
        args.extend(["-f", format, "-y"]);
        args.extend(more);
        tgradish(&args)
    };
    let refused = |output: Output, said: &str| {
        assert_eq!(output.status.code(), Some(1), "{said}");
        let events = json_lines(&output);
        assert_eq!(events.last().unwrap()["message"], said, "{events:?}");
    };
    // lossless asked for where fitting has no say: refused before any
    // input is read, where the GUI sets it aside
    let lossless = convert("webm", &["--lossless", "--fit", "auto"]);
    assert_eq!(lossless.status.code(), Some(2));
    let said = "--lossless only works with fit fps, length or off";
    assert_eq!(json_lines(&lossless).last().unwrap()["message"], said);
    assert!(!json_lines(&lossless).iter().any(|event| event["event"] == "started"));
    // an integer scale with a crop it can't go with (divergence 3)
    let cropped = ["--integer-scale", "2", "--scaling", "integer", "--resize", "crop"];
    let said = "resize crop cannot be used with integer-scale; use contain or pad";
    refused(convert("webp", &cropped), said);
    // an integer scale without integer scaling, scaled by (divergence 2)
    let scaled = convert("webp", &["--integer-scale", "2", "--scaling", "smooth"]);
    assert!(scaled.status.success(), "{}", String::from_utf8_lossy(&scaled.stderr));
    let finished = json_lines(&scaled).pop().unwrap();
    assert_eq!((finished["width"].as_u64(), finished["height"].as_u64()), (Some(512), Some(8)));
    // and values fitting ignores, set aside as the GUI does and warned of
    // right after each input starts, which the GUI doesn't do
    // (divergence 7)
    let ignored = ["--fit", "bitrate", "--crf", "20", "--spoof", "never", "--fake-duration", "1"];
    let output = convert("webm", &[&ignored[..], &["--length", "0.5"]].concat());
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let events = json_lines(&output);
    let warned: Vec<&Value> = events
        .iter()
        .skip_while(|event| event["event"] != "started")
        .skip(1)
        .take_while(|event| event["event"] == "warning")
        .map(|event| &event["message"])
        .collect();
    assert_eq!(
        warned,
        [
            "crf is ignored because fit is bitrate",
            "fake-duration is ignored because spoof is never"
        ]
    );
}

/// A preset's bad values fail with 1, naming it, before any input is read;
/// the command line's with 2.
#[cfg(target_os = "linux")]
#[test]
fn refuses_a_presets_bad_values_before_reading_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let presets = dir.path().join("tgradish").join("presets");
    std::fs::create_dir_all(&presets).unwrap();
    for (name, text) in [
        ("slow", "[webm]\ntempo = 50.0\n[tgs]\ntempo = 50.0\n"),
        ("late", "[webm]\nspoof = 'never'\nfake-duration = 5.0\n"),
        ("cropped", "[webm]\ncrop = { x = 0, y = 0, width = 8, height = 8 }\n"),
        ("lossless", "[webm]\nlossless = true\n"),
    ] {
        std::fs::write(presets.join(format!("{name}.toml")), text).unwrap();
    }
    let png = dir.path().join("a.png");
    square_png(&png, [255, 0, 0, 255]);
    let convert = |format: &str, more: &[&str]| {
        let mut args = vec!["--ffmpeg-from", "system", "convert", png.to_str().unwrap()];
        args.extend(["-f", format, "-y"]);
        args.extend(more);
        with_presets(dir.path(), &args)
    };
    let slow = "preset \"slow\" sets tempo, which must be from 0.1 to 10";
    let late = "preset \"late\" sets fake-duration, which must be greater than 0 and at most 3 \
                seconds";
    let cropped = "webm.crop is a per-file option and cannot be set in a preset";
    let sheet = "\"sheet-frames\" in --options-json requires \"sheet\"";
    for (format, more, code, said) in [
        ("webm", &["-p", "slow"][..], 1, slow),
        ("tgs", &["-p", "slow"], 1, slow),
        // though spoofing never uses it (divergence 4)
        ("webm", &["-p", "late"], 1, late),
        ("tgs", &["-p", "cropped"], 1, cropped),
        ("tgs", &["--options-json", r#"{"sheet-frames":3}"#], 2, sheet),
    ] {
        let output = convert(format, more);
        assert_eq!(output.status.code(), Some(code), "{format} {more:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.starts_with("error: ") && stderr.trim_end().ends_with(said), "{stderr}");
        assert!(!stderr.contains("Converting"), "{stderr}");
    }
    if tgradish_corpus::ffmpeg_on_path() {
        // a preset's lossless where fitting has no say, set aside as the
        // GUI does, and warned of
        let output = convert("webm", &["-p", "lossless", "--fit", "auto"]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{stderr}");
        assert!(stderr.contains("lossless is ignored because fit is auto"), "{stderr}");
        // and a picture made though its preset sets a tempo, which a
        // picture doesn't have, as the GUI makes one
        let output = convert("webp", &["-p", "slow"]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{stderr}");
    }
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
    // Saving `preset show` output as a new `.toml` preset produces an
    // identical resolved preset.
    std::fs::write(presets.join("copy.toml"), &text).unwrap();
    let json = |name: &str| {
        let shown = with_presets(dir.path(), &["--json", "preset", "show", name]);
        let mut value: Value = serde_json::from_slice(&shown.stdout).unwrap();
        value.as_object_mut().unwrap().remove("description");
        value
    };
    assert_eq!(json("copy"), json("mine"));
    assert_eq!(json("mine")["webm"]["fit-range"]["max"], 40.0);
    assert_eq!(json("mine")["webm"]["effort"], "fast");

    // `preset list` marks which formats each default preset applies to.
    let listed = with_presets(dir.path(), &["preset", "list"]);
    let listed = console::strip_ansi_codes(&String::from_utf8_lossy(&listed.stdout)).into_owned();
    assert!(listed.contains("balanced (default for webm and webp)"), "{listed}");
    assert!(listed.contains("best (default for tgs)"), "{listed}");
}

/// Converts a 4×4 red square PNG into a 1-second WebM in `dir`, or returns
/// `None` if ffmpeg is unavailable.
fn small_webm(dir: &Path) -> Option<PathBuf> {
    if !tgradish_corpus::ffmpeg_on_path() {
        return None;
    }
    let png = dir.join("square.png");
    square_png(&png, [255, 0, 0, 255]);
    let webm = dir.join("square.webm");
    let made = tgradish(&[
        "--ffmpeg-from",
        "system",
        "convert",
        png.to_str().unwrap(),
        "-o",
        webm.to_str().unwrap(),
        "--length",
        "1",
        "--preset",
        "fast",
    ]);
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    Some(webm)
}

/// Returns the `info` object from `tgradish --json inspect <path>`.
fn webm_info(path: &Path) -> Value {
    json_lines(&tgradish(&["--json", "inspect", path.to_str().unwrap()])).remove(0)["info"].take()
}

#[test]
fn spoofs_durations() {
    let dir = tempfile::tempdir().unwrap();
    let Some(webm) = small_webm(dir.path()) else { return };
    let input = webm.to_str().unwrap();
    // Writes `<stem>.spoofed.webm` beside the input by default.
    let spoofed = dir.path().join("square.spoofed.webm");
    let output = tgradish(&["spoof", input, "-d", "0.5"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Spoofed"));
    assert_eq!(webm_info(&spoofed)["header-duration"], 0.5);
    assert_eq!(webm_info(&webm)["header-duration"], 1.0, "the input is left alone");
    // Refuses to overwrite an existing output without `--overwrite`.
    let again = tgradish(&["spoof", input]);
    assert_eq!(again.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&again.stderr).contains("already exists; pass --overwrite"));
    assert!(tgradish(&["spoof", input, "-y", "-d", "0.25"]).status.success());
    assert_eq!(webm_info(&spoofed)["header-duration"], 0.25);
    // In-place spoofing via `-o <input>` or `-i`.
    let copy = dir.path().join("copy.webm");
    std::fs::copy(&webm, &copy).unwrap();
    let copy = copy.to_str().unwrap();
    assert!(tgradish(&["spoof", copy, "-o", copy, "-d", "0.75"]).status.success());
    assert_eq!(webm_info(Path::new(copy))["header-duration"], 0.75);
    assert!(tgradish(&["spoof", copy, "-i", "--title", "pig"]).status.success());
    // Without `-d`, writes the default spoofed duration (< 0.75 s).
    let info = webm_info(Path::new(copy));
    let duration = info["header-duration"].as_f64().unwrap();
    assert!(duration < 0.75 && info["title"] == "pig", "{info}");
    // Emits a single `finished` JSON event line.
    let output = tgradish(&["--json", "spoof", input, "-y"]);
    let lines = json_lines(&output);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0]["event"].as_str(), lines[0]["input"].as_str()),
        (Some("finished"), Some(input))
    );
    assert!(lines[0]["output"].as_str().unwrap().ends_with("square.spoofed.webm"));
    assert_eq!(lines[0]["report"]["old-duration"], 1.0);
    assert_eq!(lines[0]["issues"], serde_json::json!([]));
    // Writing a duration over 3 s succeeds on disk but exits 3 because
    // Telegram rejects it.
    let output = tgradish(&["spoof", input, "-y", "-d", "5"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Telegram will still reject it"));
    // JSON output includes the `too-long` error issue.
    let output = tgradish(&["--json", "spoof", input, "-y", "-d", "5"]);
    assert_eq!(output.status.code(), Some(3));
    let issues = &json_lines(&output)[0]["issues"];
    assert_eq!(
        (issues[0]["kind"].as_str(), issues[0]["severity"].as_str()),
        (Some("too-long"), Some("error"))
    );
    assert!(issues[0]["message"].is_string());
}

/// Commands that do not use the config file ignore a malformed config, whereas
/// commands that load it fail if an explicitly specified config path is
/// missing.
#[test]
fn reads_the_config_only_where_used() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("config.toml");
    std::fs::write(&broken, "presett = 'fast'\n").unwrap();
    let broken = broken.to_str().unwrap();
    let run = |args: &[&str], env: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tgradish"));
        command.args(args).env_remove("TGRADISH_CONFIG");
        if let Some(env) = env {
            command.env("TGRADISH_CONFIG", env);
        }
        command.output().unwrap()
    };
    for args in [&["config", "path"][..], &["licenses"], &["preset", "path"]] {
        let output = run(args, Some(broken));
        assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    }
    let shown = run(&["config", "show"], Some(broken));
    assert_eq!(shown.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&shown.stderr).contains("presett"));
    // Explicitly named missing config file fails instead of falling back to
    // defaults.
    let missing = dir.path().join("missing.toml");
    let missing = missing.to_str().unwrap();
    let flag = run(&["--config", missing, "config", "show"], None);
    assert_eq!(flag.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&flag.stderr);
    assert_eq!(
        stderr.trim_end(),
        format!("error: config file from --config does not exist: {missing}")
    );
    let env = run(&["preset", "list"], Some(missing));
    let stderr = String::from_utf8_lossy(&env.stderr);
    assert_eq!(
        stderr.trim_end(),
        format!("error: config file from TGRADISH_CONFIG does not exist: {missing}")
    );
    // `--config` overrides `TGRADISH_CONFIG`.
    let flag_wins =
        run(&["--config", no_config().to_str().unwrap(), "preset", "list"], Some(missing));
    assert!(flag_wins.status.success());
    // An unknown preset named in the config file cites the config path and
    // key (exit 1).
    let config = dir.path().join("nope.toml");
    std::fs::write(&config, "tgs-preset = 'nope'\n").unwrap();
    let png = dir.path().join("a.png");
    square_png(&png, [255, 0, 0, 255]);
    let output = run(&["convert", png.to_str().unwrap(), "-f", "tgs"], config.to_str());
    assert_eq!(output.status.code(), Some(1));
    let expected = format!(
        "unknown tgs-preset \"nope\" in config ({}: tgs-preset); available: ",
        config.display()
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains(&expected));
}

/// `config init` creates parent directories and writes the example config to
/// the active config path, refusing to overwrite an existing file.
#[test]
fn starts_a_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str], config: Option<&Path>, home: &Path| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tgradish"));
        command.args(args).env_remove("TGRADISH_CONFIG").env("XDG_CONFIG_HOME", home);
        if let Some(config) = config {
            command.env("TGRADISH_CONFIG", config);
        }
        command.output().unwrap()
    };
    let config = dir.path().join("new/folder/config.toml");
    let made = run(&["config", "init"], Some(&config), dir.path());
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    assert!(String::from_utf8_lossy(&made.stdout).contains("config.toml"));
    let example = run(&["config", "example"], None, dir.path()).stdout;
    assert_eq!(std::fs::read(&config).unwrap(), example);
    // The written example config parses cleanly in `config show`.
    let shown = run(&["--json", "config", "show"], Some(&config), dir.path());
    assert!(shown.status.success(), "{}", String::from_utf8_lossy(&shown.stderr));
    // An existing config file is never overwritten.
    std::fs::write(&config, "preset = 'fast'\n").unwrap();
    let again = run(&["config", "init"], Some(&config), dir.path());
    assert_eq!(again.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&again.stderr).contains("already exists"));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "preset = 'fast'\n");
    // Without `TGRADISH_CONFIG`, writes to the default `config path` location.
    #[cfg(target_os = "linux")]
    {
        let home = dir.path().join("home");
        let quiet = run(&["-q", "config", "init"], None, &home);
        assert!(quiet.status.success(), "{}", String::from_utf8_lossy(&quiet.stderr));
        let path = run(&["config", "path"], None, &home).stdout;
        assert_eq!(quiet.stdout, path);
        assert!(home.join("tgradish/config.toml").is_file());
    }
}

/// `inspect` uses `[ffmpeg] path` from the config file when probing videos and
/// fails on malformed config files just like `convert`.
#[test]
fn inspects_with_the_configs_ffmpeg() {
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg = dir.path().join("no-ffmpeg");
    let config = dir.path().join("config.toml");
    std::fs::write(&config, format!("[ffmpeg]\npath = {:?}\n", ffmpeg.to_str().unwrap())).unwrap();
    let video = dir.path().join("clip.mp4");
    std::fs::write(&video, b"not a video, but ffmpeg would have to say so").unwrap();
    let run = |config: &Path| {
        Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(["inspect", video.to_str().unwrap()])
            .env("TGRADISH_CONFIG", config)
            .env_remove("TGRADISH_FFMPEG")
            .output()
            .unwrap()
    };
    let output = run(&config);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let from = format!("{} (from {}'s [ffmpeg] path)", ffmpeg.display(), config.display());
    assert!(stderr.contains(&from), "{stderr}");
    std::fs::write(&config, "presett = 'fast'\n").unwrap();
    let output = run(&config);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("presett"));
}

#[test]
fn shows_presets_and_the_config() {
    // `preset show` prints valid preset TOML.
    let shown = tgradish(&["preset", "show", "fast"]);
    assert!(shown.status.success());
    let file: toml::Table = toml::from_str(&String::from_utf8(shown.stdout).unwrap()).unwrap();
    assert_eq!(file["webm"]["effort"].as_str(), Some("fast"));
    assert_eq!(file["tgs"]["effort"].as_str(), Some("fast"));
    assert_eq!(tgradish(&["preset", "show", "nope"]).status.code(), Some(1));
    // `preset list --json` emits one JSON object per preset.
    let listed = json_lines(&tgradish(&["--json", "preset", "list"]));
    let names: Vec<&str> = listed.iter().map(|p| p["name"].as_str().unwrap()).collect();
    for name in ["fast", "balanced", "best"] {
        assert!(names.contains(&name), "{names:?}");
    }
    let listed = tgradish(&["preset", "list"]);
    let text = console::strip_ansi_codes(&String::from_utf8_lossy(&listed.stdout)).into_owned();
    assert!(text.contains("fast  ") && text.contains("built-in"), "{text}");

    // An empty config resolves to built-in defaults (including default
    // presets).
    let shown = tgradish(&["config", "show"]);
    assert!(shown.status.success());
    let defaults: toml::Table = toml::from_str(&String::from_utf8(shown.stdout).unwrap()).unwrap();
    assert_eq!(defaults["preset"].as_str(), Some("balanced"));
    assert_eq!(defaults["tgs-preset"].as_str(), Some("best"));
    assert_eq!(json_lines(&tgradish(&["--json", "config", "show"]))[0]["preset"], "balanced");
    // When no config file exists at the default path, `config show` notes
    // that on stderr.
    #[cfg(target_os = "linux")]
    {
        let home = tempfile::tempdir().unwrap();
        let shown = Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(["config", "show"])
            .env_remove("TGRADISH_CONFIG")
            .env("XDG_CONFIG_HOME", home.path())
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&shown.stderr).contains("no config file"));
        let read: toml::Table = toml::from_str(&String::from_utf8(shown.stdout).unwrap()).unwrap();
        assert_eq!(read, defaults);
    }
    // Custom config values override defaults.
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "preset = 'fast'\n").unwrap();
    let config = config.to_str().unwrap();
    let shown = tgradish(&["--config", config, "config", "show"]);
    let read: toml::Table = toml::from_str(&String::from_utf8(shown.stdout).unwrap()).unwrap();
    assert_eq!(read["preset"].as_str(), Some("fast"));
    assert_ne!(read, defaults);
    let json = json_lines(&tgradish(&["--json", "--config", config, "config", "show"]));
    assert_eq!(json[0]["preset"], "fast");
    // `config example` output is a valid config file that parses to defaults.
    let example = tgradish(&["config", "example"]);
    assert!(example.status.success());
    let example = String::from_utf8(example.stdout).unwrap();
    assert!(example.contains("# default preset for WebM and WebP"), "{example}");
    let written = dir.path().join("example.toml");
    std::fs::write(&written, &example).unwrap();
    let shown = tgradish(&["--config", written.to_str().unwrap(), "--json", "config", "show"]);
    assert!(shown.status.success(), "{}", String::from_utf8_lossy(&shown.stderr));
    assert!(json_lines(&shown)[0]["ffmpeg"]["path"].is_null());
    let help = String::from_utf8(tgradish(&["config", "--help"]).stdout).unwrap();
    assert!(help.contains("tgradish config init "), "{help}");
    let path = json_lines(&tgradish(&["--json", "--config", config, "config", "path"]));
    assert_eq!((path[0]["path"].as_str(), path[0]["exists"].as_bool()), (Some(config), Some(true)));
}

/// Pressing Ctrl-C during a plain-text conversion exits with 130 and removes
/// partial outputs.
#[cfg(unix)]
#[test]
fn cancels_text_conversions_with_130() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let Some(input) = clip(Clip::Tall) else { return };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.webm");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tgradish"))
        .args(["--ffmpeg-from", "system", "convert", input.to_str().unwrap()])
        .args(["-o", out.to_str().unwrap()])
        .env("TGRADISH_CONFIG", no_config())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut text = String::new();
    while stderr.read_line(&mut text).unwrap() > 0 && !text.contains("Planned") {}
    Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap();
    std::io::Read::read_to_string(&mut stderr, &mut text).unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130), "{text}");
    assert!(text.contains("cancelled"), "{text}");
    assert!(!out.exists());
}

/// Discovering a closed stdout pipe only when writing the final error event
/// still exits with status 141 (SIGPIPE).
#[cfg(unix)]
#[test]
fn exits_as_stdout_closed_when_only_the_error_finds_it() {
    let commands: [&[&str]; 2] = [
        &["--json", "watch", "/nonexistent-dir"],
        &["--json", "--config", "/dev/null/no.toml", "preset", "list"],
    ];
    for args in commands {
        // Close the read end before the child writes anything.
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let status = Command::new(env!("CARGO_BIN_EXE_tgradish"))
            .args(args)
            .env("TGRADISH_CONFIG", no_config())
            .stdout(writer)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(141), "{args:?}");
    }
}

/// Normalises command exit status, stdout, and stderr for snapshot testing:
/// replaces `dir` with `"dir"`, normalises path separators to `/`, and
/// collapses digit runs to `#` so wording is verified independently of ffmpeg
/// build differences.
fn as_text(output: &Output, dir: &Path) -> String {
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let all = all.replace(&dir.display().to_string(), "dir").replace('\\', "/");
    let mut text = format!("exit {:?}\n", output.status.code());
    for c in all.chars() {
        match c {
            '0'..='9' if text.ends_with('#') => {}
            '0'..='9' => text.push('#'),
            c => text.push(c),
        }
    }
    text
}

/// Verifies the exact plain-text output format of `convert` and `inspect`
/// across all formats.
#[test]
fn prints_conversions_and_inspections_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name).to_str().unwrap().to_owned();
    let png = dir.path().join("dot.png");
    square_png(&png, [255, 0, 0, 255]);
    let art = noise(dir.path(), 24);
    let run = |args: &[&str]| as_text(&tgradish(args), dir.path());
    let png = png.to_str().unwrap();
    let mut printed = vec![
        run(&["convert", png, "--format", "tgs", "-y", "--preset", "fast", "-o", &path("dot.tgs")]),
        // Exact `.tgs` exceeding 64 KiB.
        run(&[
            "convert",
            art.to_str().unwrap(),
            "--sequence",
            "--fps",
            "24",
            "--effort",
            "fast",
            "-y",
            "-o",
            &path("exact.tgs"),
        ]),
        // Fitted `.tgs` (`--no-exact`).
        run(&[
            "convert",
            art.to_str().unwrap(),
            "--sequence",
            "--fps",
            "24",
            "--effort",
            "fast",
            "--no-exact",
            "-y",
            "-o",
            &path("fitted.tgs"),
        ]),
        run(&["inspect", &path("dot.tgs"), &path("exact.tgs"), png]),
    ];
    let mut expected = [
        r#"exit Some(0)
    Finished dir/dot.tgs #.# KiB (#.#%), exact
  Converting dir/dot.png → dir/dot.tgs
        Read #×# cells at #x, # colour, # frame, #.# s
"#,
        r#"exit Some(3)
    Finished dir/exact.tgs #.# KiB (#.#%), exact
  Converting dir/noise → dir/exact.tgs
        Read #×# cells, # colours, # frames, #.# s
warning: about #.# KiB (#.#%) as-is (too large); pass --no-exact to allow lossy fitting
warning: Telegram will reject it: #.# KiB exceeds the # KiB limit
error: # shapes in # layers have complexity # (each layer counts as #); Telegram allows at most #
"#,
        r#"exit Some(0)
    Finished dir/fitted.tgs #.# KiB (#.#%), changed to fit
  Converting dir/noise → dir/fitted.tgs
        Read #×# cells, # colours, # frames, #.# s
     Fitting about #.# KiB (#.#%) as-is (too large)
     Reduced dropped #% of frames → about #.# KiB
     Reduced merged colours closer than #.# → about #.# KiB
     Reduced merged colours closer than #.# → about #.# KiB
     Reduced merged colours closer than #.# → about #.# KiB
"#,
        r#"exit Some(3)
dir/dot.tgs
  name        made with tgradish #.#.#
  size        #.# KiB (#.#%), #.# KiB of JSON
  animation   #×#, # fps, # frames (#.# s)
  drawing     # layer, max # shapes and # fill per layer; cost # of #
  telegram    ✔ accepted

dir/exact.tgs
  name        made with tgradish #.#.#
  size        #.# KiB (#.#%), #.# KiB of JSON
  animation   #×#, # fps, # frames (#.# s)
  drawing     # layers, max # shapes and # fills per layer; cost # of #
  telegram    ✖ rejected:
              - #.# KiB exceeds the # KiB limit
  error: # shapes in # layers have complexity # (each layer counts as #); Telegram allows at most #

dir/dot.png
  kind        picture (not a sticker yet)
  size        #.# KiB
  picture     #×#, transparent
  frames      #
  telegram    ✖ rejected as-is (run `tgradish convert dir/dot.png` to convert it)
              a sticker with the same dimensions and duration would also be rejected:
              - size is #×#, expected one side to be # px and the other at most # px
"#,
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(clip) = clip(Clip::Square) {
        // Copy into `dir` so printed paths normalise to `dir/square.mp#`.
        let square = dir.path().join("square.mp4");
        std::fs::copy(clip, &square).unwrap();
        let ffmpeg = ["--ffmpeg-from", "system"];
        let with_ffmpeg = |args: &[&str]| run(&[&ffmpeg[..], args].concat());
        printed.extend([
            with_ffmpeg(&["convert", png, "--format", "webp", "-y", "-o", &path("dot.webp")]),
            with_ffmpeg(&[
                "-v",
                "convert",
                png,
                "--preset",
                "fast",
                "--fit",
                "off",
                "-y",
                "-o",
                &path("dot.webm"),
            ]),
            // Oversize WebM with fitting disabled.
            with_ffmpeg(&[
                "convert",
                square.to_str().unwrap(),
                "--target",
                "emoji",
                "--fit",
                "off",
                "--crf",
                "0",
                "--length",
                "1",
                "--effort",
                "fast",
                "-y",
                "-o",
                &path("big.webm"),
            ]),
            with_ffmpeg(&["inspect", &path("dot.webp"), &path("dot.webm")]),
            with_ffmpeg(&["inspect", "--target", "emoji", &path("dot.webp"), &path("big.webm")]),
        ]);
        expected.extend(
            [
                r#"exit Some(0)
    Finished dir/dot.webp #.# KiB (#.#%), #×#
  Converting dir/dot.png → dir/dot.webp
"#,
                r#"exit Some(0)
    Finished dir/dot.webm #.# KiB (#.#%), # fps, #.# s, #.# kbit/s
  Converting dir/dot.png → dir/dot.webm
     Planned sticker #×#, # fps, #.# s, fit off
     Encoded #: # fps, #.# s, #.# kbit/s → #.# KiB (#.#%), fits
"#,
                r#"exit Some(3)
    Finished dir/big.webm #.# KiB (#.#%), # fps, #.# s, crf #
  Converting dir/square.mp# → dir/big.webm
     Planned emoji #×#, # fps, #.# s, fit off
     Encoded #: # fps, #.# s, crf # → #.# KiB (#.#%), too large
warning: Telegram will reject it: #.# KiB exceeds the # KiB limit
"#,
                r#"exit Some(0)
dir/dot.webp
  size        #.# KiB (#.#%)
  picture     WebP #×#, lossless, transparent
  made        with tgradish #.#.#
  telegram    ✔ accepted as sticker

dir/dot.webm
  size        #.# KiB (#.#%)
  video       VP# #×#, # fps, # frames, transparent
  duration    #.# s in metadata, #.# s of video
  made        written by tgradish #.#.#, muxed by tgradish #.#.#
  signature   tgradish #.#.# https://github.com/sliva#/tgradish
  telegram    ✔ accepted as sticker
"#,
                r#"exit Some(3)
dir/dot.webp
  size        #.# KiB (#.#%)
  picture     WebP #×#, lossless, transparent
  made        with tgradish #.#.#
  telegram    ✖ rejected as emoji:
              - size is #×#, expected #×#

dir/big.webm
  size        #.# KiB (#.#%)
  video       VP# #×#, # fps, # frames
  duration    #.# s in metadata, #.# s of video
  made        written by tgradish #.#.#, muxed by tgradish #.#.#
  signature   tgradish #.#.# https://github.com/sliva#/tgradish
  telegram    ✖ rejected as emoji:
              - #.# KiB exceeds the # KiB limit
"#,
            ]
            .map(str::to_owned),
        );
    }
    assert_eq!(printed, expected, "{}", printed.join("\n=====\n"));
}

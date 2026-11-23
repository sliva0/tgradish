//! Copyable CLI command lines reproducing the current conversion outside the
//! GUI: the `tgradish` command and, for WebM, the underlying `ffmpeg` commands,
//! quoted for the host OS shell.

use crate::config::Config;
use eframe::egui;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{IntegerScale, Scaling};
use tgradish_core::picture;
use tgradish_core::presets::Format;
use tgradish_core::webm::Options;

use super::Context;
use crate::item::{Item, Kind};
use crate::l10n::fl;
use crate::widgets::{self, note};

/// Text in a monospace box that can be selected and copied.
fn code(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_top(|ui| {
        let width = (ui.available_width() - 40.0).max(200.0);
        let mut shown = text;
        ui.add(
            egui::TextEdit::multiline(&mut shown)
                .code_editor()
                .desired_rows(1)
                .desired_width(width),
        );
        if widgets::named(ui.button("🗐"), &fl!("copy")).on_hover_text(fl!("copy")).clicked() {
            ui.ctx().copy_text(text.to_owned());
        }
    });
}

/// `args` as the shell commands are written for takes them: PowerShell on
/// Windows, a POSIX shell elsewhere.
fn shell_line<S: AsRef<str>>(args: &[S]) -> String {
    if cfg!(windows) { joined(args, powershell_word) } else { posix_line(args) }
}

/// `args` as a POSIX shell takes them.
fn posix_line<S: AsRef<str>>(args: &[S]) -> String {
    joined(args, posix_word)
}

fn joined<S: AsRef<str>>(args: &[S], word: fn(&str) -> String) -> String {
    args.iter().map(|arg| word(arg.as_ref())).collect::<Vec<_>>().join(" ")
}

/// Quotes `word` for a POSIX shell (`bash`/`zsh`). Left unquoted when every
/// character is safe (`A-Za-z0-9+,-./:=@_%` and not starting with `=`, which
/// `zsh` expands as a command path), so `--start=0.16` stays readable;
/// otherwise single-quoted with `'` escaped as `'\''`.
fn posix_word(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "+,-./:=@_%".contains(c);
    let bare = !word.is_empty() && !word.starts_with('=') && word.chars().all(plain);
    if bare { word.to_owned() } else { format!("'{}'", word.replace('\'', r"'\''")) }
}

/// Quotes `word` for PowerShell. Left unquoted when safe, otherwise wrapped in
/// single quotes with any single quote (the ASCII `'` and the typographic
/// `‘ ’ ‚ ‛`, which PowerShell treats as quote characters) doubled. Words
/// starting with `-` that contain `.` are also quoted because PowerShell splits
/// unquoted `-name.more` parameters at the dot.
fn powershell_word(word: &str) -> String {
    const QUOTES: [char; 5] = ['\'', '‘', '’', '‚', '‛'];
    let plain =
        !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || "-_./\\:=+".contains(c));
    let split = word.starts_with('-') && word.contains('.');
    if plain && !split {
        return word.to_owned();
    }
    let mut quoted = String::from("'");
    for c in word.chars() {
        if QUOTES.contains(&c) {
            quoted.push(c);
        }
        quoted.push(c);
    }
    quoted.push('\'');
    quoted
}

/// The `tgradish` command that makes what the GUI makes of `item`.
fn command_line(item: &Item, context: &Context) -> Vec<String> {
    let mut args = vec!["tgradish".to_owned()];
    // what the GUI reads and runs with, where the command line
    // wouldn't by default
    let default_config = Config::default_path();
    if let Some(path) = context.config_path.filter(|&path| Some(path) != default_config.as_deref())
    {
        args.extend(["--config".into(), path.display().to_string()]);
    }
    // .tgs from pixel art needs no ffmpeg; from a video it does
    if item.format != Format::Tgs || item.kind == Kind::Video {
        let ffmpeg = &context.ffmpeg;
        let from = match ffmpeg.choice {
            FfmpegChoice::Auto => None,
            FfmpegChoice::System => Some("system"),
            FfmpegChoice::Builtin => Some("builtin"),
            _ => None,
        };
        match (&ffmpeg.path, from) {
            (Some(path), _) => args.extend(["--ffmpeg".into(), path.display().to_string()]),
            (None, Some(from)) => args.extend(["--ffmpeg-from".into(), from.into()]),
            (None, None) => {}
        }
    }
    args.push("convert".into());
    if item.sequence {
        args.push("--sequence".into());
    }
    args.extend(item.inputs.iter().map(|path| path.display().to_string()));
    if let Some(output) = &context.output {
        args.extend(["-o".into(), output.display().to_string()]);
    }
    match item.format {
        Format::Webm => {}
        Format::Tgs => args.extend(["--format".into(), "tgs".into()]),
        Format::Webp => args.extend(["--format".into(), "webp".into()]),
    }
    if let Some(threads) = context.config.threads.limit() {
        args.extend(["--threads".into(), threads.to_string()]);
    }
    let choices = &item.choices;
    let (presets, config) = (context.presets, context.config);
    // what is set over the preset, as far as it has a say
    let base = choices.webm_base(presets, config).unwrap_or_default();
    let merged = base.clone().merged(&choices.webm);
    let effective = crate::item::shown(merged.clone(), &merged);
    let mut webm = crate::item::shown(choices.webm.clone(), &merged);
    // the preset's values the GUI sets aside, as their settings are
    // greyed out, which the command line would otherwise take up again
    if base.lossless == Some(true) && effective.lossless != Some(true) {
        webm.lossless = Some(false);
    }
    if base.resize.is_some() && effective.resize.is_none() {
        webm.resize = Some(picture::default_resize(choices.target));
    }
    // and an integer scale, which no value sets aside but nothing does
    let no_scale = base.integer_scale.is_some() && effective.integer_scale.is_none();
    let flags = |mut options: Options| {
        if no_scale {
            options.integer_scale = Some(IntegerScale::NONE);
        }
        tgradish_core::options::flags(&options)
    };
    // static stickers take their picture settings from the preset too;
    // named unless it is the format's own, as the GUI's settings may
    // choose another than the config file
    let preset = choices.preset_name(item.format, config);
    if choices.preset.is_some() || preset != item.format.default_preset() {
        args.extend(["--preset".into(), preset.to_owned()]);
    }
    args.extend(match item.format {
        Format::Webm => flags(webm.merged(&choices.shared_webm())),
        Format::Webp => {
            // a picture is one frame, where its start is
            let picture = Options::from(tgradish_core::webp::Options::from(&webm));
            let shared = Options { start: choices.frame, length: None, ..choices.shared_webm() };
            flags(picture.merged(&shared))
        }
        Format::Tgs => {
            tgradish_core::options::flags(&choices.tgs.clone().merged(&choices.shared_tgs()))
        }
    });
    args
}

/// The `tgradish` command that makes the same result, and for WebM the
/// ffmpeg commands that encode it.
pub(super) fn commands(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    widgets::folding_section(ui, &fl!("commands"), "commands", |ui| {
        note(ui, if cfg!(windows) { fl!("commands-powershell") } else { fl!("commands-posix") });
        code(ui, &shell_line(&command_line(item, context)));
        if item.paste_dir.is_some() {
            note(ui, fl!("pasted-temporary"));
        }
        if item.format != Format::Webm {
            return;
        }
        ui.add_space(6.0);
        match ffmpeg_commands(item, context) {
            Ok(lines) => {
                note(
                    ui,
                    if context.ffmpeg_args {
                        fl!("ffmpeg-commands")
                    } else {
                        fl!("ffmpeg-commands", "builtin")
                    },
                );
                for line in lines {
                    code(ui, &line);
                }
            }
            Err(why) => note(ui, fl!("no-ffmpeg-commands", why = why)),
        }
    });
}

fn ffmpeg_commands(item: &Item, context: &Context) -> Result<Vec<String>, String> {
    if matches!(item.kind, Kind::Aseprite | Kind::Frames) {
        return Err(fl!("no-ffmpeg-commands", "art"));
    }
    let video = item.video.ready().ok_or_else(|| fl!("no-ffmpeg-commands", "unread"))?;
    let (request, mut plan) = super::webm::planned(item, context)?;
    // as converting decides it, guessed from the frames the GUI has
    // read, which are those converting reads unless the input is long
    let clip = &video.overview;
    let size = (video.probe.width, video.probe.height);
    let part = crate::predict::Part::of(plan.crop, size, (clip.width, clip.height));
    let looks = || crate::predict::looks_like_art(clip, part, plan.start);
    if picture::turns_sharp(&request.options, plan.enlarges(), looks) {
        plan.scaling = Scaling::Sharp;
    }
    let params = tgradish_core::webm::first_params(&plan);
    Ok(tgradish_core::ffmpeg::commands(&plan, &params)
        .iter()
        .map(|args| {
            let words: Vec<String> =
                args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
            shell_line(&words)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::config::FfmpegConfig;

    #[test]
    fn names_what_the_gui_runs_with() {
        use tgradish_core::presets::Presets;
        use tgradish_core::telegram::Target;

        let presets = Presets::builtin();
        let mut config = Config::default();
        config.ffmpeg.path = Some("/saved/ffmpeg".into());
        config.preset = Some("best".into());
        // the command line that started the GUI chose the system's,
        // over the one the settings name
        let context = Context {
            presets: &presets,
            config: &config,
            ffmpeg: FfmpegConfig { choice: FfmpegChoice::System, path: None },
            ffmpeg_args: true,
            output: None,
            config_path: Some(std::path::Path::new("/elsewhere/config.toml")),
            presets_dir: None,
            in_frames: false,
        };
        let mut video = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        video.format = Format::Webm;
        let line = command_line(&video, &context).join(" ");
        assert!(
            line.starts_with(
                "tgradish --config /elsewhere/config.toml --ffmpeg-from system convert pig.mp4"
            ),
            "{line}"
        );
        assert!(line.contains(" --preset best") && !line.contains("/saved"), "{line}");
        // as many threads as Settings allow, where they say
        assert!(!line.contains("--threads"), "{line}");
        let mut limited = config.clone();
        limited.threads = "2".parse().unwrap();
        let ffmpeg = context.ffmpeg.clone();
        let limited = Context { config: &limited, ffmpeg, output: None, ..context };
        let line = command_line(&video, &limited).join(" ");
        assert!(line.contains(" --threads 2"), "{line}");

        // .tgs needs no ffmpeg, and its preset is the format's own
        let mut art = Item::new(2, vec!["art.gif".into()], false, Target::Sticker);
        art.format = Format::Tgs;
        let default_config = Config::default_path();
        let plain = Context { config_path: default_config.as_deref(), ..context };
        let line = command_line(&art, &plain).join(" ");
        assert!(line.starts_with("tgradish convert art.gif --format tgs"), "{line}");
        assert!(!line.contains("--preset"), "{line}");
        // from a video it does
        video.format = Format::Tgs;
        let line = command_line(&video, &plain).join(" ");
        assert!(line.starts_with("tgradish --ffmpeg-from system convert pig.mp4"), "{line}");

        // what of an Aseprite file is read, whatever it is made into
        let mut walk = Item::new(3, vec!["walk.ase".into()], false, Target::Sticker);
        walk.choices.tag = Some("idle".into());
        walk.choices.hide_layers = Some(vec!["sketch".into(), "back, old".into()]);
        for format in [Format::Webm, Format::Tgs, Format::Webp] {
            walk.format = format;
            // each flag and its value, whether one word or two
            let line = command_line(&walk, &plain).join("\n").replace('=', "\n");
            let read = "--tag\nidle\n--hide-layers\nsketch\n--hide-layers\nback, old";
            assert!(line.contains(read), "{format:?}: {line:?}");
        }

        // a program it was given, by its path
        let given =
            FfmpegConfig { choice: FfmpegChoice::System, path: Some("/given/ffmpeg".into()) };
        let line = command_line(&video, &Context { ffmpeg: given, ..plain }).join(" ");
        assert!(line.starts_with("tgradish --ffmpeg /given/ffmpeg convert pig.mp4"), "{line}");
    }

    #[test]
    fn sets_aside_what_the_gui_does() {
        use tgradish_core::options::{Fit, IntegerScale, Scaling};
        use tgradish_core::presets::Presets;
        use tgradish_core::telegram::Target;

        let dir = tempfile::tempdir().unwrap();
        let preset = "[webm]\nfit = 'off'\nlossless = true\ninteger-scale = '2'\n";
        std::fs::write(dir.path().join("still.toml"), preset).unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let config = Config::default();
        let default_config = Config::default_path();
        let context = Context {
            presets: &presets,
            config: &config,
            ffmpeg: config.ffmpeg.clone(),
            ffmpeg_args: false,
            output: None,
            config_path: default_config.as_deref(),
            presets_dir: None,
            in_frames: false,
        };
        let mut item = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        item.format = Format::Webm;
        item.choices.preset = Some("still".into());
        item.choices.webm.scaling = Some(Scaling::Integer);
        // fitting a bitrate greys out lossless, which the command line
        // would take up from the preset and refuse
        item.choices.webm.fit = Some(Fit::Auto);
        let line = command_line(&item, &context).join(" ");
        assert!(line.contains("--preset still") && line.contains("--no-lossless"), "{line}");
        // without integer scaling the preset's scale has no say, which the
        // command line is told by a scale of nothing
        item.choices.webm.scaling = Some(Scaling::Smooth);
        let line = command_line(&item, &context);
        assert!(line.contains(&"--integer-scale=".to_owned()), "{line:?}");
        assert!(line.join(" ").contains("--preset still"), "{line:?}");
        assert_eq!(
            presets.webm("still").unwrap().integer_scale,
            Some(IntegerScale { up: 2, down: 1 })
        );

        // encoder options all removed: none, over the preset's
        let preset = "[webm]\nencoder-options = { aq-mode = '2' }\n";
        std::fs::write(dir.path().join("tuned.toml"), preset).unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let context = Context { presets: &presets, ..context };
        let mut item = Item::new(2, vec!["pig.mp4".into()], false, Target::Sticker);
        item.choices.preset = Some("tuned".into());
        item.choices.webm.encoder_options = Some(Default::default());
        let line = command_line(&item, &context);
        assert!(line.contains(&"--encoder-options=".to_owned()), "{line:?}");
        assert!(line.join(" ").contains("--preset tuned"), "{line:?}");
    }

    #[test]
    fn quotes_words_for_powershell() {
        assert_eq!(powershell_word("C:\\stickers\\pig.webm"), "C:\\stickers\\pig.webm");
        assert_eq!(powershell_word("--format"), "--format");
        assert_eq!(powershell_word("--crop=0,0,64,64"), "'--crop=0,0,64,64'");
        assert_eq!(powershell_word("my sticker"), "'my sticker'");
        assert_eq!(powershell_word("it's"), "'it''s'");
        assert_eq!(powershell_word("it’s"), "'it’’s'");
        assert_eq!(powershell_word("a,b"), "'a,b'");
        assert_eq!(powershell_word(""), "''");
        // a dot ends a parameter's name, and PowerShell splits it there
        assert_eq!(powershell_word("--start=0.16"), "'--start=0.16'");
        assert_eq!(powershell_word("pig.webm"), "pig.webm");
    }

    /// Words no shell may take for anything but themselves.
    #[cfg(unix)]
    const NASTY: &[&str] = &[
        "",
        " ",
        "'",
        "''",
        "\"",
        "it's",
        "a'b'c",
        "\\",
        "a\\'b",
        "$HOME",
        "${x}",
        "$(echo no)",
        "`echo no`",
        "~",
        "~/x",
        "--x=~/x",
        "a:~/x",
        "#not a comment",
        "a#b",
        "!",
        "!!",
        "a!b",
        "{a,b}",
        "x{1..3}",
        "*",
        "?",
        "[ab]",
        "a*b",
        "^x",
        "=ls",
        "=",
        "==",
        "a;b",
        "a&b",
        "a|b",
        "a>b",
        "a<b",
        "(a)",
        "a\tb",
        "a\nb",
        "-",
        "--",
        "-n",
        "-e",
        "%",
        "%1",
        "@",
        "a=b=c",
        "--title=-_-",
        "--hide-layers=back, old",
        "Стикеры/свинья.png",
        "émoji 🐷.gif",
    ];

    #[test]
    fn quotes_words_for_a_posix_shell() {
        // what reads as plain, plain; the rest quoted
        let line = posix_line(&[
            "tgradish",
            "convert",
            "my stickers/pig.ase",
            "--start=0.16",
            "--length=0.48",
            "--title=-_-",
            "--crop=0,0,64,64",
            "--hide-layers=back, old",
            "--tag=it's",
            "-o",
            "out/100%.webm",
            "a=b",
        ]);
        assert_eq!(
            line,
            "tgradish convert 'my stickers/pig.ase' --start=0.16 --length=0.48 --title=-_- \
             --crop=0,0,64,64 '--hide-layers=back, old' '--tag=it'\\''s' -o out/100%.webm a=b"
        );
        for word in ["~", "~x", "#x", "{x}", "=x", "x y", "", "*", "é"] {
            assert!(posix_word(word).starts_with('\''), "{word:?}");
        }
    }

    // elsewhere the `sh` found takes its arguments through the system's
    // own quoting, so what it reads back is not what this rule wrote
    #[cfg(unix)]
    #[test]
    fn posix_words_read_back_as_themselves() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // and with what is on in a shell typed into: history, as `!!`,
        // which only lines read expand, and zsh's extended globs
        let shells: [(&str, &[&str]); 6] = [
            ("sh", &["-c"]),
            ("bash", &["-c"]),
            ("bash", &["-o", "history", "-o", "histexpand", "-s"]),
            ("dash", &["-c"]),
            ("zsh", &["-f", "-c"]),
            ("zsh", &["-f", "-o", "extendedglob", "-o", "braceccl", "-c"]),
        ];
        let script = format!("printf '%s\\0' {}\n", posix_line(NASTY));
        let mut ran = 0;
        for (shell, flags) in shells {
            let read_in = flags.last() == Some(&"-s");
            let mut command = Command::new(shell);
            command.args(flags).stdin(Stdio::piped()).stdout(Stdio::piped());
            if !read_in {
                command.arg(&script);
            }
            let Ok(mut child) = command.stderr(Stdio::null()).spawn() else { continue };
            let mut stdin = child.stdin.take().unwrap();
            if read_in {
                stdin.write_all(script.as_bytes()).unwrap();
            }
            drop(stdin);
            let output = child.wait_with_output().unwrap();
            ran += 1;
            assert!(output.status.success(), "{shell} {flags:?}: {script}");
            let read: Vec<&str> = std::str::from_utf8(&output.stdout)
                .unwrap()
                .strip_suffix('\0')
                .unwrap()
                .split('\0')
                .collect();
            assert_eq!(read, NASTY, "{shell} {flags:?}");
        }
        // a POSIX system has at least its sh
        assert!(ran > 0);
    }
}

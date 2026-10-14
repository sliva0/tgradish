//! The same conversion as command lines to copy, so it can be repeated
//! or scripted outside the window: the `tgradish` one, and for WebM the
//! ffmpeg ones it runs, quoted for the shell of the system at hand.

use eframe::egui::{self, RichText};
use tgradish_core::config::Config;
use tgradish_core::convert;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{Options, Scaling};
use tgradish_core::presets::Format;
use tgradish_core::tgs;

use super::Context;
use crate::item::{Item, Kind};
use crate::widgets::note;

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
        if ui.button("🗐").on_hover_text("Copy").clicked() {
            ui.ctx().copy_text(text.to_owned());
        }
    });
}

/// The shell commands are written for: PowerShell on Windows, a POSIX
/// shell elsewhere.
const SHELL: &str = if cfg!(windows) { "PowerShell" } else { "a POSIX shell" };

/// `args` as [`SHELL`] takes them.
fn shell_line<S: AsRef<str>>(args: &[S]) -> String {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    if cfg!(windows) {
        args.iter().map(|arg| powershell_word(arg)).collect::<Vec<_>>().join(" ")
    } else {
        shlex::try_join(args.iter().copied()).unwrap_or_else(|_| args.join(" "))
    }
}

/// `word` for PowerShell: as it is when nothing in it is special, else in
/// single quotes, which keep everything but a single quote, doubled.
fn powershell_word(word: &str) -> String {
    let plain =
        !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || "-_./\\:=+".contains(c));
    if plain { word.to_owned() } else { format!("'{}'", word.replace('\'', "''")) }
}

/// The `tgradish` command that makes what the window makes of `item`.
fn command_line(item: &Item, context: &Context) -> Vec<String> {
    let mut args = vec!["tgradish".to_owned()];
    // what the window reads and runs with, where the command line
    // wouldn't by default
    let default_config = Config::default_path();
    if let Some(path) = context.config_path.filter(|&path| Some(path) != default_config.as_deref())
    {
        args.extend(["--config".into(), path.display().to_string()]);
    }
    // .tgs from pixel art needs no ffmpeg; from a video it does
    if item.format != Format::Tgs || item.kind == Kind::Video {
        let ffmpeg = &context.config.ffmpeg;
        let from = match ffmpeg.choice {
            FfmpegChoice::Auto => None,
            FfmpegChoice::System => Some("system"),
            FfmpegChoice::Builtin => Some("builtin"),
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
        Format::Image => args.extend(["--format".into(), "image".into()]),
    }
    let choices = &item.choices;
    let (presets, config) = (context.presets, context.config);
    // what is set over the preset, as far as it has a say
    let base = choices.webm_base(presets, config).unwrap_or_default();
    let merged = base.clone().merged(&choices.webm);
    let effective = crate::item::shown(merged.clone(), &merged);
    let mut webm = crate::item::shown(choices.webm.clone(), &merged);
    // the preset's values the window sets aside, as their settings are
    // greyed out, which the command line would otherwise take up again
    if base.lossless == Some(true) && effective.lossless != Some(true) {
        webm.lossless = Some(false);
    }
    if base.resize.is_some() && effective.resize.is_none() {
        webm.resize = Some(convert::default_resize(choices.target));
    }
    // and one it can't be told to set aside: then everything the window
    // uses goes on the command line, over a built-in preset that sets
    // nothing more than it
    let whole =
        item.format != Format::Tgs && base.exact_scale.is_some() && effective.exact_scale.is_none();
    // image stickers take their picture settings from the preset too;
    // named unless it is the format's own, as the window's settings may
    // choose another than the config file
    let preset = choices.preset_name(item.format, config);
    if whole {
        args.extend(["--preset".into(), "balanced".into()]);
        webm = effective;
    } else if choices.preset.is_some() || preset != item.format.default_preset() {
        args.extend(["--preset".into(), preset.to_owned()]);
    }
    args.extend(match item.format {
        Format::Webm => tgradish_core::options::flags(&webm.merged(&Options {
            target: Some(choices.target),
            tag: choices.tag.clone(),
            show_layers: choices.show_layers.clone(),
            hide_layers: choices.hide_layers.clone(),
            crop: choices.crop,
            start: choices.start,
            length: choices.length,
            ..Options::default()
        })),
        Format::Image => {
            let picture = tgradish_core::still::picture_options(&webm);
            tgradish_core::options::flags(&picture.merged(&Options {
                target: Some(choices.target),
                tag: choices.tag.clone(),
                show_layers: choices.show_layers.clone(),
                hide_layers: choices.hide_layers.clone(),
                crop: choices.crop,
                start: choices.frame,
                ..Options::default()
            }))
        }
        Format::Tgs => {
            tgradish_core::options::flags(&choices.tgs.clone().merged(&tgs::TgsOptions {
                target: Some(choices.target),
                tag: choices.tag.clone(),
                show_layers: choices.show_layers.clone(),
                hide_layers: choices.hide_layers.clone(),
                crop: choices.crop,
                start: choices.start,
                length: choices.length,
                ..tgs::TgsOptions::default()
            }))
        }
    });
    args
}

/// The `tgradish` command that makes the same result, and for WebM the
/// ffmpeg commands that encode it.
pub(super) fn commands(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new("Commands").strong()).id_salt("commands").show(ui, |ui| {
        note(ui, format!("The same conversion on the command line, for {SHELL}:"));
        code(ui, &shell_line(&command_line(item, context)));
        if item.pasted.is_some() {
            note(ui, "A pasted image is kept in a temporary folder, gone when the window closes");
        }
        if item.format != Format::Webm {
            return;
        }
        ui.add_space(6.0);
        match ffmpeg_commands(item, context) {
            Ok(lines) => {
                note(
                    ui,
                    if context.extra_args {
                        "The ffmpeg commands of the first encode; fitting changes the rate, and \
                         maybe the frame rate, between encodes:"
                    } else {
                        "What the built-in ffmpeg does for the first encode, as commands for the \
                         system's ffmpeg; fitting changes the rate, and maybe the frame rate, \
                         between encodes:"
                    },
                );
                for line in lines {
                    code(ui, &line);
                }
            }
            Err(why) => note(ui, format!("No ffmpeg commands: {why}")),
        }
    });
}

fn ffmpeg_commands(item: &Item, context: &Context) -> Result<Vec<String>, String> {
    if matches!(item.kind, Kind::Aseprite | Kind::Frames) {
        return Err("ffmpeg reads a PNG each conversion draws of the pixel art".into());
    }
    let video = item.video.ready().ok_or("the input isn't read yet")?;
    let options = item.choices.webm_options(context.presets, context.config)?;
    let auto = options.auto_scaling();
    let request = convert::Request {
        input: item.inputs[0].clone(),
        sequence: None,
        output: context.output.clone(),
        options,
        overwrite: true,
        keep_temp: false,
    };
    let (mut plan, _) =
        convert::plan(&request, video.probe.clone()).map_err(|err| err.to_string())?;
    // as converting decides it, on the input's frames
    if auto && plan.enlarges() >= convert::AUTO_SHARP_FROM {
        let size = (video.probe.width, video.probe.height);
        let clip = &video.overview;
        let part = crate::predict::Part::of(plan.crop, size, (clip.width, clip.height));
        if crate::predict::looks_like_art(clip, part) {
            plan.scaling = Scaling::Sharp;
        }
    }
    let params = tgradish_core::fit::first_params(&plan, plan.target.max_bytes());
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

    #[test]
    fn names_what_the_window_runs_with() {
        use tgradish_core::presets::Presets;
        use tgradish_core::telegram::Target;

        let presets = Presets::builtin();
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.preset = Some("best".into());
        let context = Context {
            presets: &presets,
            config: &config,
            extra_args: true,
            output: None,
            config_path: Some(std::path::Path::new("/elsewhere/config.toml")),
            presets_dir: None,
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
        assert!(line.contains(" --preset best"), "{line}");

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
        for format in [Format::Webm, Format::Tgs, Format::Image] {
            walk.format = format;
            let line = command_line(&walk, &plain);
            let read = ["--tag", "idle", "--hide-layers", "sketch", "--hide-layers", "back, old"];
            assert!(line.windows(read.len()).any(|words| words == read), "{format:?}: {line:?}");
        }
    }

    #[test]
    fn sets_aside_what_the_window_does() {
        use tgradish_core::options::{ExactScale, Fit, Scaling};
        use tgradish_core::presets::Presets;
        use tgradish_core::telegram::Target;

        let dir = tempfile::tempdir().unwrap();
        let preset = "[webm]\nfit = 'off'\nlossless = true\nexact-scale = '2'\n";
        std::fs::write(dir.path().join("still.toml"), preset).unwrap();
        let presets = Presets::load(Some(dir.path())).unwrap();
        let config = Config::default();
        let default_config = Config::default_path();
        let context = Context {
            presets: &presets,
            config: &config,
            extra_args: false,
            output: None,
            config_path: default_config.as_deref(),
            presets_dir: None,
        };
        let mut item = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        item.format = Format::Webm;
        item.choices.preset = Some("still".into());
        item.choices.webm.scaling = Some(Scaling::PixelPerfect);
        // fitting a bitrate greys out lossless, which the command line
        // would take up from the preset and refuse
        item.choices.webm.fit = Some(Fit::Auto);
        let line = command_line(&item, &context).join(" ");
        assert!(line.contains("--preset still") && line.contains("--lossless=false"), "{line}");
        // without integer scaling the preset's scale has no say, which the
        // command line can't be told
        item.choices.webm.scaling = Some(Scaling::Smooth);
        let line = command_line(&item, &context).join(" ");
        assert!(line.contains("--preset balanced") && !line.contains("--exact-scale"), "{line}");
        assert_eq!(presets.webm("still").unwrap().exact_scale, Some(ExactScale { up: 2, down: 1 }));
    }

    #[test]
    fn quotes_words_for_powershell() {
        assert_eq!(powershell_word("C:\\stickers\\pig.webm"), "C:\\stickers\\pig.webm");
        assert_eq!(powershell_word("my sticker"), "'my sticker'");
        assert_eq!(powershell_word("it's"), "'it''s'");
        assert_eq!(powershell_word("a,b"), "'a,b'");
        assert_eq!(powershell_word(""), "''");
    }
}

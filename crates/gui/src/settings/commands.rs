//! The same conversion as command lines to copy, so it can be repeated
//! or scripted outside the window: the `tgradish` one, and for WebM the
//! ffmpeg ones it runs, quoted for the shell of the system at hand.

use eframe::egui::{self, RichText};
use tgradish_core::convert;
use tgradish_core::options::{Options, Scaling};
use tgradish_core::presets::Format;
use tgradish_core::tgs;

use super::Context;
use crate::item::Item;
use crate::widgets::note;

/// Text in a monospace box that can be selected and copied.
fn code(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_top(|ui| {
        let width = (ui.available_width() - 60.0).max(200.0);
        let mut shown = text;
        ui.add(
            egui::TextEdit::multiline(&mut shown)
                .code_editor()
                .desired_rows(1)
                .desired_width(width),
        );
        if ui.small_button("Copy").clicked() {
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

/// The `tgradish` command that makes the same result, and for WebM the
/// ffmpeg commands that encode it.
pub(super) fn commands(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new("Commands").strong()).id_salt("commands").show(ui, |ui| {
        note(ui, format!("The same conversion on the command line, for {SHELL}:"));
        let mut args = vec!["tgradish".to_owned(), "convert".into()];
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
        // image stickers take their picture settings from the preset too
        if let Some(preset) = &item.choices.preset {
            args.extend(["--preset".into(), preset.clone()]);
        }
        let choices = &item.choices;
        // what is set over the preset, as far as it has a say
        let merged = choices.webm_merged(context.presets, context.config).unwrap_or_default();
        let webm = crate::item::shown(choices.webm.clone(), &merged);
        args.extend(match item.format {
            Format::Webm => tgradish_core::options::flags(&webm.merged(&Options {
                target: Some(choices.target),
                crop: choices.crop,
                start: choices.start,
                length: choices.length,
                ..Options::default()
            })),
            Format::Image => {
                let picture = tgradish_core::still::picture_options(&webm);
                tgradish_core::options::flags(&picture.merged(&Options {
                    target: Some(choices.target),
                    crop: choices.crop,
                    start: choices.frame,
                    ..Options::default()
                }))
            }
            Format::Tgs => tgradish_core::options::flags(&choices.tgs.clone().merged(&tgs::TgsOptions {
                target: Some(choices.target),
                crop: choices.crop,
                start: choices.start,
                length: choices.length,
                ..tgs::TgsOptions::default()
            })),
        });
        code(ui, &shell_line(&args));
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
    let video = item.video.ready().ok_or("the input isn't read yet")?;
    let options = item.choices.webm_options(context.presets, context.config)?;
    let auto = options.auto_scaling();
    let request = convert::Request {
        input: item.inputs[0].clone(),
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
    fn quotes_words_for_powershell() {
        assert_eq!(powershell_word("C:\\stickers\\pig.webm"), "C:\\stickers\\pig.webm");
        assert_eq!(powershell_word("my sticker"), "'my sticker'");
        assert_eq!(powershell_word("it's"), "'it''s'");
        assert_eq!(powershell_word("a,b"), "'a,b'");
        assert_eq!(powershell_word(""), "''");
    }
}

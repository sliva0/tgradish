//! The settings of one item: what to make, beside the preview, and how,
//! under it.

mod advanced;
mod commands;
mod picture;
mod tgs;
mod time;
mod webm;

use eframe::egui::{self, RichText};
use tgradish_core::config::Config;
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::Target;

use crate::item::{Item, Kind};
use crate::widgets::{Choice, note, segments};
use advanced::advanced;
use commands::commands;
use picture::picture;
use tgs::{reading, tgs_quality};
use time::{frame, time};
use webm::webm_quality;

pub(crate) use picture::{exact_of, ratio, resize_of, webm_sizes};

/// What the settings depend on besides the item.
pub struct Context<'a> {
    pub presets: &'a Presets,
    pub config: &'a Config,
    /// ffmpeg is a separate program, so raw arguments can be passed.
    pub extra_args: bool,
    /// Where the result goes.
    pub output: Option<std::path::PathBuf>,
}

/// The built-in presets in the order of their speed, then the user's own.
pub fn preset_names(presets: &Presets) -> Vec<(String, Result<String, String>)> {
    const BUILTIN: [&str; 3] = ["fast", "balanced", "best"];
    let mut names: Vec<(String, Result<String, String>)> = presets
        .iter()
        .map(|(name, preset)| {
            (name.to_owned(), preset.map(|p| p.description.clone()).map_err(str::to_owned))
        })
        .collect();
    names.sort_by_key(|(name, _)| {
        (BUILTIN.iter().position(|b| b == name).unwrap_or(BUILTIN.len()), name.clone())
    });
    names
}

/// What a built-in preset does for `format`.
pub fn builtin(name: &str, format: Format) -> Option<&'static str> {
    Some(match (name, format) {
        ("fast", Format::Webm) => {
            "Fits only the bitrate, with the fast encoder: seconds, but blurrier"
        }
        ("balanced", Format::Webm) => "Tries a few frame rates and keeps the one that looks best",
        ("best", Format::Webm) => {
            "Like balanced, with the slowest encoder: minutes for long videos"
        }
        ("fast", Format::Tgs) => "Under a second, a few percent larger",
        ("balanced", Format::Tgs) => "Between fast and best",
        ("best", Format::Tgs) => "The smallest file, in a few seconds",
        _ => return None,
    })
}

pub fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// What to make: format, target and preset. Narrow, for beside the
/// preview.
pub fn make(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.label(RichText::new("Make").strong().size(15.5));
    ui.add_space(4.0);
    let formats = [
        (Format::Webm, "Video", "A video sticker: VP9 in WebM, from any video or picture"),
        (
            Format::Tgs,
            "Animation",
            "An animated sticker: pixel art drawn as vector shapes, sharp at any size, and small",
        ),
        (Format::Image, "Image", "A static sticker: one frame of a video, or a picture, as WebP"),
    ];
    let refused = match item.kind {
        Kind::Video => {
            "Only pictures in PNG, GIF, WebP, JPEG, BMP or Aseprite files can become a TGS animation"
        }
        _ => "ffmpeg can't read Aseprite files or folders of frames",
    };
    let mut format = item.format;
    let allowed = |format| if item.kind.allows(format) { Ok(()) } else { Err(refused) };
    if segments(ui, &mut format, &formats, allowed) {
        item.format = format;
        item.format_guessed = false;
    }
    let targets = [(Target::Sticker, "Sticker", ""), (Target::Emoji, "Custom emoji", "")];
    segments(ui, &mut item.choices.target, &targets, |_| Ok(()));
    // .tgs animations are vectors: square, but of no size in pixels
    note(
        ui,
        match (item.format, item.choices.target) {
            (Format::Webm, Target::Sticker) => "WebM, 512 px on the longer side, up to 256 KiB",
            (Format::Webm, Target::Emoji) => "WebM, 100 × 100 px, up to 64 KiB",
            (Format::Tgs, _) => "TGS, square, up to 64 KiB",
            (Format::Image, Target::Sticker) => "WebP, 512 px on the longer side, up to 512 KiB",
            (Format::Image, Target::Emoji) => "WebP, 100 × 100 px",
        },
    );

    // presets say how to encode videos and animations; a picture has
    // nothing to choose there
    ui.add_space(8.0);
    let image = item.format == Format::Image;
    let nothing = "Pictures are saved as they are: there is nothing to encode";
    ui.add_enabled_ui(!image, |ui| {
        ui.label("Preset")
            .on_hover_text("How hard to work on the result; your own values below go on top")
            .on_disabled_hover_text(nothing);
        let current = item.choices.preset_name(item.format, context.config).to_owned();
        let default = context.config.preset_for(item.format).to_owned();
        let names = preset_names(context.presets);
        let labels: Vec<String> = names.iter().map(|(name, _)| capitalised(name)).collect();
        let choices: Vec<Choice<&str>> = names
            .iter()
            .zip(&labels)
            .map(|((name, description), label)| {
                (name.as_str(), label.as_str(), description.as_deref().unwrap_or(""))
            })
            .collect();
        let broken = |name: &str| {
            names.iter().find(|(n, _)| n == name).and_then(|(_, d)| d.as_ref().err()).is_some()
        };
        let mut chosen = current.as_str();
        // the built-in three fit side by side; more go in a list
        if choices.len() <= 3 {
            segments(ui, &mut chosen, &choices, |name| {
                if broken(name) { Err("This preset's file is broken") } else { Ok(()) }
            });
        } else {
            let shown = choices.iter().find(|(name, ..)| *name == chosen).map_or("", |c| c.1);
            egui::ComboBox::from_id_salt("preset").selected_text(shown).show_ui(ui, |ui| {
                for &(name, label, hint) in &choices {
                    let option = egui::Button::selectable(chosen == name, label);
                    let response = ui
                        .add_enabled(!broken(name), option)
                        .on_disabled_hover_text("This preset's file is broken");
                    let response =
                        if hint.is_empty() { response } else { response.on_hover_text(hint) };
                    if response.clicked() {
                        chosen = name;
                    }
                }
            });
        }
        if chosen != current {
            item.choices.preset = (chosen != default).then(|| chosen.to_owned());
        }
        match (builtin(chosen, item.format), context.presets.get(chosen)) {
            _ if image => note(ui, nothing),
            (Some(text), Ok(preset)) if preset.path.is_none() => note(ui, text),
            (_, Ok(preset)) => note(ui, preset.description.clone()),
            _ => {}
        }
    });
}

/// Shows the settings under the preview.
pub fn show(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    picture(ui, item, context);
    match item.format {
        Format::Webm | Format::Tgs => time(ui, item, context),
        Format::Image => frame(ui, item),
    }
    match item.format {
        Format::Webm => webm_quality(ui, item, context),
        Format::Tgs => tgs_quality(ui, item, context),
        // lossless, or lossy where only that fits: nothing to choose
        Format::Image => {}
    }
    if item.format == Format::Tgs {
        reading(ui, item);
    }
    if item.format == Format::Webm {
        advanced(ui, item, context);
    }
    commands(ui, item, context);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_presets_by_speed() {
        let names: Vec<String> =
            preset_names(&Presets::builtin()).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["fast", "balanced", "best"]);
    }
}

//! What converting will do to the input, step by step in the order it
//! does them, beside the preview: worked out from the settings alone, the
//! preset's included, so it never waits on a conversion, and saying
//! nothing of how it encodes.

use eframe::egui;
use tgradish_core::convert::{self, Sizes};
use tgradish_core::options::{Crop, Options, Resize, Scaling, Spoof};
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs::TgsOptions;

use tgradish_tgs::frames::Direction;

use super::Context;
use crate::item::{Item, Kind};
use crate::widgets;

/// The steps, under a heading naming what the input is.
pub fn plan(ui: &mut egui::Ui, item: &Item, context: &Context) {
    let animated = item.input_length().is_some();
    let title = match (item.kind, animated) {
        (Kind::Video, true) => "The video will be:",
        (Kind::Video | Kind::Image, false) => "The picture will be:",
        (Kind::Image, true) => "The animation will be:",
        (Kind::Aseprite | Kind::Frames, _) => "The art will be:",
    };
    widgets::section(ui, title);
    for step in steps(item, context) {
        // wrapped lines start under the text, not the bullet
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label("•");
            ui.add(egui::Label::new(step).wrap());
        });
    }
}

/// What of the input is read and used, whichever the format.
struct Used<'a> {
    tag: Option<&'a str>,
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
}

/// What converting will do, in order.
fn steps(item: &Item, context: &Context) -> Vec<String> {
    let (presets, config) = (context.presets, context.config);
    let mut steps = Vec::new();
    match item.format {
        Format::Webm | Format::Image => {
            let options = match item.format {
                Format::Image => item.choices.image_options(presets, config),
                _ => item.choices.webm_options(presets, config),
            };
            let Ok(options) = options else { return steps };
            let used = Used {
                tag: options.tag.as_deref(),
                crop: options.crop,
                start: options.start,
                length: options.length,
            };
            read(item, &used, &mut steps);
            if let Some((width, height)) = item.input_size() {
                let crop = options.crop.unwrap_or(Crop { x: 0, y: 0, width, height });
                steps.extend(picture(&options, (crop.width, crop.height)));
            }
            if item.format == Format::Webm {
                let tempo = options.tempo.unwrap_or(1.0);
                time(item, &used, tempo, options.spoof == Some(Spoof::Never), &mut steps);
            }
        }
        Format::Tgs => {
            let Ok(options) = item.choices.tgs_options(presets, config) else { return steps };
            let used = Used {
                tag: options.tag.as_deref(),
                crop: options.crop,
                start: options.start,
                length: options.length,
            };
            read(item, &used, &mut steps);
            art(&options, &mut steps);
            let tempo = item.tempo(presets, config);
            time(item, &used, tempo, true, &mut steps);
        }
    }
    if let Some(name) = context.output.as_ref().and_then(|output| output.file_name()) {
        steps.push(format!("Saved as {}", name.to_string_lossy()));
    }
    steps
}

/// What is read of the input and cut from it.
fn read(item: &Item, used: &Used, steps: &mut Vec<String>) {
    if item.kind == Kind::Aseprite
        && let Some(name) = used.tag
    {
        let tags = item.aseprite().map_or(&[][..], |(tags, ..)| tags);
        let direction = match tags.iter().find(|tag| tag.name == name).map(|tag| tag.direction) {
            Some(Direction::Reverse) => ", in reverse",
            Some(Direction::PingPong) => ", there and back",
            Some(Direction::PingPongReverse) => ", back and there",
            Some(Direction::Forward) | None => "",
        };
        steps.push(format!("Played as its “{name}” tag{direction}"));
    }
    if let Some(crop) = used.crop {
        steps.push(format!("Cropped to {} × {}", crop.width, crop.height));
    }
    if item.format == Format::Image
        && let Some(frame) = used.start.filter(|_| item.input_length().is_some())
    {
        steps.push(format!("Frame at {} taken", widgets::seconds(frame)));
    }
}

/// The sizes a WebM or image result's picture has, as converting plans
/// them: an exact scale keeps all of the picture, padded.
fn sizes(options: &Options, used: (u32, u32)) -> Option<Sizes> {
    let target = options.target.unwrap_or_default();
    let base = if target.requires_exact_size() { Resize::Pad } else { Resize::Contain };
    let scaling = options.scaling.unwrap_or(Scaling::Auto);
    match options.exact_scale.filter(|_| scaling == Scaling::PixelPerfect) {
        Some(scale) => {
            let resize = options
                .resize
                .filter(|resize| matches!(resize, Resize::Contain | Resize::Pad))
                .unwrap_or(base);
            convert::exact_sizes(target, resize, used, scale).ok()
        }
        None => Some(convert::sizes(target, options.resize.unwrap_or(base), scaling, used)),
    }
}

/// How a `used` picture is scaled, stretched, cut and padded into a WebM
/// or image result.
fn picture(options: &Options, used: (u32, u32)) -> Vec<String> {
    let Some(sizes) = sizes(options, used) else { return Vec::new() };
    let mut steps = Vec::new();
    let scaled = sizes.scaled;
    if scaled != used {
        steps.push(format!("Scaled to {} × {}", scaled.0, scaled.1));
    }
    // more one way than the other: stretched
    let (across, down) = (
        f64::from(scaled.0) / f64::from(used.0.max(1)),
        f64::from(scaled.1) / f64::from(used.1.max(1)),
    );
    if across > down * 1.005 {
        steps.push(format!("Stretched ×{} across", widgets::trimmed(across / down)));
    } else if down > across * 1.005 {
        steps.push(format!("Stretched ×{} down", widgets::trimmed(down / across)));
    }
    if scaled.0 > sizes.width || scaled.1 > sizes.height {
        steps.push(format!("Middle cut to {} × {}", sizes.width, sizes.height));
    }
    if sizes.pads() {
        steps.push(format!("Padded to {} × {}", sizes.width, sizes.height));
    }
    steps
}

/// How a `.tgs` result's art is cut and squared.
fn art(options: &TgsOptions, steps: &mut Vec<String>) {
    if !options.keep_canvas.unwrap_or(false) {
        steps.push("Trimmed to its visible pixels".into());
    }
    if let Some(scale) = options.pixel_scale {
        steps.push(format!("Pixels aligned to a {scale} × {scale} grid"));
    }
    steps.push(
        match options.resize.unwrap_or(Resize::Pad) {
            Resize::Crop => "Cropped to a square",
            Resize::Stretch => "Stretched to a square",
            Resize::Pad | Resize::Contain => "Padded to a square",
        }
        .into(),
    );
}

/// How the result plays at `tempo`: faster or slower, and how its length
/// is kept to Telegram's, `cut` at 3 seconds or told to be shorter.
fn time(item: &Item, used: &Used, tempo: f64, cut: bool, steps: &mut Vec<String>) {
    let Some(length) = item.input_length() else { return };
    let start = used.start.unwrap_or(0.0).clamp(0.0, length);
    let end = used.length.map_or(length, |used| (start + used).min(length));
    if start > 1e-6 || end < length - 1e-6 {
        // a range of seconds, with one unit
        let from = widgets::seconds(start).trim_end_matches(" s").to_owned();
        let (to, of) = (widgets::seconds(end), widgets::seconds(length));
        steps.push(format!("Cut to {from}–{to} of {of}"));
    }
    if tempo > 1.0 + 1e-9 {
        steps.push(format!("Sped up by a factor of {}", widgets::trimmed(tempo)));
    } else if tempo < 1.0 - 1e-9 {
        steps.push(format!("Slowed down by a factor of {}", widgets::trimmed(1.0 / tempo)));
    }
    let played = (end - start) / tempo;
    if !telegram::too_long(played) {
        return;
    }
    let played = widgets::seconds(played);
    if cut {
        steps.push(format!("Cut at 3 s of {played}"));
    } else {
        steps.push(format!("Duration spoofed, so all {played} plays"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::options::ExactScale;
    use tgradish_core::telegram::Target;

    #[test]
    fn says_how_the_picture_fits() {
        // scaled to fill the longer side
        assert_eq!(picture(&Options::default(), (64, 40)), ["Scaled to 512 × 320"]);
        // a whole scale pads a sticker, though it keeps its shape
        let exact = Options {
            scaling: Some(Scaling::PixelPerfect),
            exact_scale: Some(ExactScale { up: 2, down: 1 }),
            ..Options::default()
        };
        let steps = picture(&exact, (64, 40));
        assert_eq!(steps, ["Scaled to 128 × 80", "Padded to 512 × 80"]);
        // stretched to a square
        let stretched = Options { resize: Some(Resize::Stretch), ..Options::default() };
        assert_eq!(picture(&stretched, (64, 40)), ["Scaled to 512 × 512", "Stretched ×1.6 down"]);
        // an emoji, its middle cut to a square
        let cut = Options {
            target: Some(Target::Emoji),
            resize: Some(Resize::Crop),
            ..Options::default()
        };
        assert_eq!(picture(&cut, (64, 40)), ["Scaled to 160 × 100", "Middle cut to 100 × 100"]);
    }
}

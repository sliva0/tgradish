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

use super::Context;
use crate::item::{Item, Kind};
use crate::widgets;

/// The steps, under a heading naming what the input is.
pub fn plan(ui: &mut egui::Ui, item: &Item, context: &Context) {
    let animated = item.input_length().is_some();
    let title = match (item.kind, animated) {
        (Kind::Video, true) => "The video will be",
        (Kind::Video | Kind::Image, false) => "The picture will be",
        (Kind::Image, true) => "The animation will be",
        (Kind::Aseprite | Kind::Frames, _) => "The art will be",
    };
    widgets::section(ui, title);
    for step in steps(item, context) {
        ui.add(egui::Label::new(format!("– {step}")).wrap());
    }
}

/// What of the input is read and used, whichever the format.
struct Used<'a> {
    tag: Option<&'a str>,
    /// How many layers are named to show, and to hide.
    layers: (usize, usize),
    crop: Option<Crop>,
    start: Option<f64>,
    length: Option<f64>,
}

/// What converting will do, in order.
fn steps(item: &Item, context: &Context) -> Vec<String> {
    let (presets, config) = (context.presets, context.config);
    let mut steps = Vec::new();
    let count = |list: &Option<Vec<String>>| list.as_ref().map_or(0, Vec::len);
    match item.format {
        Format::Webm | Format::Image => {
            let options = match item.format {
                Format::Image => item.choices.image_options(presets, config),
                _ => item.choices.webm_options(presets, config),
            };
            let Ok(options) = options else { return steps };
            let used = Used {
                tag: options.tag.as_deref(),
                layers: (count(&options.show_layers), count(&options.hide_layers)),
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
                layers: (count(&options.show_layers), count(&options.hide_layers)),
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
        steps.push(format!("Converted into {}", name.to_string_lossy()));
    }
    steps
}

/// What is read of the input and cut from it.
fn read(item: &Item, used: &Used, steps: &mut Vec<String>) {
    if item.kind == Kind::Aseprite {
        if let Some(tag) = used.tag {
            steps.push(format!("Read from its “{tag}” tag"));
        }
        match used.layers {
            (0, 0) => {}
            (shown, 0) => steps.push(format!("Drawn with {} shown", layers(shown))),
            (0, hidden) => steps.push(format!("Drawn with {} hidden", layers(hidden))),
            (shown, hidden) => {
                steps.push(format!("Drawn with {} shown and {hidden} hidden", layers(shown)))
            }
        }
    }
    if let (Some(crop), Some((width, height))) = (used.crop, item.input_size()) {
        steps.push(format!("Cropped to {} × {} of {width} × {height}", crop.width, crop.height));
    }
    if item.format == Format::Image
        && let Some(frame) = used.start.filter(|_| item.input_length().is_some())
    {
        steps.push(format!("Its frame at {} taken", widgets::seconds(frame)));
    }
}

fn layers(count: usize) -> String {
    if count == 1 { "1 layer".into() } else { format!("{count} layers") }
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
        steps.push(format!("Stretched ×{} across", number(across / down)));
    } else if down > across * 1.005 {
        steps.push(format!("Stretched ×{} down", number(down / across)));
    }
    if scaled.0 > sizes.width || scaled.1 > sizes.height {
        steps.push(format!("Its middle cut to {} × {}", sizes.width, sizes.height));
    }
    if sizes.pads() {
        steps
            .push(format!("Padded to {} × {} with transparent margins", sizes.width, sizes.height));
    }
    steps
}

/// How a `.tgs` result's art is cut and squared.
fn art(options: &TgsOptions, steps: &mut Vec<String>) {
    if !options.keep_canvas.unwrap_or(false) {
        steps.push("Cut to its visible pixels".into());
    }
    if let Some(scale) = options.pixel_scale {
        steps.push(format!("Its pixels moved onto a grid of {scale} × {scale}"));
    }
    steps.push(
        match options.resize.unwrap_or(Resize::Pad) {
            Resize::Crop => "Cropped to a square, by whole art pixels",
            Resize::Stretch => "Stretched to a square",
            Resize::Pad | Resize::Contain => "Padded to a square with transparent margins",
        }
        .into(),
    );
    steps.push("Drawn as vector shapes".into());
}

/// How the result plays at `tempo`: faster or slower, and how its length
/// is kept to Telegram's, `cut` at 3 seconds or told to be shorter.
fn time(item: &Item, used: &Used, tempo: f64, cut: bool, steps: &mut Vec<String>) {
    let Some(length) = item.input_length() else { return };
    let start = used.start.unwrap_or(0.0).clamp(0.0, length);
    let end = used.length.map_or(length, |used| (start + used).min(length));
    if start > 1e-6 || end < length - 1e-6 {
        let (from, to, of) =
            (widgets::seconds(start), widgets::seconds(end), widgets::seconds(length));
        steps.push(format!("Cut to {from} to {to} of its {of}"));
    }
    if tempo > 1.0 + 1e-9 {
        steps.push(format!("Sped up ×{}", number(tempo)));
    } else if tempo < 1.0 - 1e-9 {
        steps.push(format!("Slowed down ×{}", number(tempo)));
    }
    let played = (end - start) / tempo;
    if !telegram::too_long(played) {
        return;
    }
    let played = widgets::seconds(played);
    if cut {
        steps.push(format!("Cut at 3 s of the {played} it plays"));
    } else {
        steps.push(format!("Told Telegram it is short, so all {played} plays"));
    }
}

/// A number, with as few decimals as it needs.
fn number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
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
        assert_eq!(steps, ["Scaled to 128 × 80", "Padded to 512 × 80 with transparent margins"]);
        // stretched to a square
        let stretched = Options { resize: Some(Resize::Stretch), ..Options::default() };
        assert_eq!(picture(&stretched, (64, 40)), ["Scaled to 512 × 512", "Stretched ×1.6 down"]);
        // an emoji, its middle cut to a square
        let cut = Options {
            target: Some(Target::Emoji),
            resize: Some(Resize::Crop),
            ..Options::default()
        };
        assert_eq!(picture(&cut, (64, 40)), ["Scaled to 160 × 100", "Its middle cut to 100 × 100"]);
    }
}

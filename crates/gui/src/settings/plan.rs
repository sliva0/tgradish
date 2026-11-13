//! What converting will do to the input, step by step in the order it
//! does them, beside the preview: worked out from the settings alone, the
//! preset's included, so it never waits on a conversion, and saying
//! nothing of how it encodes.

use eframe::egui;
use tgradish_core::convert;
use tgradish_core::options::{Crop, Options, Resize, Spoof};
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs::TgsOptions;

use tgradish_tgs::frames::aseprite::Direction;

use super::Context;
use crate::item::{Item, Kind};
use crate::l10n::fl;
use crate::{timeline, widgets};

/// The steps, under a heading naming what the input is.
pub fn plan(ui: &mut egui::Ui, item: &Item, context: &Context) {
    let animated = item.input_length().is_some();
    let title = match (item.kind, animated) {
        (Kind::Video, true) => fl!("plan-video"),
        (Kind::Video | Kind::Image, false) => fl!("plan-picture"),
        (Kind::Image, true) => fl!("plan-animation"),
        (Kind::Aseprite | Kind::Frames, _) => fl!("plan-art"),
    };
    widgets::section(ui, &title);
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
        Format::Webm | Format::Webp => {
            let options = match item.format {
                Format::Webp => item.choices.image_options(presets, config),
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
                let crop = options.crop.unwrap_or(Crop::whole((width, height)));
                steps.extend(picture(&options, (crop.width, crop.height)));
            }
            if item.format == Format::Webm {
                let tempo = options.tempo_or_default();
                time(item, &used, tempo, options.spoof_or_default(), &mut steps);
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
            // cut at 3 seconds, as a .tgs can't say it is shorter
            let tempo = item.tempo(presets, config);
            time(item, &used, tempo, Spoof::Never, &mut steps);
        }
    }
    if let Some(name) = context.output.as_ref().and_then(|output| output.file_name()) {
        steps.push(fl!("plan-saved", name = name.to_string_lossy()));
    }
    steps
}

/// What is read of the input and cut from it.
fn read(item: &Item, used: &Used, steps: &mut Vec<String>) {
    if item.kind == Kind::Aseprite
        && let Some(name) = used.tag
    {
        let tags = item.aseprite().map_or(&[][..], |(tags, ..)| tags);
        steps.push(match tags.iter().find(|tag| tag.name == name).map(|tag| tag.direction) {
            Some(Direction::Reverse) => fl!("plan-tag-reverse", tag = name),
            Some(Direction::PingPong) => fl!("plan-tag-ping-pong", tag = name),
            Some(Direction::PingPongReverse) => fl!("plan-tag-pong-ping", tag = name),
            Some(Direction::Forward) | None => fl!("plan-tag", tag = name),
        });
    }
    if let Some(crop) = used.crop {
        steps.push(fl!("plan-cropped", width = crop.width, height = crop.height));
    }
    if item.format == Format::Webp
        && let Some(frame) = used.start.filter(|_| item.input_length().is_some())
    {
        steps.push(fl!("plan-frame", time = widgets::seconds(frame)));
    }
}

/// How a `used` picture is scaled, stretched, cut and padded into a WebM
/// or image result.
fn picture(options: &Options, used: (u32, u32)) -> Vec<String> {
    let Ok(sizes) = convert::picture_sizes(options, used) else { return Vec::new() };
    let mut steps = Vec::new();
    let scaled = sizes.scaled;
    if scaled != used {
        steps.push(fl!("plan-scaled", width = scaled.0, height = scaled.1));
    }
    // more one way than the other: stretched
    let (across, down) = (
        f64::from(scaled.0) / f64::from(used.0.max(1)),
        f64::from(scaled.1) / f64::from(used.1.max(1)),
    );
    if across > down * 1.005 {
        steps.push(fl!("plan-stretched-across", factor = widgets::trimmed(across / down)));
    } else if down > across * 1.005 {
        steps.push(fl!("plan-stretched-down", factor = widgets::trimmed(down / across)));
    }
    let (width, height) = (sizes.width, sizes.height);
    if scaled.0 > width || scaled.1 > height {
        steps.push(fl!("plan-middle-cut", width = width, height = height));
    }
    if sizes.pads() {
        steps.push(fl!("plan-padded", width = width, height = height));
    }
    steps
}

/// How a `.tgs` result's art is cut and squared.
fn art(options: &TgsOptions, steps: &mut Vec<String>) {
    if !options.keep_canvas_or_default() {
        steps.push(fl!("plan-trimmed"));
    }
    if let Some(scale) = options.pixel_scale {
        steps.push(fl!("plan-grid", scale = scale));
    }
    steps.push(match options.resize_or_default() {
        Resize::Crop => fl!("plan-square-crop"),
        Resize::Stretch => fl!("plan-square-stretch"),
        Resize::Pad | Resize::Contain => fl!("plan-square-pad"),
    });
}

/// How the result plays at `tempo`: faster or slower, and how its length
/// is kept to Telegram's as `spoof` says.
fn time(item: &Item, used: &Used, tempo: f64, spoof: Spoof, steps: &mut Vec<String>) {
    let Some(length) = item.input_length() else { return };
    let (start, end) = timeline::part(length, used.start, used.length);
    if start > 1e-6 || end < length - 1e-6 {
        // a range of seconds, with one unit
        let from = widgets::seconds(start).trim_end_matches(" s").to_owned();
        let (to, of) = (widgets::seconds(end), widgets::seconds(length));
        steps.push(fl!("plan-cut", from = from, to = to, length = of));
    }
    if tempo > 1.0 + 1e-9 {
        steps.push(fl!("plan-faster", factor = widgets::trimmed(tempo)));
    } else if tempo < 1.0 - 1e-9 {
        steps.push(fl!("plan-slower", factor = widgets::trimmed(1.0 / tempo)));
    }
    steps.extend(length_step((end - start) / tempo, spoof));
}

/// What is done to a result that plays for `played` to keep to Telegram's
/// length: cut, or its duration spoofed, which "Always spoof" does to
/// short ones too.
fn length_step(played: f64, spoof: Spoof) -> Option<String> {
    let long = telegram::too_long(played);
    let played = widgets::seconds(played);
    match spoof {
        Spoof::Never if long => Some(fl!("plan-cut-3-s", played = played)),
        Spoof::Auto | Spoof::Always if long => Some(fl!("plan-spoofed", played = played)),
        Spoof::Always => Some(fl!("plan-spoofed-anyway", played = played)),
        Spoof::Auto | Spoof::Never => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::options::{ExactScale, Scaling};
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

    #[test]
    fn says_when_the_duration_is_spoofed() {
        let spoofed = |played| fl!("plan-spoofed", played = widgets::seconds(played));
        assert_eq!(length_step(2.0, Spoof::Auto), None);
        assert_eq!(length_step(5.0, Spoof::Auto), Some(spoofed(5.0)));
        // always, even a short one
        let anyway = fl!("plan-spoofed-anyway", played = widgets::seconds(2.0));
        assert_eq!(length_step(2.0, Spoof::Always), Some(anyway));
        assert_eq!(length_step(2.0, Spoof::Never), None);
        assert_eq!(length_step(5.0, Spoof::Never), Some(fl!("plan-cut-3-s", played = "5.00 s")));
    }
}

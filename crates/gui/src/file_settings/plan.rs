//! Step-by-step summary of what conversion will do to the input, shown beside
//! the preview. Computed directly from the current settings and preset without
//! waiting for a conversion, and omits encoder internals.

use eframe::egui;
use tgradish_core::options::{Crop, Resize, Spoof};
use tgradish_core::picture;
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs;
use tgradish_core::webm::Options;

use tgradish_core::tgradish_tgs::frames::aseprite::Direction;

use super::Context;
use crate::item::{Blocker, Item, Kind};
use crate::l10n::fl;
use crate::{cuts, theme, widgets};

/// The steps, under a heading naming what they are done to.
pub(crate) fn plan(ui: &mut egui::Ui, item: &Item, context: &Context) {
    // what it can't do yet is said in its place, higher up
    if item.blocked() {
        return;
    }
    let animated = item.input_length().is_some();
    let title = heading(item.kind, animated, item.input_size().is_some(), item.format);
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

/// What the steps are done to: the input, of a `kind`, `animated` or not
/// once `read`, or one frame of what plays, as a static result is made of.
fn heading(kind: Kind, animated: bool, read: bool, format: Format) -> String {
    let frame = format == Format::Webp;
    // what a video is, while it can't be read
    let plays = animated || (kind == Kind::Video && !read);
    match (kind, plays) {
        (Kind::Video, true) if frame => fl!("plan-video-frame"),
        (Kind::Video, true) => fl!("plan-video"),
        (Kind::Video | Kind::Image, false) => fl!("plan-picture"),
        (Kind::Image, true) if frame => fl!("plan-animation-frame"),
        (Kind::Image, true) => fl!("plan-animation"),
        (Kind::Aseprite | Kind::Frames, true) if frame => fl!("plan-art-frame"),
        (Kind::Aseprite | Kind::Frames, _) => fl!("plan-art"),
    }
}

/// What keeps the item from being converted, if anything does: in a line,
/// or a line each where more does. In place of what converting will do,
/// under what to make, where it is seen before the rest.
pub(crate) fn blocked(ui: &mut egui::Ui, item: &Item) {
    if !item.blocked() {
        return;
    }
    widgets::section_rule(ui);
    let warn = theme::warn(ui.visuals());
    let reasons: Vec<String> = item.blockers.iter().map(Blocker::reason).collect();
    if let [reason] = &reasons[..] {
        let text = format!("⚠ {}", fl!("blocked", reason = reason.as_str()));
        ui.add(egui::Label::new(egui::RichText::new(text).color(warn)).wrap());
        return;
    }
    let heading = format!("⚠ {}", fl!("blocked", "heading"));
    ui.label(widgets::medium(ui, heading, widgets::HEADING).color(warn));
    for reason in reasons {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.colored_label(warn, "•");
            ui.add(egui::Label::new(egui::RichText::new(reason).color(warn)).wrap());
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
                steps.extend(picture(item.format, &options, (crop.width, crop.height)));
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
/// or static (`format`) result.
fn picture(format: Format, options: &Options, used: (u32, u32)) -> Vec<String> {
    let Ok(sizes) = picture::picture_sizes(format, options, used) else { return Vec::new() };
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
fn art(options: &tgs::Options, steps: &mut Vec<String>) {
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
        _ => fl!("plan-square-pad"),
    });
}

/// How the result plays at `tempo`: faster or slower, and how its length
/// is kept to Telegram's as `spoof` says.
fn time(item: &Item, used: &Used, tempo: f64, spoof: Spoof, steps: &mut Vec<String>) {
    let Some(length) = item.input_length() else { return };
    let (start, end) = cuts::part(length, used.start, used.length);
    if start > 1e-6 || end < length - 1e-6 {
        // a range of seconds, with one unit
        let from = widgets::seconds(start).trim_end_matches(" s").to_owned();
        let (to, of) = (widgets::seconds(end), widgets::seconds(length));
        steps.push(fl!("plan-cut", from = from, to = to, length = of));
    }
    if tempo > 1.0 + 1e-9 {
        // as its field writes it, which changes as it is dragged
        steps.push(fl!("plan-faster", factor = widgets::speed(tempo)));
    } else if tempo < 1.0 - 1e-9 {
        steps.push(fl!("plan-slower", factor = widgets::speed(1.0 / tempo)));
    }
    steps.extend(length_step((end - start) / tempo, spoof));
}

/// What is done to a result that plays for `played` to keep to Telegram's
/// length: cut, or its duration spoofed, which spoofing "Always" does to
/// short ones too.
fn length_step(played: f64, spoof: Spoof) -> Option<String> {
    let long = telegram::too_long(played);
    let played = widgets::seconds(played);
    match spoof {
        Spoof::Never if long => Some(fl!("plan-cut-3-s", played = played)),
        Spoof::Auto | Spoof::Always if long => Some(fl!("plan-spoofed", played = played)),
        Spoof::Always => Some(fl!("plan-spoofed-anyway", played = played)),
        Spoof::Auto | Spoof::Never => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::options::{IntegerScale, Scaling};
    use tgradish_core::telegram::Target;

    #[test]
    fn says_how_the_picture_fits() {
        // scaled to fill the longer side
        assert_eq!(picture(Format::Webm, &Options::default(), (64, 40)), ["Scaled to 512 × 320"]);
        // a whole scale pads a sticker, though it keeps its shape
        let exact = Options {
            scaling: Some(Scaling::Integer),
            integer_scale: Some(IntegerScale { up: 2, down: 1 }),
            ..Options::default()
        };
        let steps = picture(Format::Webm, &exact, (64, 40));
        assert_eq!(steps, ["Scaled to 128 × 80", "Padded to 512 × 80"]);
        // stretched to a square
        let stretched = Options { resize: Some(Resize::Stretch), ..Options::default() };
        assert_eq!(
            picture(Format::Webm, &stretched, (64, 40)),
            ["Scaled to 512 × 512", "Stretched ×1.6 down"]
        );
        // an emoji, its middle cut to a square
        let cut = Options {
            target: Some(Target::Emoji),
            resize: Some(Resize::Crop),
            ..Options::default()
        };
        assert_eq!(
            picture(Format::Webm, &cut, (64, 40)),
            ["Scaled to 160 × 100", "Middle cut to 100 × 100"]
        );
    }

    #[test]
    fn heads_the_steps_with_what_they_are_done_to() {
        let heading = |kind, animated, format| heading(kind, animated, true, format);
        // the whole input, into a video or an animated sticker
        for format in [Format::Webm, Format::Tgs] {
            assert_eq!(heading(Kind::Video, true, format), fl!("plan-video"));
            assert_eq!(heading(Kind::Image, true, format), fl!("plan-animation"));
            assert_eq!(heading(Kind::Aseprite, true, format), fl!("plan-art"));
        }
        // one frame of it into a static sticker
        assert_eq!(heading(Kind::Video, true, Format::Webp), fl!("plan-video-frame"));
        assert_eq!(heading(Kind::Image, true, Format::Webp), fl!("plan-animation-frame"));
        assert_eq!(heading(Kind::Frames, true, Format::Webp), fl!("plan-art-frame"));
        // or what has only one, whole
        assert_eq!(heading(Kind::Video, false, Format::Webp), fl!("plan-picture"));
        assert_eq!(heading(Kind::Image, false, Format::Webp), fl!("plan-picture"));
        assert_eq!(heading(Kind::Aseprite, false, Format::Webp), fl!("plan-art"));
        // a video not read yet is taken to play
        assert_eq!(
            super::heading(Kind::Video, false, false, Format::Webp),
            fl!("plan-video-frame")
        );
        assert_eq!(super::heading(Kind::Video, false, false, Format::Webm), fl!("plan-video"));
    }

    #[test]
    fn says_a_static_picture_keeps_odd_sides() {
        // the crop's size already: nothing to scale, as a WebM would by a pixel
        assert!(picture(Format::Webp, &Options::default(), (325, 512)).is_empty());
        let scaled = fl!("plan-scaled", width = 326, height = 512);
        assert_eq!(picture(Format::Webm, &Options::default(), (325, 512)), [scaled]);
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

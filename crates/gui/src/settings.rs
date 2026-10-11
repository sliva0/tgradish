//! The settings of one item: what to make, beside the preview, and how,
//! under it.

use eframe::egui::{self, RichText};
use tgradish_core::config::Config;
use tgradish_core::convert;
use tgradish_core::options::{
    Crop, ExactScale, Fit, Options, Range, Resize, Scaling, Speed, Spoof,
};
use tgradish_core::presets::{Format, Presets};
use tgradish_core::telegram::{self, Target};
use tgradish_core::tgs;
use tgradish_tgs::reduce::Kind as Reduction;

use crate::canvas::{Exact, Keep};
use crate::item::{Aspect, Item, Kind};
use crate::widgets::{
    self, Choice, auto_number, chosen_hint, flag, label, note, preset_segments, section, segments,
};

/// What the settings depend on besides the item.
pub struct Context<'a> {
    pub presets: &'a Presets,
    pub config: &'a Config,
    /// ffmpeg is a separate program, so raw arguments can be passed.
    pub extra_args: bool,
    /// Where the result goes.
    pub output: Option<std::path::PathBuf>,
}

const LABELS: f32 = 128.0;

fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).min_col_width(LABELS).spacing([14.0, 10.0]).show(ui, rows);
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
        (Format::Webm, "Video sticker (WebM)", "A VP9 video, from any video or image"),
        (
            Format::Tgs,
            "Vector animation (TGS)",
            "Pixel art drawn as shapes: sharp at any size, and small",
        ),
    ];
    for (format, text, hint) in formats {
        let allowed = item.kind.allows(format);
        let response = ui
            .add_enabled(allowed, egui::RadioButton::new(item.format == format, text))
            .on_hover_text(hint)
            .on_disabled_hover_text(match item.kind {
                Kind::Video => {
                    "Only pictures in PNG, GIF, WebP, JPEG, BMP or Aseprite files can become a TGS animation"
                }
                _ => "ffmpeg can't read Aseprite files or folders of frames",
            });
        if response.clicked() {
            item.format = format;
            item.format_guessed = false;
        }
    }
    ui.add_space(8.0);
    // .tgs animations are vectors: square, but of no size in pixels
    let (sticker, emoji) = match item.format {
        Format::Webm => ("512 px on the longer side, up to 256 KB", "100 × 100 px, up to 64 KB"),
        Format::Tgs => ("Square, up to 64 KB", "Square, up to 64 KB"),
    };
    let targets = [(Target::Sticker, "Sticker", sticker), (Target::Emoji, "Custom emoji", emoji)];
    segments(ui, &mut item.choices.target, &targets, |_| Ok(()));
    chosen_hint(ui, item.choices.target, &targets);

    ui.add_space(8.0);
    ui.label("Preset")
        .on_hover_text("How hard to work on the result; your own values below go on top");
    let current = item.choices.preset_name(item.format, context.config).to_owned();
    let default = context.config.preset_for(item.format).to_owned();
    let names = preset_names(context.presets);
    let mut picked = current.clone();
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
    let mut chosen = picked.as_str();
    if segments(ui, &mut chosen, &choices, |name| {
        if broken(name) { Err("This preset's file is broken") } else { Ok(()) }
    }) {
        picked = chosen.to_owned();
        item.choices.preset = (picked != default).then(|| picked.clone());
    }
    match (builtin(&picked, item.format), context.presets.get(&picked)) {
        (Some(text), Ok(preset)) if preset.path.is_none() => note(ui, text),
        (_, Ok(preset)) => note(ui, preset.description.clone()),
        _ => {}
    }
}

/// The shapes the crop can keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Free,
    Preserve,
    Custom,
}

const SHAPES: [Choice<Shape>; 3] = [
    (Shape::Free, "Free", "Any shape"),
    (Shape::Preserve, "Preserve", "The input's own shape"),
    (Shape::Custom, "Custom", "A shape you type: as a ratio, or as the result's size"),
];

/// The width to height ratio `aspect` keeps for a `size` input.
pub fn ratio(aspect: Aspect, size: (u32, u32)) -> Option<f64> {
    match aspect {
        Aspect::Free => None,
        Aspect::Input => Some(f64::from(size.0) / f64::from(size.1.max(1))),
        Aspect::Ratio(width, height) => Some(width / height),
    }
}

/// `crop` made `ratio` times as wide as it is high, about as large and
/// around the same middle, within a `size` picture.
pub fn to_ratio(crop: Crop, ratio: f64, size: (u32, u32)) -> Crop {
    let area = f64::from(crop.width) * f64::from(crop.height);
    let (width, height) = ((area * ratio).sqrt(), (area / ratio).sqrt());
    let shrink = (f64::from(size.0) / width).min(f64::from(size.1) / height).min(1.0);
    let width = ((width * shrink).round() as u32).clamp(1, size.0);
    let height = ((height * shrink).round() as u32).clamp(1, size.1);
    let middle = |start: u32, old: u32, new: u32, room: u32| {
        let at = i64::from(start) + (i64::from(old) - i64::from(new)) / 2;
        at.clamp(0, i64::from(room - new)) as u32
    };
    Crop {
        x: middle(crop.x, crop.width, width, size.0),
        y: middle(crop.y, crop.height, height, size.1),
        width,
        height,
    }
}

/// `width` to `height` in the smallest whole numbers: 1920 to 1080 is
/// 16 to 9.
fn reduced(width: u32, height: u32) -> (f64, f64) {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let divisor = gcd(width, height).max(1);
    (f64::from(width / divisor), f64::from(height / divisor))
}

/// How WebM results become square, if they do.
const SQUARES: [Choice<Resize>; 4] = [
    (Resize::Contain, "Off", "The result keeps the crop's shape"),
    (Resize::Pad, "By padding", "Transparent bars fill the rest of the square"),
    (
        Resize::Crop,
        "By cropping",
        "The middle fills the square and the rest is cut off: the preview dims it",
    ),
    (Resize::Stretch, "By stretching", "The picture is stretched to the square"),
];

/// Shows the settings under the preview.
pub fn show(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    picture(ui, item, context);
    time(ui, item, context);
    match item.format {
        Format::Webm => webm_quality(ui, item, context),
        Format::Tgs => tgs_quality(ui, item, context),
    }
    if item.format == Format::Tgs {
        reading(ui, item);
    }
    metadata(ui, item);
    if item.format == Format::Webm {
        advanced(ui, item, context);
    }
    commands(ui, item, context);
}

/// libvpx-vp9 options worth knowing, offered to add with a click.
const ENCODER_EXAMPLES: [(&str, &str, &str); 7] = [
    ("tune-content", "screen", "Flat graphics and screen recordings: sharper edges"),
    ("aq-mode", "2", "Spends bits where the picture is complex"),
    ("sharpness", "4", "Keeps more fine detail, at some cost in smoothness"),
    ("arnr-strength", "3", "Less noise reduction across frames"),
    ("lag-in-frames", "25", "Looks further ahead to spend bits where they count"),
    ("g", "60", "A keyframe at least every 60 frames"),
    ("qmax", "50", "Never worse than quality 50 anywhere"),
];

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
fn commands(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
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
        if item.format == Format::Tgs {
            args.extend(["--format".into(), "tgs".into()]);
        }
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
    let auto =
        options.scaling.unwrap_or(Scaling::Auto) == Scaling::Auto && options.exact_scale.is_none();
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
    if auto && plan.enlarges() >= 2.0 {
        let size = (video.probe.width, video.probe.height);
        let clip = &video.overview;
        let part = crate::output::Part::of(plan.crop, size, (clip.width, clip.height));
        if crate::output::looks_like_art(clip, part) {
            plan.scaling = Scaling::Sharp;
        }
    }
    let params = tgradish_core::fit::first_params(&plan);
    Ok(tgradish_core::ffmpeg::commands(&plan, &params)
        .iter()
        .map(|args| {
            let words: Vec<String> =
                args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
            shell_line(&words)
        })
        .collect())
}

const SCALINGS: [Choice<Scaling>; 4] = [
    (
        Scaling::Auto,
        "Auto",
        "Sharp pixels for pixel art made twice as large or more, smooth otherwise",
    ),
    (Scaling::Smooth, "Smooth", "Neighbouring pixels blend, in linear light so edges don't darken"),
    (
        Scaling::Sharp,
        "Sharp pixels",
        "Each pixel becomes a block; only block edges between result pixels blend",
    ),
    (
        Scaling::PixelPerfect,
        "Integer",
        "Each pixel becomes the same whole number of result pixels, or whole blocks of them \
         become one; transparent margins fill the rest",
    ),
];

/// The exact scale the item's WebM result has, when integer scaling is
/// fixed to one.
fn exact_scale_of(item: &Item) -> Option<ExactScale> {
    let webm = &item.choices.webm;
    let integer = item.format == Format::Webm && webm.scaling == Some(Scaling::PixelPerfect);
    webm.exact_scale.filter(|_| integer)
}

/// Where a `crop` of the input lands in the item's WebM result; `None`
/// when its exact scale can't make it.
pub fn webm_sizes(item: &Item, crop: Crop) -> Option<convert::Sizes> {
    let target = item.choices.target;
    let used = (crop.width, crop.height);
    match exact_scale_of(item) {
        Some(scale) => convert::exact_sizes(target, resize_of(item), used, scale).ok(),
        None => {
            let scaling = item.choices.webm.scaling.unwrap_or(Scaling::Auto);
            Some(convert::sizes(target, resize_of(item), scaling, used))
        }
    }
}

/// How the item's WebM result fills its box. An exact scale keeps all of
/// the picture, so it isn't cut or stretched.
pub fn resize_of(item: &Item) -> Resize {
    let base =
        if item.choices.target.requires_exact_size() { Resize::Pad } else { Resize::Contain };
    let whole = exact_scale_of(item).is_some();
    item.choices
        .webm
        .resize
        .filter(|resize| !whole || matches!(resize, Resize::Contain | Resize::Pad))
        .unwrap_or(base)
}

/// The size an exact scale holds the item's crop to, for WebM.
pub fn exact_of(item: &Item) -> Option<Exact> {
    Exact::of(exact_scale_of(item)?, item.choices.target)
}

/// Integer scales as whole steps: 0 is 1x, 1 is 2x, -1 is 1/2.
fn scale_step(scale: ExactScale) -> i64 {
    if scale.down > 1 { 1 - i64::from(scale.down) } else { i64::from(scale.up) - 1 }
}

fn step_scale(step: i64) -> ExactScale {
    if step < 0 {
        ExactScale { up: 1, down: (1 - step) as u32 }
    } else {
        ExactScale { up: (step + 1) as u32, down: 1 }
    }
}

fn scale_text(scale: ExactScale) -> String {
    format!("×{scale}")
}

/// A scale typed as `3`, `x3`, `×3`, `1/3` or `0.5`.
fn parse_scale(text: &str) -> Option<ExactScale> {
    let text = text.trim();
    text.strip_prefix(['×', 'x', 'X', '*']).unwrap_or(text).trim().parse().ok()
}

/// The whole scale that makes the most of a `used` picture for `target`.
fn largest_scale(target: Target, used: (u32, u32)) -> ExactScale {
    let (width, height) = target.box_size();
    let (room, long) = (width.max(height), used.0.max(used.1).max(1));
    if long <= room {
        ExactScale { up: room / long, down: 1 }
    } else {
        ExactScale { up: 1, down: long.div_ceil(room) }
    }
}

fn picture(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(4.0);
    ui.label(RichText::new("Picture").strong().size(15.5));
    ui.add_space(4.0);
    let size = item.input_size();
    let exact = exact_of(item);
    // an exact scale holds the crop to the size it takes
    if let (Some(exact), Some(size)) = (exact, size) {
        let full = Crop { x: 0, y: 0, width: size.0, height: size.1 };
        let ratio = ratio(item.view.aspect, size);
        let start = (Keep::Start, Keep::Start);
        if let Some(fitted) = exact.fit(item.choices.crop.unwrap_or(full), start, ratio, size) {
            item.choices.crop = (fitted != full).then_some(fitted);
        }
    }
    grid(ui, "picture", |ui| {
        label(
            ui,
            "Crop",
            "The part of the input to use, in its pixels. Drag on the preview to change it",
        );
        ui.horizontal_wrapped(|ui| {
            let Some((width, height)) = size else {
                note(ui, "once the input is read");
                return;
            };
            let full = Crop { x: 0, y: 0, width, height };
            let mut crop = item.choices.crop.unwrap_or(full);
            let mut edited = false;
            ui.label("x");
            edited |= ui.add(egui::DragValue::new(&mut crop.x).range(0..=width - 1)).changed();
            ui.label("y");
            edited |= ui.add(egui::DragValue::new(&mut crop.y).range(0..=height - 1)).changed();
            ui.label("  width");
            edited |= ui.add(egui::DragValue::new(&mut crop.width).range(1..=width)).changed();
            ui.label("height");
            edited |= ui.add(egui::DragValue::new(&mut crop.height).range(1..=height)).changed();
            if edited {
                crop.width = crop.width.min(width - crop.x);
                crop.height = crop.height.min(height - crop.y);
                if let Some(exact) = exact {
                    let ratio = ratio(item.view.aspect, (width, height));
                    let start = (Keep::Start, Keep::Start);
                    crop = exact.fit(crop, start, ratio, (width, height)).unwrap_or(crop);
                }
                item.choices.crop = (crop != full).then_some(crop);
            }
            ui.add_space(6.0);
            let whole = if exact.is_some() { "Middle" } else { "Whole picture" };
            if ui
                .add_enabled(item.choices.crop.is_some(), egui::Button::new(whole))
                .on_hover_text(if exact.is_some() {
                    "As much of the middle as the exact scale takes"
                } else {
                    "All of the picture"
                })
                .clicked()
            {
                item.choices.crop = match (exact, size) {
                    (Some(exact), Some(size)) => {
                        let middle = (Keep::Middle, Keep::Middle);
                        let ratio = ratio(item.view.aspect, size);
                        exact.fit(full, middle, ratio, size).filter(|crop| *crop != full)
                    }
                    _ => None,
                };
            }
        });
        ui.end_row();

        label(ui, "Aspect ratio", "The width to height ratio the crop keeps while you drag it");
        ui.vertical(|ui| aspect_ratio(ui, item, size, exact));
        ui.end_row();

        match item.format {
            Format::Webm => {
                let target = item.choices.target;
                label(
                    ui,
                    "Force into square",
                    "How the result becomes square: stickers needn't be, emoji must",
                );
                ui.vertical(|ui| {
                    let base =
                        if target.requires_exact_size() { Resize::Pad } else { Resize::Contain };
                    let mut resize = resize_of(item);
                    ui.horizontal(|ui| {
                        let allowed = |option| match option {
                            Resize::Contain if target.requires_exact_size() => {
                                Err("Emoji must be square")
                            }
                            Resize::Crop | Resize::Stretch if exact.is_some() => Err(
                                "A fixed integer scale keeps all of the picture: pad it instead",
                            ),
                            _ => Ok(()),
                        };
                        if segments(ui, &mut resize, &SQUARES, allowed) {
                            item.choices.webm.resize = (resize != base).then_some(resize);
                        }
                        if item.choices.webm.resize.is_some() && widgets::reset(ui) {
                            item.choices.webm.resize = None;
                        }
                    });
                    chosen_hint(ui, resize, &SQUARES);
                });
                ui.end_row();

                label(ui, "Scaling", "How the input's pixels become the result's");
                ui.vertical(|ui| {
                    preset_segments(ui, &mut item.choices.webm.scaling, Scaling::Auto, &SCALINGS);
                    let scaling = item.choices.webm.scaling.unwrap_or(Scaling::Auto);
                    chosen_hint(ui, scaling, &SCALINGS);
                });
                ui.end_row();
                if item.choices.webm.scaling == Some(Scaling::PixelPerfect) {
                    label(
                        ui,
                        "Integer scale",
                        "How many result pixels each input pixel becomes, or how many input pixels make one",
                    );
                    ui.vertical(|ui| integer_scale(ui, item, size));
                    ui.end_row();
                }
            }
            Format::Tgs => {
                let base = item
                    .choices
                    .tgs_base(context.presets, context.config)
                    .ok()
                    .and_then(|base| base.keep_canvas)
                    .unwrap_or(false);
                label(ui, "Margins", "Rows and columns that are transparent in every frame");
                ui.vertical(|ui| {
                    let mut auto = item.choices.tgs.keep_canvas.map(|keep| !keep);
                    if flag(
                        ui,
                        &mut auto,
                        !base,
                        "Auto crop empty margins",
                        "Cuts the art to the smallest rectangle with visible pixels in any frame, so it fills the square",
                    ) {
                        item.choices.tgs.keep_canvas = auto.map(|auto| !auto);
                    }
                    note(ui, "Applies on top of the manual crop");
                });
                ui.end_row();
                label(
                    ui,
                    "Art pixel size",
                    "How many input pixels one pixel of the art takes. Pixels off its grid are moved onto it",
                );
                auto_number(ui, &mut item.choices.tgs.pixel_scale, "Detected", 2, |value| {
                    value.range(1..=64).suffix(" px")
                });
                ui.end_row();
            }
        }
    });
}

/// The shape the crop keeps: none, the input's, or one typed as a ratio
/// or, where the result takes the crop's shape, as the result's size.
fn aspect_ratio(
    ui: &mut egui::Ui,
    item: &mut Item,
    size: Option<(u32, u32)>,
    exact: Option<Exact>,
) {
    let full = size.map(|(width, height)| Crop { x: 0, y: 0, width, height });
    let crop = item.choices.crop.or(full);
    let mut shape = match item.view.aspect {
        Aspect::Free => Shape::Free,
        Aspect::Input => Shape::Preserve,
        Aspect::Ratio(..) => Shape::Custom,
    };
    if segments(ui, &mut shape, &SHAPES, |_| Ok(())) {
        item.view.aspect = match shape {
            Shape::Free => Aspect::Free,
            Shape::Preserve => Aspect::Input,
            // the crop's own shape to start from
            Shape::Custom => {
                let (width, height) = crop.map_or((1.0, 1.0), |c| reduced(c.width, c.height));
                Aspect::Ratio(width, height)
            }
        };
        keep_aspect(item, size, exact);
    }
    if let Aspect::Ratio(mut width, mut height) = item.view.aspect {
        let mut typed = false;
        ui.horizontal(|ui| {
            fn side(value: &mut f64) -> egui::DragValue<'_> {
                egui::DragValue::new(value)
                    .range(0.01..=10_000.0)
                    .speed(0.05)
                    .max_decimals(3)
                    .custom_formatter(|n, _| {
                        let text = format!("{n:.3}");
                        text.trim_end_matches('0').trim_end_matches('.').to_owned()
                    })
            }
            typed |= ui.add(side(&mut width)).on_hover_text("Width").changed();
            ui.label(":");
            typed |= ui.add(side(&mut height)).on_hover_text("Height").changed();
        });
        // the result's size, where the result takes the crop's shape: not
        // squared, nor with margins
        let sizes = crop.and_then(|crop| webm_sizes(item, crop)).filter(|sizes| !sizes.pads());
        let follows = item.format == Format::Webm && resize_of(item) == Resize::Contain;
        if follows && let Some(sizes) = sizes {
            ui.horizontal(|ui| {
                ui.label("Result size");
                let (box_w, box_h) = item.choices.target.box_size();
                let (mut result_w, mut result_h) = (sizes.width, sizes.height);
                let across = ui.add(egui::DragValue::new(&mut result_w).range(1..=box_w));
                ui.label("×");
                let down = ui.add(egui::DragValue::new(&mut result_h).range(1..=box_h));
                ui.label("px");
                if across.changed() || down.changed() {
                    (width, height) = reduced(result_w, result_h);
                    typed = true;
                }
            });
        }
        if typed {
            item.view.aspect = Aspect::Ratio(width, height);
            keep_aspect(item, size, exact);
        }
    }
    let square =
        item.choices.crop.map_or(size.is_none_or(|(w, h)| w == h), |c| c.width == c.height);
    if item.format == Format::Tgs && !square {
        ui.colored_label(
            widgets::warn(ui),
            "⚠ TGS stickers are always square: other shapes get transparent margins",
        );
    }
}

/// Gives the item's crop the shape its aspect keeps.
fn keep_aspect(item: &mut Item, size: Option<(u32, u32)>, exact: Option<Exact>) {
    let Some(size) = size else { return };
    let Some(r) = ratio(item.view.aspect, size) else { return };
    let full = Crop { x: 0, y: 0, width: size.0, height: size.1 };
    let mut cut = to_ratio(item.choices.crop.unwrap_or(full), r, size);
    if let Some(exact) = exact {
        cut = exact.fit(cut, (Keep::Middle, Keep::Middle), Some(r), size).unwrap_or(cut);
    }
    item.choices.crop = (cut != full).then_some(cut);
}

/// Integer scaling: the largest whole multiple that fits, or a scale of
/// your own, with what it makes.
fn integer_scale(ui: &mut egui::Ui, item: &mut Item, size: Option<(u32, u32)>) {
    let full = size.map(|(width, height)| Crop { x: 0, y: 0, width, height });
    let Some(crop) = item.choices.crop.or(full) else {
        note(ui, "once the input is read");
        return;
    };
    let target = item.choices.target;
    let scale = item.choices.webm.exact_scale;
    let guess = largest_scale(target, (crop.width, crop.height));
    let (box_w, box_h) = target.box_size();
    // a pixel at most the result's size, and a whole block of the input
    let highest = i64::from(box_w.max(box_h)) - 1;
    let lowest = 1 - i64::from(size.map_or(1, |(w, h)| w.min(h)).max(1));
    let mut chosen = scale;
    ui.horizontal(|ui| {
        if ui
            .radio(scale.is_none(), "Largest that fits")
            .on_hover_text("The largest whole multiple that keeps the crop within the result")
            .clicked()
        {
            chosen = None;
        }
        if ui
            .radio(scale.is_some(), "Fixed")
            .on_hover_text("This scale, the crop held to what it fits into the result")
            .clicked()
            && scale.is_none()
        {
            chosen = Some(guess);
        }
        ui.add_enabled_ui(scale.is_some(), |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let current = scale_step(scale.unwrap_or(guess));
            let mut step = current;
            if ui.small_button("-").on_hover_text("Smaller").clicked() {
                step -= 1;
            }
            ui.add(
                egui::DragValue::new(&mut step)
                    .range(lowest..=highest)
                    .speed(0.05)
                    .custom_formatter(|n, _| scale_text(step_scale(n as i64)))
                    .custom_parser(|text| parse_scale(text).map(|s| scale_step(s) as f64)),
            )
            .on_hover_text("Type 3 for three times as large, or 1/3 for a third");
            if ui.small_button("+").on_hover_text("Larger").clicked() {
                step += 1;
            }
            let step = step.clamp(lowest, highest);
            if scale.is_some() && step != current {
                chosen = Some(step_scale(step));
            }
        });
    });
    if chosen != scale {
        item.choices.webm.exact_scale = chosen;
        return;
    }
    let Some(sizes) = webm_sizes(item, crop) else {
        note(ui, "The crop doesn't suit this scale yet");
        return;
    };
    let made = format!("{} × {} px", sizes.scaled.0, sizes.scaled.1);
    let margins = if sizes.pads() {
        format!("; transparent margins fill the rest of the {} × {}", sizes.width, sizes.height)
    } else {
        String::new()
    };
    note(
        ui,
        match scale {
            Some(scale) => format!("{} makes the crop {made}{margins}", scale_text(scale)),
            None if sizes.scaling == Scaling::PixelPerfect => {
                let times = sizes.scaled.0 / crop.width.max(1);
                format!("×{times} makes the crop {made}{margins}")
            }
            None => "The crop is larger than the result, so it shrinks with sharp pixels: a \
                     fixed scale like ×1/2 shrinks it by whole blocks"
                .into(),
        },
    );
}

const SPOOFS: [Choice<Spoof>; 3] = [
    (
        Spoof::Auto,
        "Spoof when longer",
        "Longer stickers get a short duration written into them: Telegram accepts them and plays all of it",
    ),
    (Spoof::Always, "Always spoof", "Even short stickers get the short duration"),
    (Spoof::Never, "Cut at 3 s", "Keep the file honest and cut it at 3 seconds"),
];

/// A number with at most two decimals, as short as it goes: 2, 1.5.
fn preset_number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A frame rate as short as it goes: 25 fps, 29.97 fps.
fn fps_text(fps: f64) -> String {
    format!("{} fps", preset_number(fps))
}

fn time(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    let animated = item.input_length().is_some();
    let sequence =
        item.format == Format::Tgs && (item.sequence || item.choices.tgs.sheet.is_some());
    if !animated && !sequence {
        return;
    }
    section(ui, "Time");
    // how long the used part lasts, and plays
    let used = item.range().map(|(start, end)| end - start);
    let tempo = item.tempo(context.presets, context.config);
    let played = used.map(|used| used / tempo);
    // the average rate, and the most a video of variable rate reaches
    let input_fps = match item.format {
        Format::Webm => item.video.ready().map(|video| (video.probe.fps, video.probe.base_fps)),
        Format::Tgs => None,
    };
    // a preset's tempo is what the automatic choice stands for
    let preset_tempo = match item.format {
        Format::Webm => {
            item.choices.webm_base(context.presets, context.config).ok().and_then(|base| base.tempo)
        }
        Format::Tgs => {
            item.choices.tgs_base(context.presets, context.config).ok().and_then(|base| base.tempo)
        }
    };
    grid(ui, "time", |ui| {
        if animated {
            label(ui, "Speed", "How many times as fast as the input the result plays");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let (tempo, automatic) = match (item.format, preset_tempo) {
                        (_, Some(preset)) => (
                            match item.format {
                                Format::Webm => &mut item.choices.webm.tempo,
                                Format::Tgs => &mut item.choices.tgs.tempo,
                            },
                            format!("The preset's: ×{}", preset_number(preset)),
                        ),
                        (Format::Webm, None) => {
                            (&mut item.choices.webm.tempo, "As the input".into())
                        }
                        (Format::Tgs, None) => {
                            (&mut item.choices.tgs.tempo, "Fit into 3 s if longer".into())
                        }
                    };
                    auto_number(ui, tempo, &automatic, 1.0, |value| {
                        value.range(0.1..=10.0).speed(0.01).max_decimals(2).prefix("×")
                    });
                    if item.format == Format::Webm
                        && let Some(used) = used
                        && ui
                            .button("Fit 3 s")
                            .on_hover_text("Plays the used part in 3 seconds")
                            .clicked()
                    {
                        item.choices.webm.tempo =
                            Some((used / telegram::MAX_SECONDS).clamp(0.1, 10.0));
                    }
                });
                // .tgs results, and honest WebM ones, last 3 seconds at most
                let cut =
                    item.format == Format::Tgs || item.choices.webm.spoof == Some(Spoof::Never);
                match played {
                    Some(played) if cut && played > telegram::MAX_SECONDS + 1e-3 => note(
                        ui,
                        format!(
                            "Plays {} of the {}: the rest is cut off",
                            widgets::seconds(telegram::MAX_SECONDS * tempo),
                            widgets::seconds(used.unwrap_or_default())
                        ),
                    ),
                    Some(played) => note(ui, format!("Plays {}", widgets::seconds(played))),
                    None => {}
                }
            });
            ui.end_row();
        }
        match item.format {
            Format::Webm => {
                let options = &mut item.choices.webm;
                // as converting chooses it, and more would repeat the
                // input's frames
                let (average, base) = input_fps.unwrap_or_default();
                let chosen = average.map(|fps| (fps * tempo).clamp(1.0, telegram::MAX_FPS));
                let peak = match (average, base) {
                    (Some(average), Some(base)) => Some(average.max(base)),
                    (average, base) => average.or(base),
                };
                let most = peak.map(|fps| (fps * tempo).clamp(1.0, telegram::MAX_FPS));
                let automatic = match chosen {
                    Some(chosen) => format!("Input's: {}", fps_text(chosen)),
                    None => format!("Input's, at most {}", fps_text(telegram::MAX_FPS)),
                };
                label(
                    ui,
                    "Frame rate",
                    "Fewer frames a second leave more bytes for each; more than the input's would only repeat them",
                );
                let highest = most.unwrap_or(telegram::MAX_FPS);
                auto_number(ui, &mut options.fps, &automatic, highest.min(30.0), |value| {
                    value.range(1.0..=highest).speed(0.1).max_decimals(2).suffix(" fps")
                });
                ui.end_row();
                label(
                    ui,
                    "Over 3 seconds",
                    "Telegram refuses stickers that say they last longer than 3 seconds",
                );
                ui.vertical(|ui| {
                    preset_segments(ui, &mut options.spoof, Spoof::Auto, &SPOOFS);
                    let spoof = options.spoof.unwrap_or(Spoof::Auto);
                    chosen_hint(ui, spoof, &SPOOFS);
                    if let Some(played) = played
                        && spoof == Spoof::Auto
                        && played > telegram::MAX_SECONDS + 1e-3
                    {
                        ui.colored_label(
                            widgets::warn(ui),
                            format!(
                                "⚠ The result plays {}, so its duration will be spoofed",
                                widgets::seconds(played)
                            ),
                        );
                    }
                    if options.spoof != Some(Spoof::Never) {
                        ui.horizontal(|ui| {
                            ui.label("Spoof to:");
                            auto_number(ui, &mut options.fake_duration, "Default", 1.0, |value| {
                                value
                                    .range(0.01..=telegram::MAX_SECONDS)
                                    .speed(0.01)
                                    .max_decimals(5)
                                    .suffix(" s")
                            });
                        });
                    }
                });
                ui.end_row();
            }
            Format::Tgs if sequence => {
                label(ui, "Frame rate", "How fast the frames of a sequence or sprite sheet play");
                auto_number(ui, &mut item.choices.tgs.fps, "10 fps", tgs::DEFAULT_FPS, |value| {
                    value.range(0.1..=60.0).speed(0.1).max_decimals(2).suffix(" fps")
                });
                ui.end_row();
            }
            Format::Tgs => {}
        }
    });
}

const FITS: [Choice<Fit>; 6] = [
    (
        Fit::Auto,
        "Best looking",
        "Tries a few frame rates, fits the bitrate for each and keeps the one most like the input",
    ),
    (Fit::Bitrate, "Bitrate", "Fits the bitrate at the chosen frame rate"),
    (Fit::Crf, "Quality", "Fits the constant quality value (CRF)"),
    (Fit::Fps, "Frame rate", "Fits the frame rate at constant quality"),
    (Fit::Length, "Length", "Fits the length at constant quality, cutting the end"),
    (Fit::Off, "Off", "Encodes once with the values below, even if it comes out too large"),
];

const WEBM_SPEEDS: [Choice<Speed>; 3] = [
    (Speed::Fast, "Fast", "Seconds; visibly worse at the same size"),
    (Speed::Balanced, "Balanced", ""),
    (Speed::Best, "Best", "Slow: minutes for long videos"),
];

fn webm_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, "Size and quality");
    let base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.webm;
    let fit = options.fit.or(base.fit).unwrap_or(Fit::Auto);
    grid(ui, "webm-quality", |ui| {
        label(
            ui,
            "Getting under the limit",
            "What tgradish changes to land just under the size limit",
        );
        ui.vertical(|ui| {
            preset_segments(ui, &mut options.fit, base.fit.unwrap_or(Fit::Auto), &FITS);
            chosen_hint(ui, fit, &FITS);
        });
        ui.end_row();

        label(ui, "VP9 encoder", "Faster encoding looks worse at the same size");
        preset_segments(
            ui,
            &mut options.speed,
            base.speed.unwrap_or(Speed::Balanced),
            &WEBM_SPEEDS,
        );
        ui.end_row();

        if fit != Fit::Off {
            label(ui, "Encodes at most", "How many times fitting may encode");
            auto_number(ui, &mut options.attempts, "8", 8, |value| value.range(1..=50));
            ui.end_row();
            let unit = match fit {
                Fit::Auto | Fit::Bitrate => " kbit/s",
                Fit::Fps => " fps",
                Fit::Length => " s",
                Fit::Crf | Fit::Off => "",
            };
            label(ui, "Search within", "The range fitting searches, in the fitted value's unit");
            ui.horizontal(|ui| {
                let mut range = options.fit_range;
                if ui.radio(range.is_none(), "Automatic").clicked() {
                    range = None;
                }
                if ui.radio(range.is_some(), "").clicked() && range.is_none() {
                    range = Some(match fit {
                        Fit::Crf => Range { min: 4.0, max: 63.0 },
                        Fit::Fps => Range { min: 1.0, max: 30.0 },
                        Fit::Length => Range { min: 0.1, max: 3.0 },
                        _ => Range { min: 8.0, max: 2000.0 },
                    });
                }
                let mut shown = range.unwrap_or(Range { min: 0.0, max: 0.0 });
                let enabled = range.is_some();
                let low = ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut shown.min).speed(0.5).max_decimals(2),
                );
                ui.label("to");
                let high = ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut shown.max).speed(0.5).max_decimals(2).suffix(unit),
                );
                if low.changed() || high.changed() {
                    shown.max = shown.max.max(shown.min);
                    range = Some(shown);
                }
                options.fit_range = range;
            });
            ui.end_row();
        }

        if matches!(fit, Fit::Fps | Fit::Length | Fit::Off) {
            label(ui, "Quality (CRF)", "Constant quality from 0, the best, to 63");
            auto_number(ui, &mut options.crf, "32", 32, |value| value.range(0..=63));
            ui.end_row();
            label(ui, "Lossless", "");
            flag(
                ui,
                &mut options.lossless,
                false,
                "Lossless encoding",
                "Only small for tiny or still pictures",
            );
            ui.end_row();
        }
        if fit == Fit::Off {
            label(ui, "Bitrate", "Used unless only a quality is set");
            auto_number(ui, &mut options.bitrate, "From the size limit", 600.0, |value| {
                value.range(1.0..=50_000.0).speed(5.0).max_decimals(0).suffix(" kbit/s")
            });
            ui.end_row();
        }
    });
}

const TGS_SPEEDS: [Choice<Speed>; 3] = [
    (Speed::Fast, "Fast", "Under a second; a few percent larger"),
    (Speed::Balanced, "Balanced", ""),
    (Speed::Best, "Best", "A few seconds; the smallest"),
];

const REDUCTIONS: [(Reduction, &str, &str); 6] = [
    (Reduction::SnapToGrid, "Snap to the pixel grid", "Move stray pixels onto the art's grid"),
    (Reduction::MergeColours, "Merge similar colours", ""),
    (Reduction::Despeckle, "Remove specks", "Single pixels that differ from all around them"),
    (Reduction::MergeFrames, "Merge similar frames", "Frames that hardly differ show one picture"),
    (Reduction::DropFrames, "Drop frames", "Show every other frame, or fewer"),
    (Reduction::Downscale, "Lower the resolution", "Fewer, larger art pixels"),
];

const COMPROMISES: [Choice<tgs::Compromise>; 3] = [
    (tgs::Compromise::Auto, "Either", "Whatever changes the art least for the bytes it saves"),
    (
        tgs::Compromise::Motion,
        "Motion",
        "Frames merge and drop first: full detail, choppier motion",
    ),
    (tgs::Compromise::Detail, "Detail", "The picture coarsens first: smooth motion, less detail"),
];

fn tgs_quality(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    section(ui, "Size and quality");
    let base = item.choices.tgs_base(context.presets, context.config).unwrap_or_default();
    let options = &mut item.choices.tgs;
    grid(ui, "tgs-quality", |ui| {
        label(ui, "TGS encoder", "How hard to look for a small encoding");
        ui.vertical(|ui| {
            preset_segments(ui, &mut options.speed, base.speed.unwrap_or(Speed::Best), &TGS_SPEEDS);
            chosen_hint(ui, options.speed.or(base.speed).unwrap_or(Speed::Best), &TGS_SPEEDS);
        });
        ui.end_row();

        label(
            ui,
            "If output is too large",
            "What happens when the art doesn't fit into 64 KB as it is",
        );
        flag(
            ui,
            &mut options.lossless,
            base.lossless.unwrap_or(false),
            "Never change the art; report it instead",
            "",
        );
        ui.end_row();

        let lossless = options.lossless.or(base.lossless).unwrap_or(false);
        label(ui, "Give up first", "Art that is mostly motion can lose frames or detail");
        ui.add_enabled_ui(!lossless, |ui| {
            ui.vertical(|ui| {
                let base = base.compromise.unwrap_or_default();
                preset_segments(ui, &mut options.compromise, base, &COMPROMISES);
                chosen_hint(ui, options.compromise.unwrap_or(base), &COMPROMISES);
            });
        });
        ui.end_row();

        label(ui, "May change", "What fitting may give up to make it fit, least visible first");
        ui.add_enabled_ui(!lossless, |ui| {
            let all = base.reductions.clone().unwrap_or_else(|| Reduction::ALL.to_vec());
            let mut chosen = options.reductions.clone().unwrap_or_else(|| all.clone());
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (kind, text, hint) in REDUCTIONS {
                    let mut on = chosen.contains(&kind);
                    let response = ui.checkbox(&mut on, text);
                    let response =
                        if hint.is_empty() { response } else { response.on_hover_text(hint) };
                    if response.changed() {
                        if on {
                            chosen.push(kind);
                        } else {
                            chosen.retain(|&other| other != kind);
                        }
                        changed = true;
                    }
                }
                if options.reductions.is_some() && widgets::reset(ui) {
                    options.reductions = None;
                }
            });
            if changed {
                // in fitting's order
                chosen.sort_by_key(|kind| Reduction::ALL.iter().position(|other| other == kind));
                options.reductions = (chosen != all).then_some(chosen);
            }
        });
        ui.end_row();
    });
}

fn reading(ui: &mut egui::Ui, item: &mut Item) {
    let tags = item.art.ready().map(|art| art.tags.clone()).unwrap_or_default();
    let single = item.inputs.len() == 1 && !item.sequence;
    if tags.is_empty() && !single {
        return;
    }
    section(ui, "Reading the art");
    let options = &mut item.choices.tgs;
    grid(ui, "reading", |ui| {
        if !tags.is_empty() {
            label(ui, "Aseprite tag", "Only the frames of one tag, in its direction");
            ui.horizontal_wrapped(|ui| {
                if ui.radio(options.tag.is_none(), "All frames").clicked() {
                    options.tag = None;
                }
                for tag in &tags {
                    if ui.radio(options.tag.as_ref() == Some(tag), tag).clicked() {
                        options.tag = Some(tag.clone());
                    }
                }
            });
            ui.end_row();
        }
        if single && tags.is_empty() {
            label(ui, "Sprite sheet", "Frames laid out in a grid of equal cells, row by row");
            ui.horizontal_wrapped(|ui| {
                let mut sheet = options.sheet.is_some();
                if ui.checkbox(&mut sheet, "Read as a sprite sheet of").changed() {
                    options.sheet = sheet.then(|| "4x1".to_owned());
                }
                let (mut columns, mut rows) =
                    parse_sheet(options.sheet.as_deref()).unwrap_or((4, 1));
                let a = ui.add_enabled(sheet, egui::DragValue::new(&mut columns).range(1..=256));
                ui.label("×");
                let b = ui.add_enabled(sheet, egui::DragValue::new(&mut rows).range(1..=256));
                ui.label("cells");
                if sheet && (a.changed() || b.changed()) {
                    options.sheet = Some(format!("{columns}x{rows}"));
                }
                if sheet {
                    ui.label(", frames:");
                    auto_number(ui, &mut options.sheet_frames, "All", columns * rows, |value| {
                        value.range(1..=columns * rows)
                    });
                }
            });
            ui.end_row();
        }
    });
}

fn parse_sheet(sheet: Option<&str>) -> Option<(u32, u32)> {
    let (columns, rows) = sheet?.split_once('x')?;
    Some((columns.parse().ok()?, rows.parse().ok()?))
}

fn metadata(ui: &mut egui::Ui, item: &mut Item) {
    section(ui, "In the file");
    let title = match item.format {
        Format::Webm => &mut item.choices.webm.title,
        Format::Tgs => &mut item.choices.tgs.title,
    };
    grid(ui, "metadata", |ui| {
        label(ui, "Title", "A name stored in the file");
        let mut text = title.clone().unwrap_or_default();
        if ui
            .add(egui::TextEdit::singleline(&mut text).hint_text("none").desired_width(260.0))
            .changed()
        {
            *title = (!text.is_empty()).then_some(text);
        }
        ui.end_row();
    });
}

fn advanced(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new("Encoder options").strong()).id_salt("advanced").show(ui, |ui| {
        let options: &mut Options = &mut item.choices.webm;
        grid(ui, "advanced", |ui| {
            label(ui, "libvpx-vp9", "Encoder options by name, like tune-content = screen for flat graphics. ffmpeg -h encoder=libvpx-vp9 lists them");
            ui.vertical(|ui| {
                let mut pairs: Vec<(String, String)> = options.encoder_options.clone().unwrap_or_default().into_iter().collect();
                let mut changed = false;
                let mut remove = None;
                for (index, (name, value)) in pairs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(name).hint_text("name").desired_width(130.0)).changed();
                        ui.label("=");
                        changed |= ui.add(egui::TextEdit::singleline(value).hint_text("value").desired_width(100.0)).changed();
                        if ui.small_button("✖").on_hover_text("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    pairs.remove(index);
                    changed = true;
                }
                if ui.small_button("Add an option").clicked() {
                    pairs.push((String::new(), String::new()));
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    widgets::note(ui, "Examples:");
                    for (name, value, hint) in ENCODER_EXAMPLES {
                        if ui.small_button(format!("{name}={value}")).on_hover_text(hint).clicked() {
                            match pairs.iter_mut().find(|(other, _)| other == name) {
                                Some(pair) => pair.1 = value.to_owned(),
                                None => pairs.push((name.to_owned(), value.to_owned())),
                            }
                            changed = true;
                        }
                    }
                });
                if changed {
                    options.encoder_options = (!pairs.is_empty()).then(|| pairs.into_iter().collect());
                }
            });
            ui.end_row();

            label(ui, "ffmpeg arguments", "Raw arguments added before the output, split like a shell would");
            ui.vertical(|ui| {
                let mut text = options.extra_args.as_ref().map(|words| shlex::try_join(words.iter().map(String::as_str)).unwrap_or_default()).unwrap_or_default();
                let edit = ui.add_enabled(context.extra_args, egui::TextEdit::singleline(&mut text).hint_text("none").desired_width(300.0));
                if edit.changed() {
                    options.extra_args = shlex::split(&text).filter(|words| !words.is_empty());
                }
                if !context.extra_args {
                    note(ui, "Only with the system's ffmpeg, see Settings");
                }
            });
            ui.end_row();
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reshapes_crops_around_their_middle() {
        // about as large, within the picture
        let crop = Crop { x: 100, y: 100, width: 200, height: 100 };
        let size = (1000, 1000);
        assert_eq!(to_ratio(crop, 1.0, size), Crop { x: 129, y: 80, width: 141, height: 141 });
        assert_eq!(to_ratio(crop, 4.0, size), Crop { x: 59, y: 114, width: 283, height: 71 });
        let small = to_ratio(crop, 4.0, (320, 200));
        assert_eq!(small, Crop { x: 37, y: 114, width: 283, height: 71 });
        let tiny = to_ratio(crop, 4.0, (220, 120));
        assert_eq!(tiny, Crop { x: 0, y: 65, width: 220, height: 55 });
        assert_eq!(reduced(1920, 1080), (16.0, 9.0));
    }

    #[test]
    fn steps_through_whole_scales() {
        let scales: Vec<String> = (-3..=2).map(|step| scale_text(step_scale(step))).collect();
        assert_eq!(scales, ["×1/4", "×1/3", "×1/2", "×1", "×2", "×3"]);
        for text in ["3", "x3", "×3", " 3 "] {
            assert_eq!(parse_scale(text), Some(ExactScale { up: 3, down: 1 }), "{text}");
        }
        assert_eq!(parse_scale("1/3").map(scale_step), Some(-2));
        assert_eq!(parse_scale("0.5").map(scale_step), Some(-1));
        assert_eq!(parse_scale("2/3"), None);
        // as large as the result takes
        assert_eq!(largest_scale(Target::Sticker, (64, 40)), ExactScale { up: 8, down: 1 });
        assert_eq!(largest_scale(Target::Emoji, (64, 40)), ExactScale { up: 1, down: 1 });
        assert_eq!(largest_scale(Target::Sticker, (1920, 1080)), ExactScale { up: 1, down: 4 });
    }

    #[test]
    fn quotes_words_for_powershell() {
        assert_eq!(powershell_word("C:\\stickers\\pig.webm"), "C:\\stickers\\pig.webm");
        assert_eq!(powershell_word("my sticker"), "'my sticker'");
        assert_eq!(powershell_word("it's"), "'it''s'");
        assert_eq!(powershell_word("a,b"), "'a,b'");
        assert_eq!(powershell_word(""), "''");
    }

    #[test]
    fn orders_presets_by_speed() {
        let names: Vec<String> =
            preset_names(&Presets::builtin()).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["fast", "balanced", "best"]);
    }
}

//! The picture: the crop and the shape it keeps, and how the result is
//! squared and scaled. The preview works out the result's size from here
//! too, so what it shows is what converting makes.

use eframe::egui;
use tgradish_core::convert;
use tgradish_core::options::{Crop, ExactScale, Resize, Scaling};
use tgradish_core::presets::Format;
use tgradish_core::telegram::Target;

use super::Context;
use crate::canvas::{Exact, Keep};
use crate::item::{Aspect, Item};
use crate::widgets::{
    self, Choice, auto_number, chosen_hint, flag, note, preset_segments, row, segments,
};

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
pub(crate) fn ratio(aspect: Aspect, size: (u32, u32)) -> Option<f64> {
    match aspect {
        Aspect::Free => None,
        Aspect::Input => Some(f64::from(size.0) / f64::from(size.1.max(1))),
        Aspect::Ratio(width, height) => Some(width / height),
    }
}

/// `crop` made `ratio` times as wide as it is high, about as large and
/// around the same middle, within a `size` picture.
fn to_ratio(crop: Crop, ratio: f64, size: (u32, u32)) -> Crop {
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

/// How TGS stickers, which are always square, become so.
const TGS_SQUARES: [Choice<Resize>; 4] = [
    (Resize::Contain, "Off", ""),
    (Resize::Pad, "By padding", "Transparent margins fill the rest of the square"),
    (
        Resize::Crop,
        "By cropping",
        "The middle fills the square and the rest is cut off, by whole art pixels",
    ),
    (
        Resize::Stretch,
        "By stretching",
        "The art is stretched to the square: its pixels stop being square",
    ),
];

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
    let integer = item.format != Format::Tgs && webm.scaling == Some(Scaling::PixelPerfect);
    webm.exact_scale.filter(|_| integer)
}

/// Where a `crop` of the input lands in the item's WebM result; `None`
/// when its exact scale can't make it.
pub(crate) fn webm_sizes(item: &Item, crop: Crop) -> Option<convert::Sizes> {
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
pub(crate) fn resize_of(item: &Item) -> Resize {
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
pub(crate) fn exact_of(item: &Item) -> Option<Exact> {
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

pub(super) fn picture(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    widgets::section(ui, "Picture");
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
    let crop_hint = "The part of the input to use, in its pixels. Drag on the preview to change it";
    row(ui, "Crop", crop_hint, Ok(()), |ui| crop_box(ui, item, size, exact));

    let aspect_hint = "The width to height ratio the crop keeps while you drag it";
    row(ui, "Aspect ratio", aspect_hint, Ok(()), |ui| aspect_ratio(ui, item, size, exact));

    match item.format {
        Format::Webm | Format::Image => {
            let target = item.choices.target;
            let hint = "How the result becomes square: stickers needn't be, emoji must";
            row(ui, "Force into square", hint, Ok(()), |ui| {
                let base = if target.requires_exact_size() { Resize::Pad } else { Resize::Contain };
                let mut resize = resize_of(item);
                ui.horizontal(|ui| {
                    let allowed = |option| match option {
                        Resize::Contain if target.requires_exact_size() => {
                            Err("Emoji must be square")
                        }
                        Resize::Crop | Resize::Stretch if exact.is_some() => {
                            Err("A fixed integer scale keeps all of the picture: pad it instead")
                        }
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

            row(ui, "Scaling", "How the input's pixels become the result's", Ok(()), |ui| {
                preset_segments(ui, &mut item.choices.webm.scaling, Scaling::Auto, &SCALINGS);
                let scaling = item.choices.webm.scaling.unwrap_or(Scaling::Auto);
                chosen_hint(ui, scaling, &SCALINGS);
            });
            let integer = item.choices.webm.scaling == Some(Scaling::PixelPerfect);
            let allowed = if integer { Ok(()) } else { Err("Only with Integer scaling") };
            let hint = "How many result pixels each input pixel becomes, or how many input pixels make one";
            row(ui, "Integer scale", hint, allowed, |ui| integer_scale(ui, item, size, integer));
        }
        Format::Tgs => {
            let preset = item.choices.tgs_base(context.presets, context.config).ok();
            let hint = "How the art becomes square, as TGS stickers are";
            row(ui, "Force into square", hint, Ok(()), |ui| {
                let base = preset.as_ref().and_then(|base| base.resize).unwrap_or(Resize::Pad);
                let mut resize = item.choices.tgs.resize.unwrap_or(base);
                ui.horizontal(|ui| {
                    let allowed = |option| match option {
                        Resize::Contain => Err("TGS stickers are always square"),
                        _ => Ok(()),
                    };
                    if segments(ui, &mut resize, &TGS_SQUARES, allowed) {
                        item.choices.tgs.resize = (resize != base).then_some(resize);
                    }
                    if item.choices.tgs.resize.is_some() && widgets::reset(ui) {
                        item.choices.tgs.resize = None;
                    }
                });
                chosen_hint(ui, resize, &TGS_SQUARES);
            });
            let base = preset.and_then(|base| base.keep_canvas).unwrap_or(false);
            let hint = "Rows and columns that are transparent in every frame";
            row(ui, "Margins", hint, Ok(()), |ui| {
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
            // what the last conversion found, if it looked for it
            let detected = item
                .made
                .as_ref()
                .filter(|made| made.format == Format::Tgs && made.choices.tgs.pixel_scale.is_none())
                .and_then(|made| made.job.progress.report.as_ref())
                .map(|report| report.scale);
            let hint = "How many input pixels one pixel of the art takes. Pixels off its grid are moved onto it";
            row(ui, "Art pixel size", hint, Ok(()), |ui| {
                auto_number(
                    ui,
                    &mut item.choices.tgs.pixel_scale,
                    ("Detected", "Found from the art: its smallest blocks of one colour"),
                    detected,
                    2,
                    |value| value.range(1..=64).suffix(" px"),
                );
            });
        }
    }
}

/// Width of the crop's number fields.
const FIELD: f32 = 48.0;
/// Side of the crop's picture: with the fields above and below it, as
/// high as the fields beside them.
const SKETCH: f32 = 36.0;

/// A side of the crop that a field sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Top,
    Right,
    Bottom,
    Width,
    Height,
}

/// The crop as its four margins around a small picture of it, and its
/// width and height beside them: any of them can be typed, the others
/// follow, and the shape it keeps is kept.
fn crop_box(ui: &mut egui::Ui, item: &mut Item, size: Option<(u32, u32)>, exact: Option<Exact>) {
    let Some((width, height)) = size else {
        note(ui, "once the input is read");
        return;
    };
    let full = Crop { x: 0, y: 0, width, height };
    // one of another picture, as before the file changed, isn't kept
    let crop = item.choices.crop.filter(|crop| crop.fits(width, height)).unwrap_or(full);
    let mut edited = None;
    ui.horizontal_top(|ui| {
        let field = |ui: &mut egui::Ui, value: u32, most: u32, hint: &str| {
            let mut value = value;
            let response = ui
                .add_sized(
                    [FIELD, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut value).range(0..=most),
                )
                .on_hover_text(hint);
            response.changed().then_some(value)
        };
        let (right, bottom) = (width - crop.x - crop.width, height - crop.y - crop.height);
        egui::Grid::new(("crop", item.id)).spacing([4.0, 2.0]).show(ui, |ui| {
            ui.label("");
            ui.vertical_centered(|ui| {
                let most = height - bottom - 1;
                let hint = "Top margin: pixels cut off the top";
                edited = edited.or(field(ui, crop.y, most, hint).map(|v| (Side::Top, v)));
            });
            ui.end_row();
            // beside the picture, halfway down it
            let beside = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(egui::vec2(FIELD, SKETCH), beside, |ui| {
                let most = width - right - 1;
                let hint = "Left margin: pixels cut off the left";
                edited = edited.or(field(ui, crop.x, most, hint).map(|v| (Side::Left, v)));
            });
            sketch(ui, (width, height), crop);
            ui.allocate_ui_with_layout(egui::vec2(FIELD, SKETCH), beside, |ui| {
                let most = width - crop.x - 1;
                let hint = "Right margin: pixels cut off the right";
                edited = edited.or(field(ui, right, most, hint).map(|v| (Side::Right, v)));
            });
            ui.end_row();
            ui.label("");
            ui.vertical_centered(|ui| {
                let most = height - crop.y - 1;
                let hint = "Bottom margin: pixels cut off the bottom";
                edited = edited.or(field(ui, bottom, most, hint).map(|v| (Side::Bottom, v)));
            });
            ui.end_row();
        });
        ui.add_space(14.0);
        ui.vertical(|ui| {
            egui::Grid::new(("crop size", item.id)).spacing([6.0, 4.0]).show(ui, |ui| {
                ui.label("Width");
                let mut across = crop.width;
                let field = egui::DragValue::new(&mut across).range(1..=width).suffix(" px");
                if ui.add(field).changed() {
                    edited = Some((Side::Width, across));
                }
                ui.end_row();
                ui.label("Height");
                let mut down = crop.height;
                let field = egui::DragValue::new(&mut down).range(1..=height).suffix(" px");
                if ui.add(field).changed() {
                    edited = Some((Side::Height, down));
                }
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let cropped = item.choices.crop.is_some();
                let whole = ui.add_enabled(cropped, egui::Button::new("Whole picture"));
                let whole = whole.on_hover_text(if exact.is_some() {
                    "All of the picture, or as much as the exact scale takes from its top left"
                } else {
                    "All of the picture"
                });
                if whole.clicked() {
                    item.choices.crop = None;
                }
                // with an exact scale, as much of its middle as that takes
                if let Some(exact) = exact {
                    let middle = (Keep::Middle, Keep::Middle);
                    let ratio = ratio(item.view.aspect, (width, height));
                    let centred = exact
                        .fit(full, middle, ratio, (width, height))
                        .filter(|crop| *crop != full);
                    let button = ui
                        .add_enabled(item.choices.crop != centred, egui::Button::new("Middle"))
                        .on_hover_text("As much of the middle as the exact scale takes");
                    if button.clicked() {
                        item.choices.crop = centred;
                    }
                }
            });
        });
    });
    let Some((side, value)) = edited else { return };
    let ratio = ratio(item.view.aspect, (width, height));
    let mut crop = edit_crop(crop, side, value, ratio, (width, height));
    if let Some(exact) = exact {
        let start = (Keep::Start, Keep::Start);
        crop = exact.fit(crop, start, ratio, (width, height)).unwrap_or(crop);
    }
    item.choices.crop = (crop != full).then_some(crop);
}

/// `crop` with `side` set to `value`, in a `size` picture: a margin moves
/// its edge alone, a width or height the far edge, back from the picture's
/// where it would leave it; with a `ratio` kept, the other way follows,
/// from the top or the left.
fn edit_crop(crop: Crop, side: Side, value: u32, ratio: Option<f64>, size: (u32, u32)) -> Crop {
    let (width, height) = size;
    let (right, bottom) = (crop.x + crop.width, crop.y + crop.height);
    let mut crop = crop;
    match side {
        Side::Left => (crop.x, crop.width) = (value.min(right - 1), right - value.min(right - 1)),
        Side::Top => {
            (crop.y, crop.height) = (value.min(bottom - 1), bottom - value.min(bottom - 1))
        }
        Side::Right => crop.width = width.saturating_sub(value + crop.x).max(1),
        Side::Bottom => crop.height = height.saturating_sub(value + crop.y).max(1),
        Side::Width => {
            crop.width = value.clamp(1, width);
            crop.x = crop.x.min(width - crop.width);
        }
        Side::Height => {
            crop.height = value.clamp(1, height);
            crop.y = crop.y.min(height - crop.height);
        }
    }
    let Some(ratio) = ratio else { return crop };
    let across = matches!(side, Side::Left | Side::Right | Side::Width);
    if across {
        crop.height = (f64::from(crop.width) / ratio).round().max(1.0) as u32;
    } else {
        crop.width = (f64::from(crop.height) * ratio).round().max(1.0) as u32;
    }
    // too large for the picture: smaller, keeping its shape
    let shrink = (f64::from(width) / f64::from(crop.width))
        .min(f64::from(height) / f64::from(crop.height))
        .min(1.0);
    if shrink < 1.0 {
        crop.width = ((f64::from(crop.width) * shrink).round() as u32).clamp(1, width);
        crop.height = ((f64::from(crop.height) * shrink).round() as u32).clamp(1, height);
    }
    crop.x = crop.x.min(width - crop.width);
    crop.y = crop.y.min(height - crop.height);
    crop
}

/// A small picture of a `size` input with `crop` on it, dashed, around a
/// mountain.
fn sketch(ui: &mut egui::Ui, (width, height): (u32, u32), crop: Crop) {
    // as wide as the fields over and under it, so they line up
    let room = egui::vec2(FIELD, SKETCH);
    let (rect, _) = ui.allocate_exact_size(room, egui::Sense::hover());
    let visuals = ui.visuals();
    let (back, edge) = (visuals.extreme_bg_color, visuals.widgets.noninteractive.bg_stroke);
    let accent = crate::theme::accent(visuals);
    let weak = visuals.weak_text_color();
    let painter = ui.painter_at(rect);
    // the input at its shape, as large as fits, its shorter side at least
    // half the room's
    let fits = (room.x / width as f32).min(room.y / height as f32);
    let scale = fits.max(room.min_elem() / 2.0 / width.min(height) as f32);
    let picture = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(width as f32 * scale, height as f32 * scale).min(rect.size()),
    );
    let (across, down) = (picture.width() / width as f32, picture.height() / height as f32);
    painter.rect(picture, 2.0, back, edge, egui::StrokeKind::Inside);
    let cut = egui::Rect::from_min_size(
        picture.min + egui::vec2(crop.x as f32 * across, crop.y as f32 * down),
        egui::vec2(crop.width as f32 * across, crop.height as f32 * down),
    );
    painter.rect_filled(cut, 0.0, accent.gamma_multiply(0.15));
    let dashed = egui::Stroke::new(1.0, accent);
    for [from, to] in [
        [cut.left_top(), cut.right_top()],
        [cut.right_top(), cut.right_bottom()],
        [cut.right_bottom(), cut.left_bottom()],
        [cut.left_bottom(), cut.left_top()],
    ] {
        painter.extend(egui::Shape::dashed_line(&[from, to], dashed, 3.0, 3.0));
    }
    // two peaks in its middle
    let side = cut.width().min(cut.height()) * 0.6;
    let at = |x: f32, y: f32| cut.center() + egui::vec2(x * side, y * side);
    let peaks = vec![at(-0.5, 0.3), at(-0.15, -0.25), at(0.05, 0.05), at(0.25, -0.1), at(0.5, 0.3)];
    painter.with_clip_rect(cut).add(egui::Shape::line(peaks, egui::Stroke::new(1.5, weak)));
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
    // the ratio kept, or for the other shapes the crop's own, greyed out
    let custom = matches!(item.view.aspect, Aspect::Ratio(..));
    let (mut width, mut height) = match item.view.aspect {
        Aspect::Ratio(width, height) => (width, height),
        _ => crop.map_or((1.0, 1.0), |c| reduced(c.width, c.height)),
    };
    let mut typed = false;
    let pick = "Pick Custom to type a shape";
    ui.add_enabled_ui(custom, |ui| {
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
            let across = ui.add(side(&mut width)).on_hover_text("Width");
            typed |= across.on_disabled_hover_text(pick).changed();
            ui.label(":");
            let down = ui.add(side(&mut height)).on_hover_text("Height");
            typed |= down.on_disabled_hover_text(pick).changed();
            if item.format == Format::Tgs {
                return;
            }
            // the result's size, where the result takes the crop's shape:
            // not squared, nor with margins
            let sizes = crop.and_then(|crop| webm_sizes(item, crop));
            let follows = resize_of(item) == Resize::Contain && sizes.is_some_and(|s| !s.pads());
            let (box_w, box_h) = item.choices.target.box_size();
            let (mut result_w, mut result_h) =
                sizes.map_or((box_w, box_h), |s| (s.width, s.height));
            ui.add_space(14.0);
            ui.add_enabled_ui(follows, |ui| {
                let why =
                    "The result is squared or has margins, so its size doesn't follow the crop";
                ui.label("Result").on_disabled_hover_text(why);
                let across = ui.add(egui::DragValue::new(&mut result_w).range(1..=box_w));
                ui.label("×");
                let down =
                    ui.add(egui::DragValue::new(&mut result_h).range(1..=box_h).suffix(" px"));
                if follows && (across.changed() || down.changed()) {
                    (width, height) = reduced(result_w, result_h);
                    typed = true;
                }
            });
        });
    });
    if typed && custom {
        item.view.aspect = Aspect::Ratio(width, height);
        keep_aspect(item, size, exact);
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
/// your own, with what it makes. Without integer scaling, only the
/// controls, greyed out.
fn integer_scale(ui: &mut egui::Ui, item: &mut Item, size: Option<(u32, u32)>, integer: bool) {
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
        let mut fixed = scale.is_some();
        let choices = [
            (
                false,
                "Largest that fits",
                "The largest whole multiple that keeps the crop within the result",
            ),
            (true, "Fixed", "This scale; the crop can be at most what it fits into the result"),
        ];
        if segments(ui, &mut fixed, &choices, |_| Ok(())) {
            chosen = fixed.then_some(guess);
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
                    .custom_parser(|text| parse_scale(text).map(|s| scale_step(s) as f64))
                    // a scale kept for integer scaling shows as it is
                    .clamp_existing_to_range(integer),
            )
            .on_hover_text("Type 3 for three times as large, or 1/3 for a third")
            .on_disabled_hover_text("Pick Fixed to choose a scale");
            if ui.small_button("+").on_hover_text("Larger").clicked() {
                step += 1;
            }
            let step = step.clamp(lowest, highest);
            if scale.is_some() && step != current {
                chosen = Some(step_scale(step));
            }
        });
    });
    // greyed out, the scale is only shown, and kept for integer scaling
    if integer && chosen != scale {
        item.choices.webm.exact_scale = chosen;
        return;
    }
    if !integer {
        note(ui, "Whole scales apply with Integer scaling only");
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

#[cfg(test)]
mod tests {
    use tgradish_core::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::telegram;

    #[test]
    fn edits_the_crop_from_any_side() {
        let crop = Crop { x: 10, y: 20, width: 30, height: 40 };
        let size = (100, 80);
        let edit = |side, value| edit_crop(crop, side, value, None, size);
        // a margin moves its edge alone
        assert_eq!(edit(Side::Left, 5), Crop { x: 5, width: 35, ..crop });
        assert_eq!(edit(Side::Right, 50), Crop { width: 40, ..crop });
        assert_eq!(edit(Side::Top, 30), Crop { y: 30, height: 30, ..crop });
        assert_eq!(edit(Side::Bottom, 0), Crop { height: 60, ..crop });
        // a width the far edge, back from the picture's where it would leave
        assert_eq!(edit(Side::Width, 50), Crop { width: 50, ..crop });
        assert_eq!(edit(Side::Width, 95), Crop { x: 5, width: 95, ..crop });
        // a shape kept: the other way follows, smaller where it won't fit
        let square = |side, value| edit_crop(crop, side, value, Some(1.0), size);
        assert_eq!(square(Side::Width, 50), Crop { x: 10, y: 20, width: 50, height: 50 });
        assert_eq!(square(Side::Height, 70), Crop { x: 10, y: 10, width: 70, height: 70 });
        assert_eq!(square(Side::Width, 100), Crop { x: 0, y: 0, width: 80, height: 80 });
    }

    use super::*;
    use crate::testing::{finish, harness, video};

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
    fn keeps_a_fixed_scale_while_integer_scaling_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "64x48") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        // more than an emoji takes, kept for when integer scaling is back
        let scale = Some("200".parse().unwrap());
        let item = &mut harness.state_mut().items[0];
        item.choices.webm.exact_scale = scale;
        item.choices.webm.scaling = Some(Scaling::Smooth);
        item.choices.target = telegram::Target::Emoji;
        harness.run_steps(3);
        assert_eq!(harness.state().items[0].choices.webm.exact_scale, scale);
    }
}

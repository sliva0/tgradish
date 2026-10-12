//! Small pieces the window is built from.

use eframe::egui::{self, Color32, RichText, vec2};

use crate::jobs::Problem;
use crate::theme;

/// Colours that say how things are: Breeze's, the same in either theme.
pub const GOOD: Color32 = theme::POSITIVE;
pub const WARN: Color32 = theme::NEUTRAL;
pub const BAD: Color32 = theme::NEGATIVE;

/// The background pictures are shown on.
pub fn stage(dark: bool) -> Color32 {
    theme::stage(dark)
}

pub fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

/// Seconds as `1:02.50` or `2.50 s`.
pub fn seconds(value: f64) -> String {
    if value >= 60.0 {
        format!("{}:{:05.2}", (value / 60.0).floor(), value % 60.0)
    } else {
        format!("{value:.2} s")
    }
}

/// A heading over a group of settings, apart from the group before.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(14.0);
    let (line, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        line.x_range(),
        line.center().y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().size(15.5));
    ui.add_space(4.0);
}

/// How wide `text` is in buttons' font, plus `padding`.
pub fn text_width(ui: &egui::Ui, text: &str, padding: f32) -> f32 {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    galley.size().x + padding
}

/// Weaker text explaining something.
/// A link to `url`, which takes a click as a link, rather than starting a
/// selection as text does.
pub fn link(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
    url: impl ToString,
) -> egui::Response {
    let selectable = ui.style().interaction.selectable_labels;
    ui.style_mut().interaction.selectable_labels = false;
    let response = ui.add(egui::Hyperlink::from_label_and_url(text, url));
    ui.style_mut().interaction.selectable_labels = selectable;
    response
}

/// `n` and what it counts, which takes an "s" for any number but one.
pub fn count(n: impl std::fmt::Display, what: &str) -> String {
    let n = n.to_string();
    format!("{n} {what}{}", if n == "1" { "" } else { "s" })
}

pub fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).weak().size(12.5));
}

/// The label column of a grid, explained on hover.
pub fn label(ui: &mut egui::Ui, text: &str, hint: &str) {
    let label = ui.label(text);
    if !hint.is_empty() {
        label.on_hover_text(hint);
    }
}

/// Width of the labels before settings.
pub const LABELS: f32 = 150.0;

/// A setting: its label, explained on hover, and its controls beside it,
/// both from the top. Unless `allowed`, all of it is greyed out, and the
/// label's hover says why.
pub fn row<R>(
    ui: &mut egui::Ui,
    text: &str,
    hint: &str,
    allowed: Result<(), &str>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.add_enabled_ui(allowed.is_ok(), |ui| {
        ui.horizontal_top(|ui| {
            // as high as the first line of controls, the text in its middle
            let line = ui.spacing().interact_size.y;
            let layout = egui::Layout::top_down(egui::Align::Min);
            let label = ui
                .allocate_ui_with_layout(vec2(LABELS, line), layout, |ui| {
                    ui.set_width(LABELS);
                    let text_height = ui.text_style_height(&egui::TextStyle::Body);
                    ui.add_space(((line - text_height) / 2.0).floor().max(0.0));
                    ui.add(egui::Label::new(text).wrap())
                })
                .inner;
            match allowed {
                Err(why) => {
                    label.on_disabled_hover_text(why);
                }
                Ok(()) if !hint.is_empty() => {
                    label.on_hover_text(hint);
                }
                Ok(()) => {}
            }
            ui.vertical(contents).inner
        })
        .inner
    })
    .inner
}

/// One choice of a [`segments`] row: its value, label and explanation.
pub type Choice<'a, T> = (T, &'a str, &'a str);

/// Options side by side as one control, the chosen one lit. `enabled`
/// says which can be picked, with why not. True when changed.
pub fn segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    enabled: impl Fn(T) -> Result<(), &'static str>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let last = choices.len().saturating_sub(1);
        let radius = ui.visuals().widgets.inactive.corner_radius.nw;
        let height = ui.spacing().interact_size.y;
        for (index, &(option, text, hint)) in choices.iter().enumerate() {
            let round = |at_end: bool| if at_end { radius } else { 0 };
            let corners = egui::CornerRadius {
                nw: round(index == 0),
                sw: round(index == 0),
                ne: round(index == last),
                se: round(index == last),
            };
            let selected = *value == option;
            let mut button = egui::Button::selectable(selected, text)
                .corner_radius(corners)
                .min_size(vec2(0.0, height));
            button = if selected {
                button.stroke(theme::on_outline(ui.visuals()))
            } else {
                button.frame_when_inactive(true)
            };
            let allowed = enabled(option);
            let response = ui.add_enabled(allowed.is_ok(), button);
            let response = match allowed {
                Err(why) => response.on_disabled_hover_text(why),
                Ok(()) if !hint.is_empty() => response.on_hover_text(hint),
                Ok(()) => response,
            };
            if response.clicked() && !selected {
                *value = option;
                changed = true;
            }
        }
    });
    changed
}

/// [`segments`] for a value a preset may set, like [`choice`].
pub fn preset_segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    base: T,
    choices: &[Choice<T>],
) -> bool {
    let mut current = value.unwrap_or(base);
    let mut changed = false;
    ui.horizontal(|ui| {
        if segments(ui, &mut current, choices, |_| Ok(())) {
            *value = (current != base).then_some(current);
            changed = true;
        }
        if value.is_some() && reset(ui) {
            *value = None;
            changed = true;
        }
    });
    changed
}

/// The explanation of the chosen option, under a row of them.
pub fn chosen_hint<T: PartialEq + Copy>(ui: &mut egui::Ui, value: T, choices: &[Choice<T>]) {
    if let Some(&(_, _, hint)) = choices.iter().find(|(option, ..)| *option == value)
        && !hint.is_empty()
    {
        note(ui, hint);
    }
}

/// The small button that goes back to the preset's value.
pub fn reset(ui: &mut egui::Ui) -> bool {
    ui.add(egui::Button::new(RichText::new("⟲").size(13.0)).small().frame(false))
        .on_hover_text("Back to the preset's value")
        .clicked()
}

/// A checkbox for a flag a preset may set.
pub fn flag(
    ui: &mut egui::Ui,
    value: &mut Option<bool>,
    base: bool,
    text: &str,
    hint: &str,
) -> bool {
    let mut checked = value.unwrap_or(base);
    let mut changed = false;
    ui.horizontal(|ui| {
        let response = ui.checkbox(&mut checked, text);
        let response = if hint.is_empty() { response } else { response.on_hover_text(hint) };
        if response.changed() {
            *value = (checked != base).then_some(checked);
            changed = true;
        }
        if value.is_some() && reset(ui) {
            *value = None;
            changed = true;
        }
    });
    changed
}

/// A number that is either automatic (`None`) or set: the two as
/// segments, `automatic` and Custom, and the number beside them, greyed
/// out while automatic. It shows `shown` then, the automatic value if it
/// is known, and a newly set number starts from it, or from `start`.
pub fn auto_number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    automatic: &str,
    shown: Option<T>,
    start: T,
    edit: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut custom = value.is_some();
        let choices = [(false, automatic, ""), (true, "Custom", "")];
        if segments(ui, &mut custom, &choices, |_| Ok(())) {
            *value = custom.then(|| shown.unwrap_or(start));
            changed = true;
        }
        let set = value.is_some();
        // greyed out here or around: shown as it is, even outside the range
        // a set value keeps to, and never stored, as egui would clamp it
        // and say it changed
        let live = set && ui.is_enabled();
        let mut number = value.or(shown).unwrap_or(start);
        let mut field = edit(egui::DragValue::new(&mut number)).clamp_existing_to_range(live);
        if !set && shown.is_none() {
            field = field.custom_formatter(|_, _| "—".to_owned());
        }
        let response = ui.add_enabled(set, field).on_disabled_hover_text("Pick Custom to set it");
        if live && response.changed() {
            *value = Some(number);
            changed = true;
        }
    });
    changed
}

/// How much of a limit something uses: a bar, red when over.
pub fn gauge(ui: &mut egui::Ui, used: u64, limit: u64, text: String) {
    let share = used as f32 / limit.max(1) as f32;
    let colour = if share > 1.0 { BAD } else { GOOD };
    bar(ui, Fill::Share(share), &text, colour.gamma_multiply(0.55));
}

/// How far a [`bar`] got.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    /// This share of it, done.
    Share(f32),
    /// This share, still at work: a band moves over it, so a share that
    /// takes a while to change doesn't look stuck.
    Working(f32),
    /// At work, how far along unknown: only the band.
    Unknown,
}

impl Fill {
    /// At work: this share, or unknown without one.
    pub fn working(share: Option<f32>) -> Fill {
        share.map_or(Fill::Unknown, Fill::Working)
    }
}

/// A bar filled as `fill` says, `text` over it.
pub fn bar(ui: &mut egui::Ui, fill: Fill, text: &str, colour: Color32) {
    let width = ui.available_width().min(380.0);
    bar_sized(ui, egui::vec2(width, 22.0), fill, text, colour);
}

/// A [`bar`] of `size`.
pub fn bar_sized(ui: &mut egui::Ui, size: egui::Vec2, fill: Fill, text: &str, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rounding = (size.y / 2.0).min(4.0);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, rounding, ui.visuals().extreme_bg_color);
    // the whole bar's shape, cut to how far it got: no blob at its start
    // when it is nearly empty
    if let Fill::Share(share) | Fill::Working(share) = fill {
        let width = rect.width() * share.clamp(0.0, 1.0);
        let part = egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height()));
        ui.painter_at(part).rect_filled(rect, rounding, colour);
    }
    if !matches!(fill, Fill::Share(_)) {
        let band = rect.width() * 0.3;
        let time = ui.input(|input| input.time) as f32;
        let left = rect.left() - band + (time * 0.7).fract() * (rect.width() + band);
        let part = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(left + band, rect.bottom()),
        );
        // over a share, a sheen that shows on what is filled and what isn't
        let shade = match fill {
            Fill::Unknown => colour,
            _ => ui.visuals().strong_text_color().gamma_multiply(0.14),
        };
        ui.painter_at(part.intersect(rect)).rect_filled(rect, rounding, shade);
        ui.ctx().request_repaint();
    }
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.5),
        ui.visuals().strong_text_color(),
    );
}

/// Telegram's problems with a result, or that it has none.
pub fn problems(ui: &mut egui::Ui, problems: &[Problem]) {
    if problems.is_empty() {
        ui.colored_label(GOOD, "✔ Telegram should accept it");
    }
    for problem in problems {
        let (colour, mark) = if problem.refused { (BAD, "✖") } else { (WARN, "⚠") };
        ui.colored_label(colour, format!("{mark} {}", problem.text));
    }
}

/// Draws a checkerboard, so transparency shows.
pub fn checkerboard(painter: &egui::Painter, rect: egui::Rect, square: f32) {
    let (light, dark) = if painter.ctx().global_style().visuals.dark_mode {
        (Color32::from_gray(92), Color32::from_gray(72))
    } else {
        (Color32::from_gray(240), Color32::from_gray(212))
    };
    painter.rect_filled(rect, 0.0, light);
    let clip = painter.clip_rect().intersect(rect);
    if clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    // only the squares in view
    let first_column = ((clip.left() - rect.left()) / square).floor().max(0.0) as i32;
    let first_row = ((clip.top() - rect.top()) / square).floor().max(0.0) as i32;
    let columns = ((clip.right() - rect.left()) / square).ceil() as i32;
    let rows = ((clip.bottom() - rect.top()) / square).ceil() as i32;
    for row in first_row..rows {
        let start = first_column + (row + first_column) % 2;
        for column in (start..columns).step_by(2) {
            let min = rect.min + egui::vec2(column as f32 * square, row as f32 * square);
            let cell = egui::Rect::from_min_size(min, egui::vec2(square, square)).intersect(rect);
            painter.rect_filled(cell, 0.0, dark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_one_without_an_s() {
        assert_eq!((count(1, "frame"), count(2, "frame")), ("1 frame".into(), "2 frames".into()));
        assert_eq!(
            (count(1.0, "frame"), count(0usize, "layer")),
            ("1 frame".into(), "0 layers".into())
        );
    }

    #[test]
    fn takes_clicks_on_links_rather_than_selecting() {
        type State = Option<egui::Rect>;
        let show = |ui: &mut egui::Ui, at: &mut State| {
            ui.style_mut().interaction.selectable_labels = true;
            *at = Some(link(ui, "source code", "https://example.org").rect);
        };
        let mut harness = egui_kittest::Harness::new_ui_state(show, None);
        harness.run();
        let at = harness.state().unwrap().center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(at));
        harness.event(press(true));
        harness.step();
        assert_eq!(harness.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand);
        harness.event(press(false));
        harness.step();
        let opened = &harness.output().platform_output.commands;
        assert!(matches!(opened.as_slice(), [egui::OutputCommand::OpenUrl(_)]), "{opened:?}");
    }

    #[test]
    fn keeps_automatic_numbers_automatic() {
        // an automatic value outside the range a set one keeps to, like the
        // speed that fits a minute into 3 seconds
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, value: &mut Option<f64>| {
                auto_number(ui, value, "Automatic", Some(20.0), 1.0, |field| {
                    field.range(0.1..=10.0)
                });
            },
            None,
        );
        harness.run();
        assert_eq!(*harness.state(), None);
        // a set value, greyed out with all around it, outside the range
        // the row has now
        let mut greyed = egui_kittest::Harness::new_ui_state(
            |ui, value: &mut Option<u32>| {
                ui.add_enabled_ui(false, |ui| {
                    auto_number(ui, value, "All", Some(4), 4, |field| field.range(1..=4));
                });
            },
            Some(20),
        );
        greyed.run();
        assert_eq!(*greyed.state(), Some(20));
        // shown as it is, not clamped to the range
        use egui_kittest::kittest::Queryable;
        harness.get(egui_kittest::kittest::by().value("20"));
    }

    #[test]
    fn writes_times() {
        assert_eq!(seconds(2.5), "2.50 s");
        assert_eq!(seconds(62.5), "1:02.50");
    }
}

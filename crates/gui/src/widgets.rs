//! Small pieces the window is built from.

use eframe::egui::{self, Color32, RichText};

use crate::jobs::Problem;

pub const GOOD: Color32 = Color32::from_rgb(96, 186, 112);
pub const WARN: Color32 = Color32::from_rgb(226, 170, 72);
pub const BAD: Color32 = Color32::from_rgb(232, 96, 88);

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

/// Weaker text explaining something.
pub fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).weak().size(12.5));
}

/// The label column of a settings grid, explained on hover.
pub fn label(ui: &mut egui::Ui, text: &str, hint: &str) {
    let label = ui.label(text);
    if !hint.is_empty() {
        label.on_hover_text(hint);
    }
}

/// One choice of a [`choice`] row: its value, label and explanation.
pub type Choice<'a, T> = (T, &'a str, &'a str);

/// Radio buttons for a value with no preset behind it.
pub fn radios<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for &(option, text, hint) in choices {
            let response = ui.radio(*value == option, text);
            let response = if hint.is_empty() { response } else { response.on_hover_text(hint) };
            if response.clicked() && *value != option {
                *value = option;
                changed = true;
            }
        }
    });
    changed
}

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
        for (index, &(option, text, hint)) in choices.iter().enumerate() {
            let round = |at_end: bool| if at_end { 5 } else { 0 };
            let corners = egui::CornerRadius {
                nw: round(index == 0),
                sw: round(index == 0),
                ne: round(index == last),
                se: round(index == last),
            };
            let selected = *value == option;
            let mut button = egui::Button::selectable(selected, text)
                .corner_radius(corners)
                .min_size(egui::vec2(0.0, 26.0));
            if !selected {
                button =
                    button.frame_when_inactive(true).fill(ui.visuals().widgets.inactive.bg_fill);
            }
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

/// A number that is either automatic (`None`) or set: a radio for each,
/// the number editable when set. `fallback` is where a newly set number
/// starts.
pub fn auto_number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    automatic: &str,
    fallback: T,
    edit: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        if ui.radio(value.is_none(), automatic).clicked() && value.is_some() {
            *value = None;
            changed = true;
        }
        if ui.radio(value.is_some(), "").clicked() && value.is_none() {
            *value = Some(fallback);
            changed = true;
        }
        let mut number = value.unwrap_or(fallback);
        let response = ui.add_enabled(value.is_some(), edit(egui::DragValue::new(&mut number)));
        if response.changed() {
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
    bar(ui, Some(share), &text, colour.gamma_multiply(0.55));
}

/// A bar filled to `fraction`, or with a band moving through it while how
/// far along is unknown, and `text` over it.
pub fn bar(ui: &mut egui::Ui, fraction: Option<f32>, text: &str, colour: Color32) {
    let width = ui.available_width().min(380.0);
    bar_sized(ui, egui::vec2(width, 22.0), fraction, text, colour);
}

/// A [`bar`] of `size`.
pub fn bar_sized(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fraction: Option<f32>,
    text: &str,
    colour: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rounding = (size.y / 2.0).min(4.0);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, rounding, ui.visuals().extreme_bg_color);
    // the whole bar's shape, cut to how far it got: no blob at its start
    // when it is nearly empty
    let part = match fraction {
        Some(fraction) => egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
        ),
        None => {
            let band = rect.width() * 0.3;
            let time = ui.input(|input| input.time) as f32;
            let left = rect.left() - band + (time * 0.7).fract() * (rect.width() + band);
            ui.ctx().request_repaint();
            egui::Rect::from_min_max(
                egui::pos2(left, rect.top()),
                egui::pos2(left + band, rect.bottom()),
            )
        }
    };
    ui.painter_at(part.intersect(rect)).rect_filled(rect, rounding, colour);
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.5),
        Color32::WHITE,
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
    let (light, dark) = (Color32::from_gray(92), Color32::from_gray(72));
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
    fn writes_times() {
        assert_eq!(seconds(2.5), "2.50 s");
        assert_eq!(seconds(62.5), "1:02.50");
    }
}

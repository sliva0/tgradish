//! Segmented choices and choice buttons, and the pickers, flags and reset
//! buttons for settings that can override a preset.

use eframe::egui::{self, RichText, vec2};

use super::layout::{hint, named};
use super::text::text_width;
use crate::l10n::fl;
use crate::theme;

/// A single option in a [`segments`] row: `(value, label, hover_hint)`.
pub(crate) type Choice<T> = (T, String, String);

/// Availability of an option in [`segments`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pick {
    Yes,
    /// Disabled, with a tooltip explaining why.
    No(String),
    /// Dimmed because another setting conflicts with it, but still clickable to
    /// switch that setting (explained by the tooltip string).
    Changes(String),
}

/// Draws a segmented button control where `enabled` returns `Ok(())` or an
/// explanation for disabled choices. Returns `true` when the selection changes.
pub(crate) fn segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    enabled: impl Fn(T) -> Result<(), String>,
) -> bool {
    picking(ui, value, choices, |option| match enabled(option) {
        Ok(()) => Pick::Yes,
        Err(why) => Pick::No(why),
    })
}

/// Like [`segments`], taking a [`Pick`] state per option.
pub(crate) fn picking<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
) -> bool {
    let room = ui.available_width();
    picking_in(ui, value, choices, pick, room, true)
}

/// Draws [`segments`] on a single line of width `room` without wrapping,
/// truncating labels that do not fit and showing the full label on hover.
pub(crate) fn segments_on_line<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    enabled: impl Fn(T) -> Result<(), String>,
    room: f32,
) -> bool {
    let pick = |option| match enabled(option) {
        Ok(()) => Pick::Yes,
        Err(why) => Pick::No(why),
    };
    picking_in(ui, value, choices, pick, room, false)
}

/// Horizontal button padding used when segments are tight on space before
/// wrapping or truncating.
const TIGHT_PADDING: f32 = 4.0;

/// Lays out segments of `widths` separated by `gap` within `room`: returns the
/// index range for each line and the maximum width any segment on that line may
/// take (truncating the widest segments first when a line overflows `room`).
fn flow(widths: &[f32], gap: f32, room: f32, wrap: bool) -> Vec<(std::ops::Range<usize>, f32)> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut width = 0.0;
    for (index, each) in widths.iter().enumerate() {
        let wider = width + if index > start { gap } else { 0.0 } + each;
        if wrap && index > start && wider > room {
            lines.push(start..index);
            (start, width) = (index, *each);
        } else {
            width = wider;
        }
    }
    lines.push(start..widths.len());
    lines
        .into_iter()
        .map(|line| {
            // Trim the widest segments first by only as much as needed.
            let gaps = gap * line.len().saturating_sub(1) as f32;
            let mut sorted: Vec<f32> = widths[line.clone()].to_vec();
            sorted.sort_by(f32::total_cmp);
            let mut most = f32::INFINITY;
            let mut left = room - gaps;
            for (index, each) in sorted.iter().enumerate() {
                let share = left / (sorted.len() - index) as f32;
                if *each > share {
                    most = share.floor().max(1.0);
                    break;
                }
                left -= each;
            }
            (line, most)
        })
        .collect()
}

/// Draws [`picking`] within `room`, reducing padding first, then wrapping across
/// lines if `wrap` is true, and finally truncating labels if still too wide.
fn picking_in<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
    room: f32,
    wrap: bool,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        let gap = 1.0;
        ui.spacing_mut().item_spacing = vec2(gap, 4.0);
        let widths = |ui: &egui::Ui, padding: f32| -> Vec<f32> {
            choices.iter().map(|(_, text, _)| text_width(ui, text, 2.0 * padding)).collect()
        };
        let total = |widths: &[f32]| widths.iter().sum::<f32>() + gap * (widths.len() - 1) as f32;
        let mut natural = widths(ui, ui.spacing().button_padding.x);
        if total(&natural) > room {
            let tight = widths(ui, TIGHT_PADDING);
            if total(&tight) <= room || !wrap {
                ui.spacing_mut().button_padding.x = TIGHT_PADDING;
                natural = tight;
            }
        }
        let radius = ui.visuals().widgets.inactive.corner_radius.nw;
        for (line, most) in flow(&natural, gap, room, wrap) {
            ui.horizontal(|ui| {
                for index in line.clone() {
                    let (option, text, hint) = &choices[index];
                    let option = *option;
                    let round = |at_end: bool| if at_end { radius } else { 0 };
                    let (first, last) = (index == line.start, index + 1 == line.end);
                    let corners = egui::CornerRadius {
                        nw: round(first),
                        sw: round(first),
                        ne: round(last),
                        se: round(last),
                    };
                    let selected = *value == option;
                    let button = egui::Button::new(text.as_str()).truncate().corner_radius(corners);
                    let button = as_choice(ui, button, selected);
                    let cut = natural[index] > most;
                    let response = ui
                        .scope(|ui| {
                            if most.is_finite() {
                                ui.set_max_width(most);
                            }
                            match pick(option) {
                                Pick::No(why) => {
                                    ui.add_enabled(false, button).on_disabled_hover_text(why)
                                }
                                Pick::Changes(what) => {
                                    greyed(ui, true, |ui| ui.add(button)).on_hover_text(what)
                                }
                                Pick::Yes if !hint.is_empty() => {
                                    ui.add(button).on_hover_text(hint.as_str())
                                }
                                Pick::Yes => ui.add(button),
                            }
                        })
                        .inner;
                    // Show the full label on hover when truncated.
                    let response =
                        if cut { response.on_hover_text(text.as_str()) } else { response };
                    if response.clicked() && !selected {
                        *value = option;
                        changed = true;
                    }
                }
            });
        }
    });
    changed
}

/// Styles `button` as a choice button (used by [`segments`] and standalone
/// choices): framed at control height, and lit and outlined with the accent
/// when `selected`. Every choice is drawn so.
pub(crate) fn as_choice<'a>(
    ui: &egui::Ui,
    button: egui::Button<'a>,
    selected: bool,
) -> egui::Button<'a> {
    let height = ui.spacing().interact_size.y;
    let button =
        button.selected(selected).frame(true).frame_when_inactive(true).min_size(vec2(0.0, height));
    if selected { button.stroke(theme::on_outline(ui.visuals())) } else { button }
}

/// Draws a standalone choice button styled like a [`segments`] option inside a
/// `horizontal_wrapped` row. If `text` exceeds a full line, it moves to its own
/// line and wraps across multiple lines, truncating (with a full hover tooltip)
/// only if a single word exceeds the line width.
pub(crate) fn choice(
    ui: &mut egui::Ui,
    selected: bool,
    text: &str,
    enabled: bool,
) -> egui::Response {
    let button = as_choice(ui, egui::Button::new(text), selected);
    let line = ui.max_rect().width();
    let padding = 2.0 * ui.spacing().button_padding.x;
    if text_width(ui, text, padding) <= line {
        return ui.add_enabled(enabled, button.wrap_mode(egui::TextWrapMode::Extend));
    }
    // Start a line of its own.
    if ui.cursor().min.x > ui.max_rect().min.x + 0.5 {
        ui.end_row();
    }
    let longest = text.split_whitespace().map(|word| text_width(ui, word, padding));
    let cut = longest.fold(0.0, f32::max) > line;
    let button = if cut { button.truncate() } else { button.wrap() };
    let response = ui
        .scope(|ui| {
            ui.set_max_width(line);
            ui.add_enabled(enabled, button)
        })
        .inner;
    if cut { response.on_hover_text(text) } else { response }
}

/// Draws [`segments`] for a setting that can override a preset default `base`.
pub(crate) fn preset_segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    base: T,
    choices: &[Choice<T>],
) -> bool {
    preset_picking(ui, value, base, choices, |_| Pick::Yes)
}

/// Like [`preset_segments`], taking a [`Pick`] state per option.
pub(crate) fn preset_picking<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    base: T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
) -> bool {
    let mut current = value.unwrap_or(base);
    let mut changed = false;
    ui.horizontal(|ui| {
        let room = ui.available_width() - RESET - ui.spacing().item_spacing.x;
        if picking_in(ui, &mut current, choices, pick, room, true) {
            *value = (current != base).then_some(current);
            changed = true;
        }
        if reset(ui, value.is_some()) {
            *value = None;
            changed = true;
        }
    });
    changed
}

/// Draws `contents` with weak inactive text colour when `greyed` is true while
/// keeping it interactive (for options that conflict with another setting but
/// switch it when clicked; hovering shows normal active contrast).
pub(crate) fn greyed<R>(
    ui: &mut egui::Ui,
    greyed: bool,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        if greyed {
            let weak = ui.visuals().weak_text_color();
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.fg_stroke.color = weak;
            widgets.noninteractive.fg_stroke.color = weak;
        }
        contents(ui)
    })
    .inner
}

/// Draws the hint caption for the currently selected option in `choices`.
pub(crate) fn chosen_hint<T: PartialEq + Copy>(ui: &mut egui::Ui, value: T, choices: &[Choice<T>]) {
    if let Some((_, _, hint)) = choices.iter().find(|(option, ..)| *option == value)
        && !hint.is_empty()
    {
        self::hint(ui, hint.as_str());
    }
}

/// Width and height of the slot reserved for a [`reset`] button.
const RESET: f32 = 20.0;

/// Draws a small reset-to-preset button when `shown` is true.
pub(crate) fn reset(ui: &mut egui::Ui, shown: bool) -> bool {
    reset_to(ui, shown, &fl!("back-to-preset"))
}

/// Draws a frameless accent-coloured reset button with tooltip `hint` when
/// `shown` is true, or reserves its space when hidden so adjacent widgets do
/// not shift.
pub(crate) fn reset_to(ui: &mut egui::Ui, shown: bool, hint: &str) -> bool {
    let size = vec2(RESET, RESET);
    if !shown {
        ui.allocate_exact_size(size, egui::Sense::hover());
        return false;
    }
    let glyph = RichText::new("⟲").size(13.0).color(theme::accent_text(ui.visuals()));
    let button = ui.add(egui::Button::new(glyph).small().frame(false).min_size(size));
    named(button, hint).on_hover_text(hint).clicked()
}

/// Draws a checkbox for a boolean setting that can override a preset default
/// `base`, paired with a reset button.
pub(crate) fn flag(
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
        if reset(ui, value.is_some()) {
            *value = None;
            changed = true;
        }
    });
    changed
}

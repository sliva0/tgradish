//! Shared GUI widgets and formatting helpers, in a submodule for each kind;
//! the rest of the GUI reaches them through the re-exports here.

mod choice;
mod colour;
mod dialog;
mod format;
mod layout;
mod number;
mod paint;
mod progress;
mod scroll;
mod text;

pub(crate) use choice::{
    Choice, Pick, as_choice, choice, chosen_hint, flag, greyed, picking, preset_picking,
    preset_segments, reset, reset_to, segments, segments_on_line,
};
pub(crate) use colour::{colour_picker, swatch};
#[cfg(test)]
pub(crate) use dialog::DIALOG_MARGIN;
pub(crate) use dialog::{dialog, dialog_button, dialog_buttons, dialog_title, title_text};
pub(crate) use format::{
    FPS_DECIMALS, SPEED_DECIMALS, fixed, fps, kib, parse_seconds, seconds, speed, trimmed,
    trimmed_to, widest_seconds, zeros,
};
pub(crate) use layout::{
    HEADING, LABELS, TITLE, attention, attention_explained, folding_section, hint, hint_in_room,
    hint_room, label, link, medium, named, note, primary, problems, row, section, section_rule,
    section_with, settings_spacing, steady_hint, text_field,
};
pub(crate) use number::{
    Number, acted_now, auto_number, escape_taken, finish_typed_number, forget_typed_number,
    from_preset, number_field, number_taken_now, of_file, of_file_in, preset_number, stepper,
    take_typed_number, typed_number, typing_number,
};
pub(crate) use paint::{cased_outline, checkerboard, checkerboard_from, hatch, tick};
pub(crate) use progress::{Fill, bar, bar_sized, bar_spent, gauge, steady_bar, thin_bar};
#[cfg(test)]
pub(crate) use scroll::SCROLL_BAR;
pub(crate) use scroll::{SCROLL_FADE, scrolled};
pub(crate) use text::{fitting, steady_numbers, steady_width, text_width};

/// Installs the GUI's fonts into a bare test `harness`. egui's default fonts
/// are disabled in Cargo features, and without glyphs a text cursor always
/// stays at index 0 so typed characters would be inserted backwards.
#[cfg(test)]
pub(crate) fn test_fonts<State>(harness: &egui_kittest::Harness<'_, State>) {
    harness.ctx.set_fonts(crate::fonts::fonts());
}

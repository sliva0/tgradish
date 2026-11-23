//! Modal dialogs: the fading frame, the title row with its close button,
//! the body between them that scrolls where the window is too short for
//! it, and the bottom button row.
//!
//! A dialog fits its window at any UI scale: its width is held to the
//! window's, its body is made short enough for the rest of it to fit, the
//! [`super::row`]s in it put their label over their controls where they
//! are narrow, and its buttons go on in another row where they can't all
//! go side by side.

use eframe::egui::{self, Color32, RichText, vec2};

use super::layout::{TITLE, medium, named};
use crate::l10n::fl;
use crate::theme;

/// Inner margin inside a dialog frame.
pub(crate) const DIALOG_MARGIN: i8 = 16;
/// Minimum margin kept between a dialog and the window edges when the window or
/// UI scale makes the viewport narrower or shorter than the dialog.
const DIALOG_AROUND: f32 = 8.0;
/// Least height a dialog's body is given, however short the window: a
/// line of it, to scroll through.
const LEAST_BODY: f32 = theme::CONTROL;
/// Tag on the [`egui::UiStack`] of a dialog's contents, by which what is
/// drawn in a dialog knows it is.
const IN_DIALOG: &str = "tgradish dialog";

/// Dialog fade-in duration in seconds. The dialog is placed at full size and
/// accepts keyboard input from its first frame.
const DIALOG_FADE: f64 = 0.11;
/// Maximum alpha of the dimmed backdrop behind a dialog.
const BACKDROP: f32 = 100.0;

/// Creates a modal dialog with the given `id`, framed with [`dialog_title`] at
/// the top and [`dialog_buttons`] at the bottom.
pub(crate) fn dialog(_ctx: &egui::Context, id: &str) -> Dialog {
    let id = egui::Id::new(id);
    Dialog { id, area: egui::Modal::default_area(id), width: None }
}

/// Modal dialog wrapper around [`egui::Modal`] whose dialog and dimmed
/// backdrop behind it fade in faster than egui's default easing.
pub(crate) struct Dialog {
    id: egui::Id,
    area: egui::Area,
    width: Option<f32>,
}

impl Dialog {
    /// Overrides the modal [`egui::Area`] instead of centring in the window.
    pub(crate) fn area(self, area: egui::Area) -> Dialog {
        Dialog { area, ..self }
    }

    /// Sets the content width to `width`, clamped to the available window width
    /// so text wraps and the right-aligned close button stays on screen.
    pub(crate) fn width(self, width: f32) -> Dialog {
        Dialog { width: Some(width), ..self }
    }

    pub(crate) fn show<R>(
        self,
        ctx: &egui::Context,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::ModalResponse<R> {
        let opacity = opening(ctx, self.id);
        let backdrop = Color32::from_black_alpha((BACKDROP * opacity).round() as u8);
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(DIALOG_MARGIN);
        egui::Modal::new(self.id)
            .area(self.area.fade_in(false))
            .frame(egui::Frame::NONE)
            .backdrop_color(backdrop)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                let shown = frame.show(ui, |ui| {
                    if let Some(width) = self.width {
                        let margins = 2.0 * (f32::from(DIALOG_MARGIN) + DIALOG_AROUND);
                        let room = ctx.content_rect().width() - margins;
                        ui.set_width(width.min(room).max(0.0));
                    }
                    let tagged = egui::UiStackInfo::default().with_tag(IN_DIALOG);
                    ui.scope_builder(egui::UiBuilder::new().ui_stack_info(tagged), content).inner
                });
                let drawn = shown.response.rect.height();
                ui.data_mut(|data| data.insert_temp(self.id.with(DRAWN), drawn));
                shown.inner
            })
    }
}

/// Whether `ui` is in a dialog.
pub(crate) fn in_dialog(ui: &egui::Ui) -> bool {
    ui.stack().iter().any(|frame| frame.tags().contains(IN_DIALOG))
}

/// Keys under the dialog's own id of how tall it was last drawn, and its
/// body. Its area's own size is no use, as egui holds it to the window's.
const DRAWN: &str = "drawn";
const BODY: &str = "body";

/// The most height the body of the dialog `id` may take for all of the
/// dialog to fit the window, [`DIALOG_AROUND`] from its edges: the
/// window's height less what the dialog drew around its body when last
/// shown (its title, what it says over and under its body, and its
/// buttons), and at least [`LEAST_BODY`].
pub(crate) fn dialog_room(ctx: &egui::Context, id: egui::Id) -> f32 {
    let window = ctx.content_rect().height() - 2.0 * DIALOG_AROUND;
    let (drawn, body): (Option<f32>, Option<f32>) =
        ctx.data(|data| (data.get_temp(id.with(DRAWN)), data.get_temp(id.with(BODY))));
    // not yet drawn, the dialog is sized unseen first
    let around = drawn.zip(body).map_or(0.0, |(drawn, body)| (drawn - body).max(0.0));
    (window - around).max(LEAST_BODY)
}

/// Draws the body of a dialog, between its title and its buttons, in
/// `area`, no taller than [`dialog_room`] so the dialog fits the window
/// and what scrolls is the body.
pub(crate) fn dialog_body<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    dialog_body_within(ui, area, f32::INFINITY, add)
}

/// [`dialog_body`] no taller than `most` either.
pub(crate) fn dialog_body_within<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    most: f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    // the dialog's area is the layer it is drawn on
    let id = ui.layer_id().id;
    let room = dialog_room(ui.ctx(), id).min(most);
    // egui's own least height of what scrolls, 64, would be taller than
    // the room in a short window
    let area = area.max_height(room).min_scrolled_height(room);
    let top = ui.cursor().top();
    let shown = super::scrolled(ui, area, add);
    // as tall as it was laid out, which is less than its `inner_rect`
    // where it is shorter than the room it had
    let body = ui.min_rect().bottom() - top;
    ui.data_mut(|data| data.insert_temp(id.with(BODY), body));
    shown
}

/// Returns the fade-in progress (`0.0..=1.0`) for dialog `id`, restarting from
/// `0.0` whenever the dialog was not shown on the previous pass.
fn opening(ctx: &egui::Context, id: egui::Id) -> f32 {
    let (key, pass) = (id.with("opened"), ctx.cumulative_pass_nr());
    let now = ctx.input(|input| input.time);
    let shown: Option<(f64, u64)> = ctx.data(|data| data.get_temp(key));
    let since = match shown {
        Some((since, last)) if last + 1 >= pass => since,
        _ => now,
    };
    ctx.data_mut(|data| data.insert_temp(key, (since, pass)));
    crate::motion::along(ctx, since, DIALOG_FADE)
}

/// Draws a truncated dialog title with a right-aligned close button, returning
/// `true` when the close button is clicked.
pub(crate) fn dialog_title(ui: &mut egui::Ui, title: impl Into<egui::WidgetText>) -> bool {
    let title = title.into();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let side = ui.spacing().interact_size.y;
            let cross = egui::Button::new("✖").frame(false).min_size(vec2(side, side));
            let closed = named(ui.add(cross), &fl!("close")).on_hover_text(fl!("close")).clicked();
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(title).truncate());
            });
            closed
        })
        .inner
    })
    .inner
}

/// Formats dialog title text in the heading weight.
pub(crate) fn title_text(ui: &egui::Ui, text: impl Into<String>) -> RichText {
    medium(ui, text, TITLE)
}

/// Draws a dialog's bottom button row below a separator line. `add` places
/// buttons right-to-left (dismiss button first, then primary, then others,
/// and [`dialog_button_apart`] at the left end of the row). Those that
/// don't fit beside the ones before, as in a narrow window, go on in
/// another row under them.
pub(crate) fn dialog_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    // Start at one control height, as wrapping rows do; otherwise a
    // right-to-left layout consumes all remaining vertical space and
    // prevents the dialog from shrinking when the window or scale changes.
    let row = vec2(ui.available_width(), theme::CONTROL);
    let layout = egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true);
    ui.allocate_ui_with_layout(row, layout, add).inner
}

/// Creates a dialog action button with a uniform minimum width.
pub(crate) fn dialog_button<'a>(text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text).min_size(vec2(DIALOG_BUTTON, theme::CONTROL))
}

/// Least width of a [`dialog_button`].
const DIALOG_BUTTON: f32 = 90.0;

/// Adds a [`dialog_button`] of `text` last in [`dialog_buttons`], apart
/// from the rest at the left end of the row, or of a row of its own under
/// them where it doesn't fit beside them: one that doesn't close the
/// dialog.
pub(crate) fn dialog_button_apart(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let padding = 2.0 * ui.spacing().button_padding.x;
    let wide = super::text_width(ui, text, padding).max(DIALOG_BUTTON);
    if ui.available_size_before_wrap().x < wide {
        ui.end_row();
    }
    // a point short of the end, lest rounding take it past
    ui.add_space((ui.available_size_before_wrap().x - wide - 1.0).max(0.0));
    ui.add(dialog_button(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_a_dialog_in_where_it_goes_from_its_first_frame() {
        // Record `(opacity, label_rect)` on each frame while the dialog is open.
        type State = (bool, Vec<(f32, egui::Rect)>);
        let show = |ui: &mut egui::Ui, (open, drawn): &mut State| {
            if *open {
                let modal = dialog(ui.ctx(), "test").show(ui.ctx(), |ui| {
                    ui.set_width(200.0);
                    (ui.opacity(), ui.label("Inside").rect)
                });
                drawn.push(modal.inner);
            }
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(60)
            .build_ui_state(show, (false, Vec::new()));
        for motion in [true, false] {
            crate::motion::set(&harness.ctx, motion);
            harness.state_mut().1.clear();
            harness.state_mut().0 = true;
            harness.run_steps(10);
            let drawn = &harness.state().1;
            let (opacities, places): (Vec<f32>, Vec<egui::Rect>) = drawn.iter().copied().unzip();
            if motion {
                // Starts at 0, reaches ~halfway after 3 frames, and finishes in 0.11 s.
                assert!(opacities[0] < 0.01 && opacities[3] > 0.3 && opacities[3] < 0.7);
                assert!(opacities[7..].iter().all(|&opacity| opacity == 1.0), "{opacities:?}");
            } else {
                assert!(opacities.iter().all(|&opacity| opacity == 1.0), "{opacities:?}");
            }
            // Remains at its final position from the first visible frame onwards.
            assert!(places[1..].iter().all(|&place| place == places[9]), "{places:?}");
            harness.try_run().expect("still once shown");
            harness.state_mut().0 = false;
            harness.run_steps(2);
        }
    }
}

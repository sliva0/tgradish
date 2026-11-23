//! Modal dialogs: the fading frame, the title row with its close button,
//! and the bottom button row.

use eframe::egui::{self, Color32, RichText, vec2};

use super::layout::{TITLE, medium, named};
use crate::l10n::fl;
use crate::theme;

/// Inner margin inside a dialog frame.
pub(crate) const DIALOG_MARGIN: i8 = 16;
/// Minimum margin kept between a dialog and the window edges when the window or
/// UI scale makes the viewport narrower than the dialog's target width.
const DIALOG_AROUND: f32 = 8.0;

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
                frame
                    .show(ui, |ui| {
                        if let Some(width) = self.width {
                            let margins = 2.0 * (f32::from(DIALOG_MARGIN) + DIALOG_AROUND);
                            let room = ctx.content_rect().width() - margins;
                            ui.set_width(width.min(room).max(0.0));
                        }
                        content(ui)
                    })
                    .inner
            })
    }
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
/// buttons right-to-left (dismiss button first, then primary, then others; any
/// left-to-right sub-layout inside `add` sits at the left end of the row).
pub(crate) fn dialog_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    // Fix the row height to one control height; otherwise a right-to-left
    // layout consumes all remaining vertical space and prevents the dialog
    // from shrinking when the window or scale changes.
    let row = vec2(ui.available_width(), theme::CONTROL);
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    ui.allocate_ui_with_layout(row, layout, add).inner
}

/// Creates a dialog action button with a uniform minimum width.
pub(crate) fn dialog_button<'a>(text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text).min_size(vec2(90.0, theme::CONTROL))
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

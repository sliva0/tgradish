//! Colour swatches and the saturation, value and hue colour picker.

use eframe::egui::{self, Color32, vec2};

use crate::l10n::fl;
use crate::theme;

/// Radius of a [`swatch`] circle and its selection/hover ring.
const SWATCH: f32 = 10.0;
const SWATCH_RING: f32 = 12.5;

/// Draws a circular colour swatch inside a control-sized square: either a solid
/// `colour` or, when `None`, a hue wheel for custom colour selection. When
/// `chosen`, draws a contrasting black or white checkmark and an outer ring in
/// the text colour; announces `name` on hover and to screen readers.
pub(crate) fn swatch(
    ui: &mut egui::Ui,
    colour: Option<Color32>,
    chosen: bool,
    name: &str,
) -> egui::Response {
    let side = theme::CONTROL;
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), egui::Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, chosen, name)
    });
    if !ui.is_rect_visible(rect) {
        return response.on_hover_text(name);
    }
    let visuals = ui.visuals();
    let painter = ui.painter();
    let centre = rect.center();
    match colour {
        Some(colour) => {
            painter.circle_filled(centre, SWATCH, colour);
        }
        None => {
            // White at the centre, full hue circle around the rim.
            let mut wheel = egui::Mesh::default();
            wheel.colored_vertex(centre, Color32::WHITE);
            let steps = 48;
            for step in 0..=steps {
                let turn = step as f32 / steps as f32;
                let angle = turn * std::f32::consts::TAU;
                let at = centre + SWATCH * vec2(angle.cos(), angle.sin());
                wheel.colored_vertex(at, egui::ecolor::Hsva::new(turn, 1.0, 1.0, 1.0).into());
                if step > 0 {
                    wheel.add_triangle(0, step, step + 1);
                }
            }
            painter.add(wheel);
        }
    }
    // Outline the circle so it remains visible even when matching the background.
    painter.circle_stroke(centre, SWATCH, visuals.widgets.inactive.bg_stroke);
    let text = visuals.text_color();
    if chosen {
        let fill = colour.unwrap_or(Color32::WHITE);
        let tick = [Color32::WHITE, Color32::BLACK]
            .into_iter()
            .max_by(|a, b| theme::contrast(*a, fill).total_cmp(&theme::contrast(*b, fill)))
            .unwrap_or(Color32::BLACK);
        let points = [vec2(-4.5, 0.0), vec2(-1.5, 3.0), vec2(4.5, -3.5)].map(|at| centre + at);
        painter.add(egui::Shape::line(points.to_vec(), egui::Stroke::new(2.0, tick)));
        // Skip the text-colour ring when keyboard focus already draws an accent ring.
        if !response.has_focus() {
            painter.circle_stroke(centre, SWATCH_RING, egui::Stroke::new(1.5, text));
        }
    } else if response.hovered() && enabled {
        painter.circle_stroke(centre, SWATCH_RING, egui::Stroke::new(1.0, text));
    }
    response.on_hover_text(name)
}

/// Fractional step across a [`colour_picker`] per arrow key press (and with
/// Shift held).
const PICKER_STEP: f32 = 0.01;
const PICKER_LEAP: f32 = 0.1;

/// Returns the `(x, y)` adjustment requested this frame on a [`colour_picker`]
/// control via Left/Right (and Up/Down where `upright`, for the square) while
/// focused (locking focus against arrow navigation), or via screen-reader
/// increment/decrement actions (lighter/darker on the square, hue on/back on
/// the strip). Each step is [`PICKER_STEP`], or [`PICKER_LEAP`] with Shift.
fn picker_steps(ui: &egui::Ui, response: &egui::Response, upright: bool) -> egui::Vec2 {
    use egui::Key;
    use egui::accesskit::Action;
    let focused = response.has_focus();
    if focused {
        let filter = egui::EventFilter {
            horizontal_arrows: true,
            vertical_arrows: upright,
            ..Default::default()
        };
        // egui only applies focus lock starting on the frame after focus is
        // gained, so request an immediate repaint before a key press can move
        // focus away.
        if !ui.memory(|memory| memory.had_focus_last_frame(response.id)) {
            ui.ctx().request_repaint();
        }
        ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, filter));
    }
    ui.input(|input| {
        let presses = |key| if focused { input.num_presses(key) as f32 } else { 0.0 };
        let asked = |action| input.num_accesskit_action_requests(response.id, action) as f32;
        let asked = asked(Action::Increment) - asked(Action::Decrement);
        let across = presses(Key::ArrowRight) - presses(Key::ArrowLeft);
        let (across, up) = match upright {
            true => (across, presses(Key::ArrowUp) - presses(Key::ArrowDown) + asked),
            false => (across + asked, 0.0),
        };
        let step = if input.modifiers.shift { PICKER_LEAP } else { PICKER_STEP };
        vec2(across, up) * step
    })
}

/// Draws an opaque colour picker consisting of a 2D saturation/value square
/// (more saturated to the right and lighter to the top) and a hue strip below
/// it, operable by pointer or arrow keys. Returns `true` when `colour` changes.
pub(crate) fn colour_picker(ui: &mut egui::Ui, colour: &mut Color32) -> bool {
    use egui::ecolor::HsvaGamma;
    // Preserve the chosen hue while the colour is pure grey (which has no hue).
    let id = ui.id().with("colour-picker");
    let kept = ui.data(|data| data.get_temp::<(Color32, HsvaGamma)>(id));
    let mut hsv = match kept {
        Some((kept, hsv)) if kept == *colour => hsv,
        _ => HsvaGamma::from(*colour),
    };
    let width = 240.0;
    let enabled = ui.is_enabled();
    let outline = ui.visuals().widgets.noninteractive.bg_stroke;
    let area = |ui: &mut egui::Ui, height: f32| {
        let sense = egui::Sense::click_and_drag();
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        let at = response.interact_pointer_pos().map(|at| {
            let at = (at - rect.min) / rect.size();
            vec2(at.x.clamp(0.0, 1.0), at.y.clamp(0.0, 1.0))
        });
        (rect, response, at)
    };
    let (square, square_response, picked) = area(ui, 150.0);
    if let Some(at) = picked {
        (hsv.s, hsv.v) = (at.x, 1.0 - at.y);
    }
    let steps = picker_steps(ui, &square_response, true);
    hsv.s = (hsv.s + steps.x).clamp(0.0, 1.0);
    hsv.v = (hsv.v + steps.y).clamp(0.0, 1.0);
    let (strip, strip_response, picked) = area(ui, 16.0);
    if let Some(at) = picked {
        hsv.h = at.x;
    }
    hsv.h = (hsv.h + picker_steps(ui, &strip_response, false).x).clamp(0.0, 1.0);
    // Expose both controls to screen readers as sliders with current values.
    let percent = |value: f32| (value * 100.0).round();
    square_response.widget_info(|| egui::WidgetInfo {
        current_text_value: Some(fl!(
            "accent-shade",
            "value",
            saturation = percent(hsv.s),
            value = percent(hsv.v)
        )),
        ..egui::WidgetInfo::slider(enabled, f64::from(percent(hsv.v)), fl!("accent-shade"))
    });
    let hue = f64::from((hsv.h * 360.0).round());
    strip_response.widget_info(|| egui::WidgetInfo::slider(enabled, hue, fl!("accent-hue")));
    // Configure AccessKit numeric ranges and increment/decrement actions.
    for (response, value, most) in
        [(&square_response, hsv.v, 100.0), (&strip_response, hsv.h, 360.0)]
    {
        ui.ctx().accesskit_node_builder(response.id, |node| {
            use egui::accesskit::Action;
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(most);
            if value < 1.0 {
                node.add_action(Action::Increment);
            }
            if value > 0.0 {
                node.add_action(Action::Decrement);
            }
        });
    }
    // Render each area as a vertex-coloured triangle mesh.
    let grid = |rect: egui::Rect, steps: (usize, usize), at: &dyn Fn(f32, f32) -> Color32| {
        let mut mesh = egui::Mesh::default();
        for row in 0..=steps.1 {
            for column in 0..=steps.0 {
                let (x, y) = (column as f32 / steps.0 as f32, row as f32 / steps.1 as f32);
                mesh.colored_vertex(rect.min + rect.size() * vec2(x, y), at(x, y));
                if row > 0 && column > 0 {
                    let index = |row: usize, column: usize| (row * (steps.0 + 1) + column) as u32;
                    let (a, b) = (index(row - 1, column - 1), index(row - 1, column));
                    let (c, d) = (index(row, column - 1), index(row, column));
                    mesh.add_triangle(a, b, c);
                    mesh.add_triangle(b, d, c);
                }
            }
        }
        mesh
    };
    let painter = ui.painter();
    let shade = |s, v| Color32::from(HsvaGamma { h: hsv.h, s, v, a: 1.0 });
    painter.add(grid(square, (16, 16), &|x, y| shade(x, 1.0 - y)));
    painter.add(grid(strip, (36, 1), &|x, _| HsvaGamma { h: x, s: 1.0, v: 1.0, a: 1.0 }.into()));
    // Draw the keyboard focus outline outside the rect because the interior
    // colours may match the accent colour.
    let focus = egui::Stroke::new(2.0, theme::mark(ui.visuals()));
    for (rect, response) in [(square, &square_response), (strip, &strip_response)] {
        let stroke = if response.has_focus() { focus } else { outline };
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
    }
    // Mark the selected position with concentric black and white rings so it is
    // visible against any colour.
    let ring = |at: egui::Pos2, radius: f32| {
        painter.circle_stroke(at, radius + 1.0, egui::Stroke::new(1.0, Color32::BLACK));
        painter.circle_stroke(at, radius, egui::Stroke::new(1.0, Color32::WHITE));
    };
    ring(square.min + square.size() * vec2(hsv.s, 1.0 - hsv.v), 5.0);
    let x = strip.left() + strip.width() * hsv.h;
    painter.rect_stroke(
        egui::Rect::from_center_size(
            egui::pos2(x, strip.center().y),
            vec2(5.0, strip.height() + 4.0),
        ),
        1.0,
        egui::Stroke::new(1.0, Color32::BLACK),
        egui::StrokeKind::Middle,
    );
    painter.rect_stroke(
        egui::Rect::from_center_size(
            egui::pos2(x, strip.center().y),
            vec2(3.0, strip.height() + 2.0),
        ),
        1.0,
        egui::Stroke::new(1.0, Color32::WHITE),
        egui::StrokeKind::Middle,
    );
    let picked = Color32::from(hsv);
    ui.data_mut(|data| data.insert_temp(id, (picked, hsv)));
    let changed = picked != *colour;
    *colour = picked;
    changed
}

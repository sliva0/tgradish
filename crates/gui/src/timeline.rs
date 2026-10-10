//! The bar under the preview: where playback is, and which part of the
//! input is used, with handles and numbers to change it.

use eframe::egui::{self, CursorIcon, PointerButton, Rect, Sense, Shape, Stroke, pos2, vec2};

use crate::widgets;

/// What the timeline changes.
pub struct Timeline<'a> {
    /// Length of the input, in seconds.
    pub length: f64,
    pub start: &'a mut Option<f64>,
    /// Length used from the start; the rest of the input when `None`.
    pub used: &'a mut Option<f64>,
    pub time: &'a mut f64,
    pub playing: &'a mut bool,
    /// Telegram's length limit, marked from the start.
    pub limit: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Start,
    End,
    Seek,
}

/// The used part as `(start, end)`, and as the item stores it.
fn part(length: f64, start: Option<f64>, used: Option<f64>) -> (f64, f64) {
    let from = start.unwrap_or(0.0).clamp(0.0, length);
    (from, used.map_or(length, |used| (from + used).min(length)))
}

fn set_part(length: f64, (from, to): (f64, f64), start: &mut Option<f64>, used: &mut Option<f64>) {
    *start = (from > 1e-3).then_some(from);
    *used = (to < length - 1e-3).then_some(to - from);
}

/// Shows the timeline.
pub fn show(ui: &mut egui::Ui, timeline: Timeline) {
    let Timeline { length, start, used, time, playing, limit } = timeline;
    ui.horizontal(|ui| {
        play_button(ui, playing);
        let (mut from, to) = part(length, *start, *used);
        fn seconds(value: &mut f64, length: f64) -> egui::DragValue<'_> {
            egui::DragValue::new(value).range(0.0..=length).speed(0.01).max_decimals(2).suffix(" s")
        }
        let field = ui
            .add_sized(vec2(64.0, 24.0), seconds(&mut from, length))
            .on_hover_text("Where the used part starts");
        if field.changed() {
            from = from.min(to - 0.01).max(0.0);
            set_part(length, (from, to), start, used);
            *time = from;
        }
        let end_width = 64.0 + 8.0;
        let width = (ui.available_width() - end_width).max(80.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());
        bar(ui, rect, &response, length, (start, used), time, limit);
        let (from, mut to) = part(length, *start, *used);
        let field = ui
            .add_sized(vec2(64.0, 24.0), seconds(&mut to, length))
            .on_hover_text("Where the used part ends");
        if field.changed() {
            to = to.max(from + 0.01).min(length);
            set_part(length, (from, to), start, used);
        }
    });
}

fn bar(
    ui: &mut egui::Ui,
    rect: Rect,
    response: &egui::Response,
    length: f64,
    (start, used): (&mut Option<f64>, &mut Option<f64>),
    time: &mut f64,
    limit: f64,
) {
    let length = length.max(1e-6);
    let track = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top() + 9.0),
        pos2(rect.right() - 8.0, rect.bottom() - 7.0),
    );
    let x_of = |seconds: f64| track.left() + (seconds / length) as f32 * track.width();
    let seconds_at =
        |x: f32| (f64::from((x - track.left()) / track.width()) * length).clamp(0.0, length);
    let (from, to) = part(length, *start, *used);

    let grab_at = |x: f32| {
        if (x - x_of(from)).abs() <= 8.0 {
            Grab::Start
        } else if (x - x_of(to)).abs() <= 8.0 {
            Grab::End
        } else {
            Grab::Seek
        }
    };
    if let Some(pointer) = response.hover_pos() {
        if grab_at(pointer.x) != Grab::Seek {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        response.clone().on_hover_text_at_pointer(widgets::seconds(seconds_at(pointer.x)));
    }
    let id = response.id.with("grab");
    if response.drag_started_by(PointerButton::Primary)
        && let Some(origin) = ui.input(|input| input.pointer.press_origin())
    {
        ui.data_mut(|data| data.insert_temp(id, grab_at(origin.x)));
    }
    let grabbed: Option<Grab> = ui.data(|data| data.get_temp(id));
    let pointer = response.interact_pointer_pos();
    if let (Some(grab), Some(pointer)) = (grabbed, pointer)
        && response.dragged_by(PointerButton::Primary)
    {
        let at = seconds_at(pointer.x);
        match grab {
            Grab::Start => {
                let at = at.min(to - 0.05).max(0.0);
                set_part(length, (at, to), start, used);
                *time = at;
            }
            Grab::End => {
                let at = at.max(from + 0.05).min(length);
                set_part(length, (from, at), start, used);
                *time = (at - 0.5).max(from);
            }
            Grab::Seek => *time = at,
        }
    }
    if response.clicked()
        && let Some(pointer) = pointer
    {
        *time = seconds_at(pointer.x);
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<Grab>(id));
    }

    let (from, to) = part(length, *start, *used);
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let accent = visuals.selection.bg_fill;
    painter.rect_filled(track, 3.0, accent.gamma_multiply(0.55));
    // the unused parts, dimmed
    painter.rect_filled(
        Rect::from_min_max(track.min, pos2(x_of(from), track.max.y)),
        3.0,
        visuals.extreme_bg_color,
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(x_of(to), track.min.y), track.max),
        3.0,
        visuals.extreme_bg_color,
    );
    // Telegram plays 3 seconds; longer stickers are spoofed, sped up or cut
    if to - from > limit + 1e-3 {
        let x = x_of(from + limit);
        painter.line_segment(
            [pos2(x, track.top()), pos2(x, track.bottom())],
            Stroke::new(1.5, widgets::warn(ui)),
        );
        painter.text(
            pos2(x + 3.0, rect.top()),
            egui::Align2::LEFT_TOP,
            "3 s",
            egui::FontId::proportional(10.5),
            widgets::warn(ui),
        );
    }
    // brackets at both ends of the used part
    let marks = ui.visuals().strong_text_color();
    let bracket = Stroke::new(2.5, marks);
    for (x, inward) in [(x_of(from), 4.0), (x_of(to), -4.0)] {
        let (top, bottom) = (track.top() - 3.0, track.bottom() + 3.0);
        painter.add(Shape::line(
            vec![pos2(x + inward, top), pos2(x, top), pos2(x, bottom), pos2(x + inward, bottom)],
            bracket,
        ));
    }
    // the playhead, with a small head
    let x = x_of(time.clamp(0.0, length));
    painter.line_segment(
        [pos2(x, track.top() - 2.0), pos2(x, rect.bottom())],
        Stroke::new(1.0, marks),
    );
    painter.add(Shape::convex_polygon(
        vec![
            pos2(x - 4.5, track.top() - 7.0),
            pos2(x + 4.5, track.top() - 7.0),
            pos2(x, track.top() - 1.0),
        ],
        marks,
        Stroke::NONE,
    ));
}

/// Plays a result: a play button and a bar to seek in it.
pub fn player(ui: &mut egui::Ui, length: f64, time: &mut f64, playing: &mut bool) {
    let length = length.max(1e-6);
    ui.horizontal(|ui| {
        play_button(ui, playing);
        let text = format!("{} / {}", widgets::seconds(*time), widgets::seconds(length));
        let label_width = 120.0;
        let width = (ui.available_width() - label_width).max(80.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());
        let track = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top() + 11.0),
            pos2(rect.right() - 8.0, rect.bottom() - 11.0),
        );
        let seconds_at =
            |x: f32| (f64::from((x - track.left()) / track.width()) * length).clamp(0.0, length);
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            *time = seconds_at(pointer.x).min(length - 1e-6);
        }
        if let Some(pointer) = response.hover_pos() {
            response.clone().on_hover_text_at_pointer(widgets::seconds(seconds_at(pointer.x)));
        }
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(track, 3.0, visuals.extreme_bg_color);
        let x = track.left() + (*time / length) as f32 * track.width();
        let played = Rect::from_min_max(track.min, pos2(x, track.max.y));
        // clipped rather than shrunk, so the start of the bar keeps its shape
        ui.painter_at(played).rect_filled(
            track,
            3.0,
            visuals.selection.bg_fill.gamma_multiply(0.7),
        );
        painter.circle_filled(pos2(x, track.center().y), 6.0, visuals.strong_text_color());
        ui.label(text);
    });
}

fn play_button(ui: &mut egui::Ui, playing: &mut bool) {
    let icon = if *playing { "⏸" } else { "▶" };
    if ui
        .add(egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(30.0, 26.0)))
        .on_hover_text(if *playing { "Pause (Space)" } else { "Play (Space)" })
        .clicked()
    {
        *playing = !*playing;
    }
}

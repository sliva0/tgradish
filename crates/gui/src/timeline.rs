//! The bar under the preview: where playback is, and which part of the
//! input is used, with handles and numbers to change it. Where the
//! input's frames are known, cuts land between them, and the numbers can
//! count frames instead of seconds.

use std::ops::RangeInclusive;

use eframe::egui::{self, CursorIcon, PointerButton, Rect, Sense, Shape, Stroke, pos2, vec2};

use crate::cuts::{Frames, part, set_part};
use crate::l10n::fl;
use crate::{theme, widgets};

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
    /// The input's frames, which cuts land between, when they are known.
    pub frames: Option<&'a Frames>,
    /// Numbers count frames rather than seconds.
    pub in_frames: &'a mut bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Start,
    End,
    Seek,
}

/// A field of a boundary between frames: its frame counted from 1, or
/// where it is in seconds. `end` marks the boundary after the last frame
/// used, which counts as that frame.
pub(crate) fn boundary_field<'a>(
    boundary: &'a mut usize,
    range: RangeInclusive<usize>,
    frames: &'a Frames,
    in_frames: bool,
    end: bool,
) -> egui::DragValue<'a> {
    let field = egui::DragValue::new(boundary).range(range).speed(0.1);
    if in_frames {
        let shift = if end { 0.0 } else { 1.0 };
        field
            .custom_formatter(move |n, _| format!("{}", n + shift))
            .custom_parser(move |text| text.trim().parse::<f64>().ok().map(|n| n.round() - shift))
    } else {
        let last = frames.count();
        field
            .custom_formatter(move |n, _| widgets::seconds(frames.times[(n as usize).min(last)]))
            .custom_parser(move |text| Some(frames.nearest(widgets::parse_seconds(text)?) as f64))
    }
}

/// A field of one of `frames`, by its `index`: its number from 1, or where
/// it starts in seconds. A time typed picks the frame shown then.
pub(crate) fn frame_field<'a>(
    index: &'a mut usize,
    frames: &'a Frames,
    in_frames: bool,
) -> egui::DragValue<'a> {
    let last = frames.count() - 1;
    let field = egui::DragValue::new(index).range(0..=last).speed(0.1);
    if in_frames {
        field
            .custom_formatter(|n, _| format!("{}", n + 1.0))
            .custom_parser(|text| text.trim().parse::<f64>().ok().map(|n| n.round() - 1.0))
    } else {
        field
            .custom_formatter(move |n, _| widgets::seconds(frames.times[(n as usize).min(last)]))
            .custom_parser(move |text| Some(frames.at(widgets::parse_seconds(text)?) as f64))
    }
}

/// How wide frames are on the bar, at least, for where they meet to be
/// marked.
const MARKED_FRAME: f32 = 6.0;

/// Width of [`unit`]'s two buttons.
const UNIT: f32 = 122.0;

/// Whether the numbers count frames or seconds, which `known` frames
/// allow, greyed out without them.
fn unit(ui: &mut egui::Ui, in_frames: &mut bool, known: bool) {
    let choices = [
        (false, fl!("in-seconds"), fl!("in-seconds", "hint")),
        (true, fl!("in-frames"), fl!("in-frames", "hint")),
    ];
    let mut shown = *in_frames && known;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 24.0;
        widgets::segments(ui, &mut shown, &choices, |_| {
            if known { Ok(()) } else { Err(fl!("in-frames", "unknown")) }
        });
    });
    if known {
        *in_frames = shown;
    }
}

/// Shows the timeline.
pub fn show(ui: &mut egui::Ui, timeline: Timeline) {
    let Timeline { length, start, used, time, playing, limit, frames, in_frames } = timeline;
    // one frame used has nothing to play
    let still = frames.is_some_and(|frames| {
        let (first, after) = frames.part(length, *start, *used);
        after - first == 1
    });
    if still {
        *playing = false;
    }
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!still, |ui| play_button(ui, playing))
            .response
            .on_disabled_hover_text(fl!("nothing-to-play"));
        let size = vec2(64.0, 24.0);
        match frames {
            Some(frames) => {
                let (mut first, after) = frames.part(length, *start, *used);
                let field = boundary_field(&mut first, 0..=after - 1, frames, *in_frames, false);
                let hint = fl!("part-start", "frame");
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    frames.set_part((first, after), start, used);
                    *time = frames.times[first];
                }
            }
            None => {
                let (mut from, to) = part(length, *start, *used);
                let field = seconds(&mut from, length);
                let hint = fl!("part-start");
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    from = from.min(to - 0.01).max(0.0);
                    set_part(length, (from, to), start, used);
                    *time = from;
                }
            }
        }
        let width = (ui.available_width() - 64.0 - 2.0 * 8.0 - UNIT).max(48.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());
        bar(ui, rect, &response, (length, frames, *in_frames), (start, used), time, limit);
        match frames {
            Some(frames) => {
                let (first, mut after) = frames.part(length, *start, *used);
                let range = first + 1..=frames.count();
                let field = boundary_field(&mut after, range, frames, *in_frames, true);
                let hint = fl!("part-end", "frame");
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    frames.set_part((first, after), start, used);
                }
                unit(ui, in_frames, true);
            }
            None => {
                let (from, mut to) = part(length, *start, *used);
                let field = seconds(&mut to, length);
                if ui.add_sized(size, field).on_hover_text(fl!("part-end")).changed() {
                    to = to.max(from + 0.01).min(length);
                    set_part(length, (from, to), start, used);
                }
                unit(ui, in_frames, false);
            }
        }
    });
}

/// A field of a time in the input, written as [`widgets::seconds`] writes
/// times, and read back so.
pub(crate) fn seconds(value: &mut f64, length: f64) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .range(0.0..=length)
        .speed(0.01)
        .custom_formatter(|value, _| widgets::seconds(value))
        .custom_parser(widgets::parse_seconds)
}

/// What the pointer is over, as the numbers count.
fn time_text(time: f64, frames: Option<&Frames>, in_frames: bool) -> String {
    match frames {
        Some(frames) if in_frames => {
            let number = frames.at(time) + 1;
            fl!("frame-number", number = number)
        }
        _ => widgets::seconds(time),
    }
}

fn bar(
    ui: &mut egui::Ui,
    rect: Rect,
    response: &egui::Response,
    (length, frames, in_frames): (f64, Option<&Frames>, bool),
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
    // where the used part's ends show: between frames, where they are known
    let ends = |start: Option<f64>, used: Option<f64>| match frames {
        Some(frames) => {
            let (first, after) = frames.part(length, start, used);
            (frames.times[first], frames.times[after])
        }
        None => part(length, start, used),
    };
    let (from, to) = ends(*start, *used);

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
        let text = time_text(seconds_at(pointer.x), frames, in_frames);
        response.clone().on_hover_text_at_pointer(text);
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
        match (grab, frames) {
            (Grab::Start, Some(frames)) => {
                let (_, after) = frames.part(length, *start, *used);
                let first = frames.nearest(at).min(after - 1);
                frames.set_part((first, after), start, used);
                *time = frames.times[first];
            }
            (Grab::End, Some(frames)) => {
                let (first, _) = frames.part(length, *start, *used);
                let after = frames.nearest(at).max(first + 1);
                frames.set_part((first, after), start, used);
                *time = frames.times[after - 1];
            }
            (Grab::Start, None) => {
                let at = at.min(to - 0.05).max(0.0);
                set_part(length, (at, to), start, used);
                *time = at;
            }
            (Grab::End, None) => {
                let at = at.max(from + 0.05).min(length);
                set_part(length, (from, at), start, used);
                *time = (at - 0.5).max(from);
            }
            (Grab::Seek, _) => *time = at,
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

    let (from, to) = ends(*start, *used);
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(track, 3.0, theme::tint(theme::accent(visuals), 0.5));
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
    // where frames meet, while they are far enough apart to tell
    if let Some(frames) = frames
        && track.width() / frames.count() as f32 >= MARKED_FRAME
    {
        let gap = Stroke::new(1.0, visuals.panel_fill);
        for &time in &frames.times[1..frames.count()] {
            let x = x_of(time).round() + 0.5;
            painter.line_segment([pos2(x, track.top()), pos2(x, track.bottom())], gap);
        }
    }
    // Telegram plays 3 seconds; longer stickers are spoofed, sped up or cut
    if to - from > limit + 1e-3 {
        let x = x_of(from + limit);
        painter.line_segment(
            [pos2(x, track.top()), pos2(x, track.bottom())],
            Stroke::new(1.5, widgets::WARN),
        );
        painter.text(
            pos2(x + 3.0, rect.top()),
            egui::Align2::LEFT_TOP,
            format!("{} s", tgradish_core::telegram::MAX_SECONDS),
            egui::FontId::proportional(10.5),
            widgets::WARN,
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
        ui.painter_at(played).rect_filled(track, 3.0, theme::tint(theme::accent(visuals), 0.5));
        painter.circle_filled(pos2(x, track.center().y), 6.0, visuals.strong_text_color());
        ui.label(text);
    });
}

/// Picks a frame of a video for a still picture: a frame either way, its
/// number or time, and a bar to click or drag along, the frame lit on it
/// for as long as it shows. `frame` is where the picked frame starts, as
/// the picture is made of it.
pub fn frame_picker(
    ui: &mut egui::Ui,
    frames: &Frames,
    frame: &mut Option<f64>,
    in_frames: &mut bool,
) {
    let length = frames.times[frames.count()].max(1e-6);
    ui.horizontal(|ui| {
        let mut index = frames.at(frame.unwrap_or(0.0));
        let last = frames.count() - 1;
        let mut picked = None;
        // as large as the play button, which stands here in the other modes
        let step = |ui: &mut egui::Ui, icon: &str, enabled: bool, hint: String, end: String| {
            let button =
                egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(30.0, 26.0));
            ui.add_enabled(enabled, button)
                .on_hover_text(hint)
                .on_disabled_hover_text(end)
                .clicked()
        };
        if step(ui, "⏴", index > 0, fl!("frame-before"), fl!("frame-before", "first")) {
            picked = Some(index - 1);
        }
        if step(ui, "⏵", index < last, fl!("frame-after"), fl!("frame-after", "last")) {
            picked = Some(index + 1);
        }
        let field = frame_field(&mut index, frames, *in_frames);
        if ui.add_sized(vec2(72.0, 24.0), field).on_hover_text(fl!("frame-picked")).changed() {
            picked = Some(index);
        }
        let width = (ui.available_width() - UNIT - 8.0).max(80.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());
        // as the bar of the used part in the other modes
        let track = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top() + 9.0),
            pos2(rect.right() - 8.0, rect.bottom() - 7.0),
        );
        let x_of = |seconds: f64| track.left() + (seconds / length) as f32 * track.width();
        let seconds_at =
            |x: f32| (f64::from((x - track.left()) / track.width()) * length).clamp(0.0, length);
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            picked = Some(frames.at(seconds_at(pointer.x)));
        }
        let hovered = response.hover_pos().map(|pointer| {
            let text = time_text(seconds_at(pointer.x), Some(frames), *in_frames);
            response.clone().on_hover_text_at_pointer(text);
            frames.at(seconds_at(pointer.x))
        });
        if let Some(picked) = picked {
            index = picked;
            *frame = (picked > 0).then_some(frames.times[picked]);
        }
        unit(ui, in_frames, true);
        // the span of a frame on the bar, at least wide enough to see
        let span = |index: usize| {
            let (from, to) = (x_of(frames.times[index]), x_of(frames.times[index + 1]));
            let middle = (from + to) / 2.0;
            let half = ((to - from) / 2.0).max(1.5);
            Rect::from_min_max(
                pos2(middle - half, track.top()),
                pos2(middle + half, track.bottom()),
            )
        };
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        // the other frames dimmed, as the unused part is in the other modes
        painter.rect_filled(track, 3.0, visuals.extreme_bg_color);
        // the frame lit, the bar's rounded ends kept
        painter.with_clip_rect(span(index)).rect_filled(track, 3.0, theme::accent(visuals));
        // where frames meet, while they are far enough apart to tell
        if track.width() / frames.count() as f32 >= MARKED_FRAME {
            let gap = Stroke::new(1.0, visuals.panel_fill);
            for &time in &frames.times[1..frames.count()] {
                let x = x_of(time).round() + 0.5;
                painter.line_segment([pos2(x, track.top()), pos2(x, track.bottom())], gap);
            }
        }
        // the frame a click picks
        if let Some(hovered) = hovered {
            let outline = Stroke::new(1.0, visuals.strong_text_color());
            painter.rect_stroke(span(hovered), 2.0, outline, egui::StrokeKind::Inside);
        }
    });
}

fn play_button(ui: &mut egui::Ui, playing: &mut bool) {
    let icon = if *playing { "⏸" } else { "▶" };
    if ui
        .add(egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(30.0, 26.0)))
        .on_hover_text(fl!("play-keys"))
        .clicked()
    {
        *playing = !*playing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuts::Cut;

    #[test]
    fn takes_times_typed_as_they_are_shown() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        // two frames a second over a minute and a half
        let frames = Frames::even(90.0, 2.0, Cut::Video).unwrap();
        let mut harness = Harness::new_ui_state(
            |ui, boundary: &mut usize| {
                ui.add(boundary_field(boundary, 0..=frames.count(), &frames, false, false));
            },
            125,
        );
        widgets::test_fonts(&harness);
        harness.run_steps(2);
        use egui::accesskit::Role;
        let spin = egui_kittest::kittest::by().role(Role::SpinButton).value("1:02.50");
        harness.get(spin).focus();
        harness.run_steps(2);
        // editing starts from the text shown, all of it selected: typed over
        harness.event(egui::Event::Text("1:10".into()));
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(*harness.state(), 140);
        // and fields of times without frames write them alike
        let mut plain = Harness::new_ui_state(
            |ui, time: &mut f64| {
                ui.add(seconds(time, 90.0));
            },
            62.5,
        );
        plain.run_steps(2);
        plain.get(egui_kittest::kittest::by().role(Role::SpinButton).value("1:02.50"));
    }

    #[test]
    fn picks_the_frame_shown_at_a_time_typed() {
        use egui::accesskit::Role;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let frames = Frames::even(1.0, 25.0, Cut::Video).unwrap();
        let typed = |in_frames: bool, text: &str| {
            let mut harness = Harness::new_ui_state(
                |ui, index: &mut usize| {
                    ui.add(frame_field(index, &frames, in_frames));
                },
                0,
            );
            widgets::test_fonts(&harness);
            harness.run_steps(2);
            harness.get_by_role(Role::SpinButton).focus();
            harness.run_steps(2);
            harness.event(egui::Event::Text(text.into()));
            harness.key_press(egui::Key::Enter);
            harness.run_steps(2);
            *harness.state()
        };
        // 0.07 s is within the second frame, which starts at 0.04
        assert_eq!(frames.at(0.07), 1);
        assert_eq!(typed(false, "0.07"), 1);
        // counted from 1
        assert_eq!(typed(true, "3"), 2);
    }
}

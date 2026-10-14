//! The bar under the preview: where playback is, and which part of the
//! input is used, with handles and numbers to change it. Where the
//! input's frames are known, cuts land between them, and the numbers can
//! count frames instead of seconds.

use std::ops::RangeInclusive;

use eframe::egui::{self, CursorIcon, PointerButton, Rect, Sense, Shape, Stroke, pos2, vec2};
use tgradish_core::convert::read_from;

use crate::{theme, widgets};

/// How a cut between frames is written, so converting keeps the frames
/// meant, whatever rounding does to the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// ffmpeg keeps frames from the first that starts at or after a cut,
    /// and counts them over the length: a cut is a hair before the frame
    /// it starts, and the length is the frames'.
    Video,
    /// Normalising keeps every frame a cut overlaps: a cut is a hair
    /// inside the frames it keeps.
    Art,
}

/// Where an input's frames start, then where the last one ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Frames {
    pub times: Vec<f64>,
    pub cut: Cut,
}

impl Frames {
    /// Frames starting at `starts`, in order, over `length`: the first
    /// shows from the beginning, and none starts at the end.
    pub fn starting(starts: &[f64], length: f64, cut: Cut) -> Option<Frames> {
        if starts.is_empty() || !(length > 0.0 && length.is_finite()) {
            return None;
        }
        let mut times = vec![0.0];
        times.extend(
            starts[1..].iter().copied().filter(|&start| start > 1e-6 && start < length - 1e-6),
        );
        times.push(length);
        Some(Frames { times, cut })
    }

    /// Frames of `1 / fps` seconds over `length`, the last one cut short;
    /// a tenth of a frame or less over the last is taken for rounding.
    pub fn even(length: f64, fps: f64, cut: Cut) -> Option<Frames> {
        if !(fps > 0.0 && length > 0.0) {
            return None;
        }
        let count = ((length * fps - 0.1).ceil() as usize).max(1);
        let mut times: Vec<f64> = (0..count).map(|i| i as f64 / fps).collect();
        times.push(length);
        Some(Frames { times, cut })
    }

    pub fn count(&self) -> usize {
        self.times.len() - 1
    }

    /// The frame shown at `time`.
    pub fn at(&self, time: f64) -> usize {
        self.times
            .partition_point(|&start| start <= time + 1e-9)
            .saturating_sub(1)
            .min(self.count() - 1)
    }

    /// The boundary between frames nearest `time`: 0 is before the first,
    /// [`Frames::count`] after the last.
    pub fn nearest(&self, time: f64) -> usize {
        let after = self.times.partition_point(|&boundary| boundary < time);
        match after {
            0 => 0,
            n if n > self.count() => self.count(),
            n if time - self.times[n - 1] <= self.times[n] - time => n - 1,
            n => n,
        }
    }

    /// The used part as boundaries: its first frame and the one after its
    /// last.
    pub fn part(&self, length: f64, start: Option<f64>, used: Option<f64>) -> (usize, usize) {
        let (from, to) = part(length, start, used);
        let first = self.nearest(from).min(self.count() - 1);
        (first, self.nearest(to).max(first + 1))
    }

    /// Uses the frames from `first` up to `after`, writing the cuts into
    /// `start` and `used` as the item stores them.
    pub fn set_part(
        &self,
        (first, after): (usize, usize),
        start: &mut Option<f64>,
        used: &mut Option<f64>,
    ) {
        let times = &self.times;
        let (from, length) = match self.cut {
            Cut::Video if first > 0 => {
                (read_from(times[first], times[first - 1]), times[after] - times[first])
            }
            Cut::Video => (0.0, times[after]),
            Cut::Art if first > 0 => (times[first] + 1e-6, times[after] - times[first] - 2e-6),
            Cut::Art => (0.0, times[after] - 1e-6),
        };
        *start = (first > 0).then_some(from);
        *used = (after < self.count()).then_some(length);
    }
}

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

/// The used part as `(start, end)`, and as the item stores it.
fn part(length: f64, start: Option<f64>, used: Option<f64>) -> (f64, f64) {
    let from = start.unwrap_or(0.0).clamp(0.0, length);
    (from, used.map_or(length, |used| (from + used).min(length)))
}

fn set_part(length: f64, (from, to): (f64, f64), start: &mut Option<f64>, used: &mut Option<f64>) {
    *start = (from > 1e-3).then_some(from);
    *used = (to < length - 1e-3).then_some(to - from);
}

/// A field of a boundary between frames: its frame counted from 1, or
/// where it is in seconds. `end` marks the boundary after the last frame
/// used, which counts as that frame.
fn boundary_field<'a>(
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
            .custom_parser(move |text| {
                let seconds: f64 = text.trim().trim_end_matches('s').trim().parse().ok()?;
                Some(frames.nearest(seconds) as f64)
            })
    }
}

/// What the keys over the preview do, for the play button's hover.
const KEYS: &str = "Space: play or pause\n← →: a frame back or on, with Shift ten\n\
                    Home, End: the first or last frame used\n\
                    [ ]: start or end the used part at the frame shown\n\
                    Ctrl+← →, Alt+← →: move its start or its end";

/// How wide frames are on the bar, at least, for where they meet to be
/// marked.
const MARKED_FRAME: f32 = 6.0;

/// Width of [`unit`]'s two buttons.
const UNIT: f32 = 122.0;

/// Whether the numbers count frames or seconds, which `known` frames
/// allow, greyed out without them.
fn unit(ui: &mut egui::Ui, in_frames: &mut bool, known: bool) {
    let choices = [
        (false, "Seconds", "Times in seconds"),
        (true, "Frames", "Times as frame numbers, from 1"),
    ];
    let mut shown = *in_frames && known;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 24.0;
        widgets::segments(ui, &mut shown, &choices, |_| {
            if known { Ok(()) } else { Err("Where the input's frames are isn't known") }
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
            .on_disabled_hover_text("One frame is used: there is nothing to play");
        let size = vec2(64.0, 24.0);
        match frames {
            Some(frames) => {
                let (mut first, after) = frames.part(length, *start, *used);
                let field = boundary_field(&mut first, 0..=after - 1, frames, *in_frames, false);
                let hint = "Where the used part starts: its first frame";
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    frames.set_part((first, after), start, used);
                    *time = frames.times[first];
                }
            }
            None => {
                let (mut from, to) = part(length, *start, *used);
                let field = seconds(&mut from, length);
                let hint = "Where the used part starts";
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    from = from.min(to - 0.01).max(0.0);
                    set_part(length, (from, to), start, used);
                    *time = from;
                }
            }
        }
        let width = (ui.available_width() - 64.0 - 2.0 * 8.0 - UNIT).max(80.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());
        bar(ui, rect, &response, (length, frames, *in_frames), (start, used), time, limit);
        match frames {
            Some(frames) => {
                let (first, mut after) = frames.part(length, *start, *used);
                let range = first + 1..=frames.count();
                let field = boundary_field(&mut after, range, frames, *in_frames, true);
                let hint = "Where the used part ends: its last frame";
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    frames.set_part((first, after), start, used);
                }
                unit(ui, in_frames, true);
            }
            None => {
                let (from, mut to) = part(length, *start, *used);
                let field = seconds(&mut to, length);
                if ui.add_sized(size, field).on_hover_text("Where the used part ends").changed() {
                    to = to.max(from + 0.01).min(length);
                    set_part(length, (from, to), start, used);
                }
                unit(ui, in_frames, false);
            }
        }
    });
}

fn seconds(value: &mut f64, length: f64) -> egui::DragValue<'_> {
    egui::DragValue::new(value).range(0.0..=length).speed(0.01).max_decimals(2).suffix(" s")
}

/// What the pointer is over, as the numbers count.
fn time_text(time: f64, frames: Option<&Frames>, in_frames: bool) -> String {
    match frames {
        Some(frames) if in_frames => format!("frame {}", frames.at(time) + 1),
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
            "3 s",
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
/// number or time, and a bar to click or drag along. `frame` is where the
/// picked frame starts, as the picture is made of it.
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
        let button = |text| egui::Button::new(text).min_size(vec2(26.0, 26.0));
        if ui.add(button("<")).on_hover_text("The frame before (←)").clicked() {
            picked = Some(index.saturating_sub(1));
        }
        if ui.add(button(">")).on_hover_text("The frame after (→)").clicked() {
            picked = Some((index + 1).min(last));
        }
        let field = boundary_field(&mut index, 0..=last, frames, *in_frames, false);
        if ui.add_sized(vec2(72.0, 24.0), field).on_hover_text("The frame").changed() {
            picked = Some(index);
        }
        let width = (ui.available_width() - UNIT - 8.0).max(80.0);
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
            picked = Some(frames.at(seconds_at(pointer.x)));
        }
        if let Some(pointer) = response.hover_pos() {
            let text = time_text(seconds_at(pointer.x), Some(frames), *in_frames);
            response.clone().on_hover_text_at_pointer(text);
        }
        if let Some(picked) = picked {
            index = picked;
            *frame = (picked > 0).then_some(frames.times[picked]);
        }
        unit(ui, in_frames, true);
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(track, 3.0, visuals.extreme_bg_color);
        // the frame, marked like a playhead
        let x = track.left() + (frames.times[index] / length) as f32 * track.width();
        let marks = visuals.strong_text_color();
        painter.line_segment(
            [pos2(x, track.top() - 4.0), pos2(x, rect.bottom())],
            Stroke::new(1.5, marks),
        );
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x - 5.0, track.top() - 9.0),
                pos2(x + 5.0, track.top() - 9.0),
                pos2(x, track.top() - 2.0),
            ],
            marks,
            Stroke::NONE,
        ));
    });
}

fn play_button(ui: &mut egui::Ui, playing: &mut bool) {
    let icon = if *playing { "⏸" } else { "▶" };
    if ui
        .add(egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(30.0, 26.0)))
        .on_hover_text(KEYS)
        .clicked()
    {
        *playing = !*playing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_frames_and_the_boundaries_between_them() {
        // 25 fps, 1 s: frames start every 0.04 s, the last at 0.96
        let frames = Frames::even(1.0, 25.0, Cut::Video).unwrap();
        assert_eq!(frames.count(), 25);
        assert_eq!(
            (frames.at(0.5), frames.at(0.48), frames.at(1.0), frames.at(-1.0)),
            (12, 12, 24, 0)
        );
        assert_eq!((frames.nearest(0.49), frames.nearest(0.51), frames.nearest(2.0)), (12, 13, 25));
        // a short last frame, and one a hair over rounding
        let short = Frames::even(0.97, 25.0, Cut::Video).unwrap();
        assert_eq!((short.count(), short.at(0.965)), (25, 24));
        assert_eq!(Frames::even(1.003, 25.0, Cut::Video).unwrap().count(), 25);
        // frames as long as their packets say, the first from the start
        let uneven = Frames::starting(&[0.02, 0.1, 0.2, 0.7], 0.8, Cut::Video).unwrap();
        assert_eq!(uneven.times, [0.0, 0.1, 0.2, 0.7, 0.8]);
        assert_eq!((uneven.at(0.65), uneven.at(0.7)), (2, 3));
    }

    #[test]
    fn cuts_where_converting_keeps_the_frames_meant() {
        let video = Frames::even(1.0, 25.0, Cut::Video).unwrap();
        // ffmpeg keeps from the first frame at or after the start, and
        // counts frames over the length
        let frames = |part| {
            let (mut start, mut used) = (None, None);
            video.set_part(part, &mut start, &mut used);
            assert_eq!(video.part(1.0, start, used), part);
            let first = start.map_or(0, |start| video.times.partition_point(|&t| t < start));
            (first, tgradish_core::convert::frame_count(used.unwrap(), 25.0) as usize)
        };
        // frames 5 to 9, and 0 to 9
        assert_eq!(frames((5, 10)), (5, 5));
        assert_eq!(frames((0, 10)), (0, 10));
        // the whole of it is no cut at all
        let (mut start, mut used) = (Some(0.3), Some(0.2));
        video.set_part((0, 25), &mut start, &mut used);
        assert_eq!((start, used), (None, None));

        // art keeps every frame a cut overlaps: cuts fall just inside
        let art = Frames { times: vec![0.0, 0.1, 0.25, 0.4], cut: Cut::Art };
        let (mut start, mut used) = (None, None);
        art.set_part((1, 2), &mut start, &mut used);
        let end = start.unwrap() + used.unwrap();
        assert!(start.unwrap() > 0.1 && end < 0.25, "{start:?} {used:?}");
        assert_eq!(art.part(0.4, start, used), (1, 2));

        // frames of different lengths keep their own
        let uneven = Frames::starting(&[0.0, 0.1, 0.2, 0.7], 0.8, Cut::Video).unwrap();
        let (mut start, mut used) = (None, None);
        uneven.set_part((2, 3), &mut start, &mut used);
        let start = start.unwrap();
        let used = used.unwrap();
        assert!(start > 0.1 && start < 0.2 && (used - 0.5).abs() < 1e-9, "{start} {used}");
        assert_eq!(uneven.part(0.8, Some(start), Some(used)), (2, 3));
    }
}

//! Playback and trim bar under the preview: playhead scrubbing, trim start/end
//! brackets and numeric fields, frame-picker mode for static outputs, and the
//! result player. When frame timestamps are known, trim boundaries snap
//! between frames and fields can display either frame numbers or seconds.

use std::ops::RangeInclusive;

use eframe::egui::{self, CursorIcon, PointerButton, Rect, Sense, Shape, Stroke, pos2, vec2};

use tgradish_core::options::Spoof;
use tgradish_core::presets::Format;
use tgradish_core::{tgs, webm};

use crate::cuts::{Frames, part, set_part};
use crate::l10n::fl;
use crate::{theme, widgets};

/// Parameters and mutable state for the input [`Timeline`].
pub(crate) struct Timeline<'a> {
    /// Total input duration in seconds.
    pub(crate) length: f64,
    pub(crate) start: &'a mut Option<f64>,
    /// Trimmed duration from `start` (`None` uses the remainder of the input).
    pub(crate) used: &'a mut Option<f64>,
    pub(crate) time: &'a mut f64,
    pub(crate) playing: &'a mut bool,
    /// Telegram's 3 s output duration limit, marked from the start of the used
    /// part, expressed in input seconds.
    pub(crate) limit: f64,
    /// How content beyond `limit` is handled.
    pub(crate) past: PastLimit,
    /// Known frame boundaries for frame-accurate snapping.
    pub(crate) frames: Option<&'a Frames>,
    /// Whether numeric fields display frame numbers instead of seconds.
    pub(crate) in_frames: &'a mut bool,
}

/// How the selected time range beyond Telegram's 3 s limit is handled (the
/// timeline hatches the region past the limit only when it will be [`Cut`](PastLimit::Cut)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PastLimit {
    /// Kept in full: WebM container duration is spoofed to `<= 3 s`.
    Spoofed,
    /// Kept in full: `.tgs` with automatic speed is accelerated to fit within 3 s.
    SpedUp,
    /// Truncated: output stops at the 3 s limit.
    Cut,
}

impl PastLimit {
    /// Determines how an output in `format` with effective options `webm` or
    /// `tgs` handles content past the 3 s limit.
    pub(crate) fn of(format: Format, webm: &webm::Options, tgs: &tgs::Options) -> PastLimit {
        match format {
            Format::Webm if webm.spoof_or_default() != Spoof::Never => PastLimit::Spoofed,
            // An explicit `.tgs` tempo plays at that exact speed and truncates past 3 s.
            Format::Tgs if tgs.tempo.is_none() => PastLimit::SpedUp,
            _ => PastLimit::Cut,
        }
    }

    /// Returns the tooltip and screen-reader description for the 3 s limit marker.
    pub(crate) fn text(self) -> String {
        match self {
            PastLimit::Spoofed => fl!("limit-mark", "spoofed"),
            PastLimit::SpedUp => fl!("limit-mark", "sped-up"),
            PastLimit::Cut => fl!("limit-mark"),
        }
    }
}

/// Returns the `(cut_start, cut_end)` interval truncated past `limit` when
/// `past` is [`PastLimit::Cut`], or `None` if nothing is truncated.
fn left_out(from: f64, to: f64, limit: f64, past: PastLimit) -> Option<(f64, f64)> {
    let over = to - from > limit + 1e-3;
    (over && past == PastLimit::Cut).then_some((from + limit, to))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Start,
    End,
    /// Ambiguous grab when `Start` and `End` overlap, resolved on the first
    /// pointer movement away from press `x`.
    Either(f32),
    /// Shift-drag sliding the entire trimmed range of `(from_sec, to_sec)`
    /// relative to press `x`.
    Slide((f64, f64), f32),
    Seek,
}

/// Hit-test radius in points around the trim start/end brackets.
const REACH: f32 = 8.0;

/// Determines what a pointer at `x` grabs on a bar with trim brackets at
/// `(start, end)`: the nearer bracket within [`REACH`] (splitting at the
/// midpoint when both brackets are within reach, or using movement direction
/// `towards`, where negative means moving left, when they overlap), or
/// [`Grab::Seek`] outside [`REACH`].
fn grab_at(x: f32, (start, end): (f32, f32), towards: f32) -> Grab {
    if (x - start).abs().min((x - end).abs()) > REACH {
        return Grab::Seek;
    }
    let middle = (start + end) / 2.0;
    match x - middle {
        side if side < -0.25 => Grab::Start,
        side if side > 0.25 => Grab::End,
        _ if towards < 0.0 => Grab::Start,
        _ if towards > 0.0 => Grab::End,
        _ => Grab::Either(x),
    }
}

/// Builds a numeric field for a frame boundary index (`0..=frames.count()`),
/// formatted as a 1-based frame number (`end = true` formats boundary `N` as
/// frame `N`) or as a timestamp in seconds.
pub(crate) fn boundary_field<'a>(
    boundary: &'a mut usize,
    range: RangeInclusive<usize>,
    frames: &'a Frames,
    in_frames: bool,
    end: bool,
) -> widgets::Number<'a> {
    let field = widgets::Number::new(boundary).range(range).speed(0.1);
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
            .stepper(move |n, steps, large| {
                let time = frames.times[(n.max(0.0) as usize).min(last)];
                by_seconds(n, steps, large, |seconds| frames.nearest(time + seconds))
            })
    }
}

/// Steps a frame or boundary `index` by `steps` frames (when `!large`) or by
/// `steps` seconds via `at` (when `large`, advancing by at least one frame).
fn by_seconds(index: f64, steps: f64, large: bool, at: impl Fn(f64) -> usize) -> f64 {
    if !large {
        return index + steps;
    }
    let to = at(steps) as f64;
    match steps > 0.0 {
        true => to.max(index + 1.0),
        false => to.min(index - 1.0),
    }
}

/// Builds a numeric field for a single frame `index` (`0..frames.count()`),
/// formatted as a 1-based frame number or start timestamp in seconds. A time
/// typed picks the frame shown then.
pub(crate) fn frame_field<'a>(
    index: &'a mut usize,
    frames: &'a Frames,
    in_frames: bool,
) -> widgets::Number<'a> {
    let last = frames.count() - 1;
    let field = widgets::Number::new(index).range(0..=last).speed(0.1);
    if in_frames {
        field
            .custom_formatter(|n, _| format!("{}", n + 1.0))
            .custom_parser(|text| text.trim().parse::<f64>().ok().map(|n| n.round() - 1.0))
    } else {
        field
            .custom_formatter(move |n, _| widgets::seconds(frames.times[(n as usize).min(last)]))
            .custom_parser(move |text| Some(frames.at(widgets::parse_seconds(text)?) as f64))
            .stepper(move |n, steps, large| {
                let time = frames.times[(n.max(0.0) as usize).min(last)];
                by_seconds(n, steps, large, |seconds| frames.at(time + seconds))
            })
    }
}

/// Minimum points per frame on the timeline bar before individual frame
/// tick marks are drawn.
const MARKED_FRAME: f32 = 6.0;

/// Timeline bar height (matches [`theme::CONTROL`] so buttons, fields, and bar
/// align on one row).
const BAR: f32 = theme::CONTROL;

/// Height of the inner track centred inside [`BAR`], leaving room above and
/// below for the playhead triangle, 3 s limit label, and hover readout pill.
const TRACK: f32 = 12.0;

/// Returns the inner track rectangle within `rect`, inset by 8 pt on each end
/// so trim brackets and the playhead triangle fit without clipping.
fn track_of(rect: Rect) -> Rect {
    Rect::from_center_size(rect.center(), vec2(rect.width() - 16.0, TRACK))
}

/// Width of the trim start and end numeric fields.
const FIELD: f32 = 64.0;

/// Minimum width of the timeline bar before the seconds/frames unit selector
/// wraps onto a second row below.
const NARROWEST_BAR: f32 = 160.0;

/// Width of the play/pause button and frame-step buttons.
const BUTTON: f32 = 30.0;
/// Width of the frame picker's numeric field.
const PICKED: f32 = 72.0;

/// Segment choices for the timeline unit toggle (`Seconds` vs `Frames`).
fn units() -> [widgets::Choice<bool>; 2] {
    [
        (false, fl!("in-seconds"), fl!("in-seconds", "hint")),
        (true, fl!("in-frames"), fl!("in-frames", "hint")),
    ]
}

/// Total width of the seconds/frames unit segments.
fn unit_width(ui: &egui::Ui) -> f32 {
    let padding = 2.0 * ui.spacing().button_padding.x;
    units().iter().map(|(_, text, _)| widgets::text_width(ui, text, padding) + 1.0).sum()
}

/// Returns true if a timeline row of `width` must place the unit segments on a
/// second line below the bar so the bar remains at least [`NARROWEST_BAR`] wide.
fn two_lines(ui: &egui::Ui, width: f32, picker: bool) -> bool {
    let gap = ui.spacing().item_spacing.x;
    let fixed = match picker {
        true => 2.0 * BUTTON + PICKED + 3.0 * gap,
        false => BUTTON + 2.0 * FIELD + 3.0 * gap,
    };
    fixed + NARROWEST_BAR + gap + unit_width(ui) > width
}

/// Returns any extra height beyond a single [`theme::CONTROL`] row needed when
/// the unit selector wraps onto a second line.
pub(crate) fn extra_height(ui: &egui::Ui, width: f32, picker: bool) -> f32 {
    match two_lines(ui, width, picker) {
        true => theme::CONTROL + ui.spacing().item_spacing.y,
        false => 0.0,
    }
}

/// Computes `(bar_width, unit_width)` from the remaining row width, reserving
/// `after` points for controls between the bar and unit segments (or giving
/// the full remaining width to the bar when `alone` puts the unit segments on
/// a second line).
fn bar_width(ui: &egui::Ui, after: f32, alone: bool) -> (f32, f32) {
    let gap = ui.spacing().item_spacing.x;
    let unit = unit_width(ui);
    let rest = ui.available_width() - after - gap;
    if alone {
        return ((rest + gap).max(1.0), unit);
    }
    let unit = unit.min(rest - NARROWEST_BAR);
    ((rest - unit).max(NARROWEST_BAR), unit)
}

/// Draws the seconds/frames unit segments right-aligned on a second line below
/// the timeline bar.
fn unit_below(ui: &mut egui::Ui, in_frames: &mut bool, known: bool) {
    let padding = 2.0 * ui.spacing().button_padding.x;
    let choices = units();
    let words: f32 =
        choices.iter().map(|(_, text, _)| widgets::text_width(ui, text, padding)).sum();
    let room = (words + (choices.len() - 1) as f32).min(ui.available_width());
    ui.horizontal(|ui| {
        ui.add_space(ui.available_width() - room);
        unit(ui, in_frames, known, room);
    });
}

/// Draws the seconds/frames unit selector within `room` (disabling `Frames`
/// when frame boundaries are not `known`).
fn unit(ui: &mut egui::Ui, in_frames: &mut bool, known: bool, room: f32) {
    let mut shown = *in_frames && known;
    widgets::segments_on_line(
        ui,
        &mut shown,
        &units(),
        |_| if known { Ok(()) } else { Err(fl!("in-frames", "unknown")) },
        room,
    );
    if known {
        *in_frames = shown;
    }
}

/// Draws the input timeline row.
pub(crate) fn show(ui: &mut egui::Ui, timeline: Timeline) {
    let Timeline { length, start, used, time, playing, limit, past, frames, in_frames } = timeline;
    // Pause and disable playback when the trimmed range spans only a single frame.
    let still = frames.is_some_and(|frames| {
        let (first, after) = frames.part(length, *start, *used);
        after - first == 1
    });
    if still {
        *playing = false;
    }
    let alone = two_lines(ui, ui.available_width(), false);
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!still, |ui| play_button(ui, playing))
            .response
            .on_disabled_hover_text(fl!("nothing-to-play"));
        let size = vec2(FIELD, ui.spacing().interact_size.y);
        match frames {
            Some(frames) => {
                let (mut first, after) = frames.part(length, *start, *used);
                let field = boundary_field(&mut first, 0..=after - 1, frames, *in_frames, false);
                let hint = fl!("part-start", "frame");
                if ui.add_sized(size, field.name(&hint)).on_hover_text(hint).changed() {
                    frames.set_part((first, after), start, used);
                    *time = frames.times[first];
                }
            }
            None => {
                let (mut from, to) = part(length, *start, *used);
                let field = seconds(&mut from, length);
                let hint = fl!("part-start");
                if ui.add_sized(size, field.name(&hint)).on_hover_text(hint).changed() {
                    from = from.min(to - 0.01).max(0.0);
                    set_part(length, (from, to), start, used);
                    *time = from;
                }
            }
        }
        let (width, unit_room) = bar_width(ui, FIELD + ui.spacing().item_spacing.x, alone);
        let (rect, response) = ui.allocate_exact_size(vec2(width, BAR), Sense::click_and_drag());
        let limit = (limit, past);
        let numbers = (length, frames, *in_frames);
        bar(ui, rect, &response, numbers, (start, used), time, limit);
        match frames {
            Some(frames) => {
                let (first, mut after) = frames.part(length, *start, *used);
                let range = first + 1..=frames.count();
                let field = boundary_field(&mut after, range, frames, *in_frames, true);
                let hint = fl!("part-end", "frame");
                if ui.add_sized(size, field.name(&hint)).on_hover_text(hint).changed() {
                    frames.set_part((first, after), start, used);
                }
                if !alone {
                    unit(ui, in_frames, true, unit_room);
                }
            }
            None => {
                let (from, mut to) = part(length, *start, *used);
                let hint = fl!("part-end");
                let field = seconds(&mut to, length).name(&hint);
                if ui.add_sized(size, field).on_hover_text(hint).changed() {
                    to = to.max(from + 0.01).min(length);
                    set_part(length, (from, to), start, used);
                }
                if !alone {
                    unit(ui, in_frames, false, unit_room);
                }
            }
        }
    });
    if alone {
        unit_below(ui, in_frames, frames.is_some());
    }
}

/// Builds a numeric field for a timestamp in `0.0..=length` seconds, formatted
/// and parsed via [`widgets::seconds`] and [`widgets::parse_seconds`].
pub(crate) fn seconds(value: &mut f64, length: f64) -> widgets::Number<'_> {
    widgets::Number::new(value)
        .range(0.0..=length)
        .speed(0.01)
        .steps(0.1, 1.0)
        .custom_formatter(|value, _| widgets::seconds(value))
        .custom_parser(widgets::parse_seconds)
}

/// Formats `time` for the timeline hover readout pill (as a 1-based frame
/// number when `in_frames` is active, or in seconds).
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
    (limit, past): (f64, PastLimit),
) {
    let length = length.max(1e-6);
    let track = track_of(rect);
    let x_of = |seconds: f64| track.left() + (seconds / length) as f32 * track.width();
    let seconds_at =
        |x: f32| (f64::from((x - track.left()) / track.width()) * length).clamp(0.0, length);
    // Snap displayed trim ends to frame boundaries when known.
    let ends = |start: Option<f64>, used: Option<f64>| match frames {
        Some(frames) => {
            let (first, after) = frames.part(length, start, used);
            (frames.times[first], frames.times[after])
        }
        None => part(length, start, used),
    };
    let (from, to) = ends(*start, *used);

    let ends_x = (x_of(from), x_of(to));
    // Holding Shift while dragging a trim bracket slides the entire range.
    let sliding = ui.input(|input| input.modifiers.shift_only());
    let hovered = response.hover_pos().map(|pointer| match grab_at(pointer.x, ends_x, 0.0) {
        Grab::Seek => Grab::Seek,
        _ if sliding => Grab::Slide((from, to), pointer.x),
        grab => grab,
    });
    let id = response.id.with("grab");
    let pointer = response.interact_pointer_pos();
    let origin = ui.input(|input| input.pointer.press_origin());
    // Pressing outside the trim brackets seeks immediately on mouse-down.
    if response.is_pointer_button_down_on()
        && ui.input(|input| input.pointer.primary_pressed())
        && let Some(origin) = origin
        && grab_at(origin.x, ends_x, 0.0) == Grab::Seek
    {
        *time = seconds_at(origin.x);
    }
    if response.drag_started_by(PointerButton::Primary)
        && let (Some(origin), Some(pointer)) = (origin, pointer)
    {
        let grab = match grab_at(origin.x, ends_x, pointer.x - origin.x) {
            Grab::Seek => Grab::Seek,
            _ if sliding => Grab::Slide((from, to), origin.x),
            grab => grab,
        };
        ui.data_mut(|data| data.insert_temp(id, grab));
    }
    // Resolve overlapping brackets in the direction of the first drag movement.
    let mut grabbed: Option<Grab> = ui.data(|data| data.get_temp(id));
    if let (Some(Grab::Either(origin)), Some(pointer)) = (grabbed, pointer)
        && pointer.x != origin
    {
        let grab = if pointer.x < origin { Grab::Start } else { Grab::End };
        ui.data_mut(|data| data.insert_temp(id, grab));
        grabbed = Some(grab);
    }
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
            (Grab::Slide((was_from, was_to), origin), Some(frames)) => {
                // Preserve the exact frame count while sliding to the nearest boundary.
                let count = frames.nearest(was_to) - frames.nearest(was_from);
                let by = at - seconds_at(origin);
                let first = frames.nearest(was_from + by).min(frames.count() - count);
                frames.set_part((first, first + count), start, used);
                *time = frames.times[first];
            }
            (Grab::Slide((was_from, was_to), origin), None) => {
                let by = at - seconds_at(origin);
                let from = (was_from + by).clamp(0.0, length - (was_to - was_from));
                set_part(length, (from, from + was_to - was_from), start, used);
                *time = from;
            }
            (Grab::Seek, _) => *time = at,
            (Grab::Either(_), _) => {}
        }
    }
    let cursor = match grabbed.filter(|_| response.dragged()).or(hovered) {
        Some(Grab::Slide(..)) if response.dragged() => Some(CursorIcon::Grabbing),
        Some(Grab::Slide(..)) => Some(CursorIcon::Grab),
        Some(Grab::Start | Grab::End | Grab::Either(_)) => Some(CursorIcon::ResizeHorizontal),
        _ => None,
    };
    if let Some(cursor) = cursor {
        ui.ctx().set_cursor_icon(cursor);
    }
    if response.clicked()
        && let Some(pointer) = pointer
    {
        *time = seconds_at(pointer.x);
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<Grab>(id));
    }
    // what is dragged till the button is let go is one step
    held_edit(ui, response);

    let (from, to) = ends(*start, *used);
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(track, 3.0, theme::span(visuals));
    // Mark Telegram's 3 s output limit relative to `from` when it falls within
    // the input duration, and hatch the portion past the limit only when it
    // will be truncated (`PastLimit::Cut`); the stripes stay put as the ends
    // move. Spoofed or sped-up content is all kept, so it is not hatched.
    let limit_x = (from + limit < length).then(|| x_of(from + limit));
    let cut = left_out(from, to, limit, past);
    if let Some((cut_from, cut_to)) = cut {
        let cut = Rect::from_min_max(
            pos2(x_of(cut_from), track.top()),
            pos2(x_of(cut_to), track.bottom()),
        );
        widgets::hatch(&painter, cut, track.left(), theme::past_limit(visuals));
    }
    // Dim the trimmed-out regions before `from` and after `to`.
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
    // Draw frame boundary ticks when frames are wide enough to distinguish.
    if let Some(frames) = frames
        && track.width() / frames.count() as f32 >= MARKED_FRAME
    {
        let gap = Stroke::new(1.0, visuals.panel_fill);
        for &time in &frames.times[1..frames.count()] {
            let x = x_of(time).round() + 0.5;
            painter.line_segment([pos2(x, track.top()), pos2(x, track.bottom())], gap);
        }
    }
    // Draw the 3 s limit tick and its "3 s" label above the track (coloured as
    // a warning only when part of the range is actually cut).
    let mut mark = None;
    if let Some(x) = limit_x {
        let ends = (track.top() - 2.0, track.bottom() + 2.0);
        let (left, right) = widgets::tick(&painter, visuals, x, ends);
        let warned = cut.is_some();
        let colour = if warned { theme::warn(visuals) } else { visuals.weak_text_color() };
        let text = format!("{} s", tgradish_core::telegram::MAX_SECONDS);
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(10.5), colour);
        // Position the label baseline 1 pt above the track, to the right of
        // the tick (or to the left near the right end of the bar).
        let baseline = galley
            .rows
            .first()
            .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y));
        let y = track.top() - 1.0 - baseline.unwrap_or(galley.size().y);
        let gap = 2.0;
        let x = if right + gap + galley.size().x <= rect.right() {
            right + gap
        } else {
            left - gap - galley.size().x
        };
        let label = Rect::from_min_size(pos2(x, y), galley.size());
        let tick = Rect::from_x_y_ranges(left..=right, ends.0..=ends.1);
        mark = Some((label, tick));
        // Expand the clip rect slightly above `rect` so tall glyphs are not clipped.
        let above = Rect::from_min_max(pos2(rect.left(), rect.top() - 3.0), rect.max);
        ui.painter().with_clip_rect(above).galley(pos2(x, y), galley, colour);
        let text = past.text();
        ui.ctx().accesskit_node_builder(response.id, |node| node.set_description(text));
    }
    // Show the past-limit tooltip when hovering the 3 s tick or its label,
    // but not while an end is dragged.
    if let Some(pointer) = response.hover_pos()
        && !response.dragged()
        && mark.is_some_and(|(label, tick)| {
            label.expand(1.0).contains(pointer) || tick.expand2(vec2(1.0, 0.0)).contains(pointer)
        })
    {
        response.clone().on_hover_text_at_pointer(past.text());
    }
    // Draw the trim start/end brackets, highlighting whichever bracket(s) will
    // be moved by a press or active drag.
    let marks = ui.visuals().strong_text_color();
    let lit = match grabbed {
        Some(grab) if response.dragged() => Some(grab),
        _ => hovered.filter(|_| !response.dragged()),
    };
    let lit = |end: Grab| {
        lit.is_some_and(|lit| lit == end || matches!(lit, Grab::Either(_) | Grab::Slide(..)))
    };
    for (x, inward, end) in [(x_of(from), 4.0, Grab::Start), (x_of(to), -4.0, Grab::End)] {
        let bracket = match lit(end) {
            true => Stroke::new(3.0, theme::mark(visuals)),
            false => Stroke::new(2.5, marks),
        };
        let (top, bottom) = (track.top() - 3.0, track.bottom() + 3.0);
        painter.add(Shape::line(
            vec![pos2(x + inward, top), pos2(x, top), pos2(x, bottom), pos2(x + inward, bottom)],
            bracket,
        ));
    }
    // Playhead: reaches as far under the track as its head does over it, drawn
    // as a line ending half its width back so it stays within `reach`.
    let x = x_of(time.clamp(0.0, length));
    let (reach, stroke) = (7.0, Stroke::new(1.0, marks));
    let end = track.bottom() + reach - stroke.width / 2.0;
    painter.line_segment([pos2(x, track.top() - 2.0), pos2(x, end)], stroke);
    painter.add(Shape::convex_polygon(
        vec![
            pos2(x - 4.5, track.top() - reach),
            pos2(x + 4.5, track.top() - reach),
            pos2(x, track.top() - 1.0),
        ],
        marks,
        Stroke::NONE,
    ));
    // Show the active drag value (or hovered time/frame) in a readout pill
    // next to the pointer.
    let dragged = grabbed.filter(|_| response.dragged());
    let x = (response.hover_pos().or(pointer)).map(|at| at.x.clamp(track.left(), track.right()));
    let said = match (dragged, frames) {
        (Some(Grab::Start | Grab::Slide(..)), Some(frames)) if in_frames => {
            let (first, _) = frames.part(length, *start, *used);
            let number = first + 1;
            Some(fl!("frame-number", number = number))
        }
        (Some(Grab::End), Some(frames)) if in_frames => {
            let (_, after) = frames.part(length, *start, *used);
            Some(fl!("frame-number", number = after))
        }
        (Some(Grab::Start | Grab::Slide(..)), _) => Some(widgets::seconds(from)),
        (Some(Grab::End), _) => Some(widgets::seconds(to)),
        _ => x.map(|x| time_text(seconds_at(x), frames, in_frames)),
    };
    // When Shift-sliding the range, anchor the readout pill under the range's
    // start bracket rather than under the pointer so it clearly labels the start time.
    let x = match dragged {
        Some(Grab::Slide(..)) => x.map(|_| x_of(from).clamp(track.left(), track.right())),
        _ => x,
    };
    let at = x.zip(said);
    if let Some((x, text)) = at.filter(|_| response.hovered() || response.dragged()) {
        readout(ui, rect, x, text);
    }
}

/// Draws an immediate readout pill below the track of `rect`, positioned just
/// left of `x` (so a top-left arrow cursor does not obscure it), or right of
/// `x` past the cursor width when near the left edge of the bar. Drawn by hand
/// because an egui tooltip appears only once the pointer rests and never while
/// a button is held; kept in the room under the track, never over the picture.
fn readout(ui: &egui::Ui, rect: Rect, x: f32, text: String) {
    let visuals = ui.visuals();
    let colour = visuals.strong_text_color();
    let painter = ui.ctx().layer_painter(ui.layer_id());
    let galley = painter.layout_no_wrap(text, READOUT, colour);
    // A point shorter than the text's line (there is room under the glyphs),
    // so the pill stays clear of the row under the bar.
    let size = galley.size() + vec2(2.0 * READOUT_PADDING, -1.0);
    let left = match x - READOUT_GAP - size.x {
        left if left >= rect.left() => left,
        _ => x + READOUT_PAST,
    };
    let left = left.min(rect.right() - size.x).max(rect.left());
    let pill = Rect::from_min_size(pos2(left, rect.center().y + READOUT_DOWN), size);
    let radius = size.y / 2.0;
    painter.rect(
        pill,
        radius,
        visuals.window_fill,
        theme::floating_outline(visuals),
        egui::StrokeKind::Inside,
    );
    painter.galley(pill.min + vec2(READOUT_PADDING, -0.5), galley, colour);
}

/// Font for the timeline hover/drag readout pill.
const READOUT: egui::FontId = egui::FontId::proportional(11.0);
/// Horizontal padding inside the readout pill.
const READOUT_PADDING: f32 = 5.0;
/// Gap between the readout pill's right edge and `x`.
const READOUT_GAP: f32 = 6.0;
/// Offset to the right of `x` when the pill does not fit on the left (clears
/// the standard arrow cursor width).
const READOUT_PAST: f32 = 14.0;
/// Vertical offset from the bar centre to the top of the readout pill.
const READOUT_DOWN: f32 = TRACK / 2.0 + 1.0;

/// Draws the result playback bar (play/pause button, scrub track, and
/// `current / total` timestamp label).
pub(crate) fn player(ui: &mut egui::Ui, length: f64, time: &mut f64, playing: &mut bool) {
    let length = length.max(1e-6);
    ui.horizontal(|ui| {
        play_button(ui, playing);
        let text = format!("{} / {}", widgets::seconds(*time), widgets::seconds(length));
        // Reserve fixed width for the widest `time / length` string so the
        // scrub bar does not resize during playback.
        let longest = widgets::zeros(&widgets::seconds(length));
        let widest = widgets::text_width(ui, &format!("{longest} / {longest}"), 0.0).ceil() + 1.0;
        let label_width = widest + ui.spacing().item_spacing.x;
        let width = (ui.available_width() - label_width).max(80.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, BAR), Sense::click_and_drag());
        let track = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top() + 10.0),
            pos2(rect.right() - 8.0, rect.bottom() - 10.0),
        );
        let seconds_at =
            |x: f32| (f64::from((x - track.left()) / track.width()) * length).clamp(0.0, length);
        // Seek immediately on mouse-down or drag.
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.is_pointer_button_down_on() || response.clicked())
        {
            *time = seconds_at(pointer.x).min(length - 1e-6);
        }
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(track, 3.0, visuals.extreme_bg_color);
        let x = track.left() + (*time / length) as f32 * track.width();
        let played = Rect::from_min_max(track.min, pos2(x, track.max.y));
        // Clip the full track rectangle so the left cap retains its rounded corners.
        ui.painter_at(played).rect_filled(track, 3.0, theme::span(visuals));
        painter.circle_filled(pos2(x, track.center().y), 6.0, visuals.strong_text_color());
        let pointer = response.hover_pos().or(response.interact_pointer_pos());
        if let Some(pointer) = pointer.filter(|_| response.hovered() || response.dragged()) {
            let x = pointer.x.clamp(track.left(), track.right());
            readout(ui, rect, x, widgets::seconds(seconds_at(x)));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(text));
    });
}

/// Draws the single-frame picker row for static WebP outputs from animated
/// inputs (previous/next frame buttons, frame/time field, and frame scrub bar).
/// `frame` is where the picked frame starts, as the picture is made of it.
pub(crate) fn frame_picker(
    ui: &mut egui::Ui,
    frames: &Frames,
    frame: &mut Option<f64>,
    in_frames: &mut bool,
) {
    let length = frames.times[frames.count()].max(1e-6);
    let alone = two_lines(ui, ui.available_width(), true);
    ui.horizontal(|ui| {
        let mut index = frames.at(frame.unwrap_or(0.0));
        let last = frames.count() - 1;
        let mut picked = None;
        // As large as the play button that stands here in the other modes.
        let step = |ui: &mut egui::Ui, icon: &str, enabled: bool, hint: String, end: String| {
            let button =
                egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(BUTTON, 0.0));
            widgets::named(ui.add_enabled(enabled, button), &hint)
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
        let hint = fl!("frame-picked");
        let field = frame_field(&mut index, frames, *in_frames).name(&hint);
        let size = vec2(PICKED, ui.spacing().interact_size.y);
        if ui.add_sized(size, field).on_hover_text(hint).changed() {
            picked = Some(index);
        }
        let (width, unit_room) = bar_width(ui, 0.0, alone);
        let (rect, response) = ui.allocate_exact_size(vec2(width, BAR), Sense::click_and_drag());
        // As the used part's bar in the other modes.
        let track = track_of(rect);
        let x_of = |seconds: f64| track.left() + (seconds / length) as f32 * track.width();
        let seconds_at =
            |x: f32| (f64::from((x - track.left()) / track.width()) * length).clamp(0.0, length);
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.is_pointer_button_down_on() || response.clicked())
        {
            picked = Some(frames.at(seconds_at(pointer.x)));
        }
        let hovered = response.hover_pos().map(|pointer| frames.at(seconds_at(pointer.x)));
        if let Some(picked) = picked {
            index = picked;
            *frame = frames.start_of(picked);
        }
        held_edit(ui, &response);
        if !alone {
            unit(ui, in_frames, true, unit_room);
        }
        // Screen rectangle for a single frame's duration on the bar (at least
        // 3 pt wide).
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
        // Dimmed as the unused part is.
        painter.rect_filled(track, 3.0, visuals.extreme_bg_color);
        // Highlight the selected frame while preserving the track's rounded ends.
        painter.with_clip_rect(span(index)).rect_filled(track, 3.0, theme::mark(visuals));
        if track.width() / frames.count() as f32 >= MARKED_FRAME {
            let gap = Stroke::new(1.0, visuals.panel_fill);
            for &time in &frames.times[1..frames.count()] {
                let x = x_of(time).round() + 0.5;
                painter.line_segment([pos2(x, track.top()), pos2(x, track.bottom())], gap);
            }
        }
        // Outline the hovered frame with a high-contrast two-tone border so it
        // remains visible even when hovering the already-selected frame.
        if let Some(hovered) = hovered {
            widgets::cased_outline(&painter, visuals, span(hovered), 2.0);
        }
        let pointer = response.hover_pos().or(response.interact_pointer_pos());
        if let Some(pointer) = pointer.filter(|_| response.hovered() || response.dragged()) {
            let x = pointer.x.clamp(track.left(), track.right());
            let text = time_text(seconds_at(x), Some(frames), *in_frames);
            readout(ui, rect, x, text);
        }
    });
    if alone {
        unit_below(ui, in_frames, true);
    }
}

/// Tells history of the edit `response`, a bar, makes as it is held: under
/// way while the button is down on it, and done with once it is let go.
fn held_edit(ui: &egui::Ui, response: &egui::Response) {
    if response.is_pointer_button_down_on() {
        widgets::tell_history(ui.ctx(), crate::history::underway);
    } else if response.drag_stopped() || response.clicked() {
        widgets::tell_history(ui.ctx(), crate::history::commit);
    }
}

fn play_button(ui: &mut egui::Ui, playing: &mut bool) {
    let (icon, name) = if *playing { ("⏸", fl!("play", "pause")) } else { ("▶", fl!("play")) };
    let button =
        egui::Button::new(egui::RichText::new(icon).size(15.0)).min_size(vec2(BUTTON, 0.0));
    if widgets::named(ui.add(button), &name).on_hover_text(fl!("play-keys")).clicked() {
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
    fn hatches_only_what_the_result_leaves_out() {
        // from half a second to five, over the 3 seconds from its start
        for past in [PastLimit::Spoofed, PastLimit::SpedUp] {
            assert_eq!(left_out(0.5, 5.0, 3.0, past), None, "{past:?}");
        }
        assert_eq!(left_out(0.5, 5.0, 3.0, PastLimit::Cut), Some((3.5, 5.0)));
        // twice as fast, 6 of the input's seconds fit
        assert_eq!(left_out(0.5, 5.0, 6.0, PastLimit::Cut), None);
        // within the limit, or over it by rounding, nothing is cut
        assert_eq!(left_out(0.5, 3.0, 3.0, PastLimit::Cut), None);
        assert_eq!(left_out(0.5, 3.5005, 3.0, PastLimit::Cut), None);
    }

    #[test]
    fn follows_what_becomes_of_the_part_past_the_limit() {
        use tgradish_core::options::Spoof;
        let webm = |spoof| webm::Options { spoof, ..webm::Options::default() };
        let tgs = |tempo| tgs::Options { tempo, ..tgs::Options::default() };
        let (unset, fast) = (tgs(None), tgs(Some(2.0)));
        let of = |spoof, tgs: &tgs::Options| PastLimit::of(Format::Webm, &webm(spoof), tgs);
        assert_eq!(of(None, &unset), PastLimit::Spoofed);
        assert_eq!(of(Some(Spoof::Auto), &unset), PastLimit::Spoofed);
        assert_eq!(of(Some(Spoof::Always), &unset), PastLimit::Spoofed);
        assert_eq!(of(Some(Spoof::Never), &unset), PastLimit::Cut);
        // a .tgs plays fast enough to fit, unless its speed is set
        let honest = webm(Some(Spoof::Never));
        assert_eq!(PastLimit::of(Format::Tgs, &honest, &unset), PastLimit::SpedUp);
        assert_eq!(PastLimit::of(Format::Tgs, &webm(None), &fast), PastLimit::Cut);
    }

    #[test]
    fn says_what_becomes_of_the_part_past_the_limit() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::{NodeT, Queryable};

        for past in [PastLimit::Spoofed, PastLimit::SpedUp, PastLimit::Cut] {
            let mut harness = Harness::builder().with_size(vec2(640.0, 80.0)).build_ui_state(
                |ui, (start, used, time): &mut (Option<f64>, Option<f64>, f64)| {
                    let (mut playing, mut in_frames) = (false, false);
                    show(
                        ui,
                        Timeline {
                            length: 10.0,
                            start,
                            used,
                            time,
                            playing: &mut playing,
                            limit: 3.0,
                            past,
                            frames: None,
                            in_frames: &mut in_frames,
                        },
                    );
                },
                (Some(0.5), Some(4.5), 1.0),
            );
            widgets::test_fonts(&harness);
            // hovers at once
            harness.ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            harness.run_steps(2);
            let text = past.text();
            let bar = harness
                .query_all(egui_kittest::kittest::by())
                .find(|node| node.accesskit_node().description().as_ref() == Some(&text))
                .unwrap_or_else(|| panic!("{past:?}: {text}"))
                .rect();
            // the tick, 3 seconds after the used part's start, says it on
            // hover, and the rest of the bar where the pointer is
            let track = bar.shrink2(vec2(8.0, 0.0));
            let at =
                |seconds: f32| pos2(track.left() + seconds / 10.0 * track.width(), bar.center().y);
            harness.hover_at(at(3.5));
            harness.run_steps(2);
            assert!(harness.query_by_label(&text).is_some(), "{past:?}: {text}");
            harness.hover_at(at(2.0));
            harness.run_steps(2);
            assert!(harness.query_by_label(&text).is_none(), "{past:?}: {text}");
            assert!(painted(&harness).contains(&"2.00 s".to_owned()), "{past:?}");
        }
    }

    /// The texts `harness` drew last.
    fn painted<State>(harness: &egui_kittest::Harness<'_, State>) -> Vec<String> {
        let shapes = harness.output().shapes.iter();
        let texts = shapes.filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        });
        texts.collect()
    }

    #[test]
    fn takes_the_nearer_end_of_the_used_part() {
        // apart, each within reach of the pointer
        assert_eq!(grab_at(98.0, (100.0, 103.0), 0.0), Grab::Start);
        assert_eq!(grab_at(101.0, (100.0, 103.0), 0.0), Grab::Start);
        assert_eq!(grab_at(102.0, (100.0, 103.0), 0.0), Grab::End);
        assert_eq!(grab_at(110.0, (100.0, 103.0), 0.0), Grab::End);
        assert_eq!(grab_at(112.0, (100.0, 103.0), 0.0), Grab::Seek);
        // met: the way the pointer goes, once it does
        assert_eq!(grab_at(100.0, (100.0, 100.0), -1.0), Grab::Start);
        assert_eq!(grab_at(100.0, (100.0, 100.0), 1.0), Grab::End);
        assert_eq!(grab_at(100.0, (100.0, 100.0), 0.0), Grab::Either(100.0));
    }

    /// A harness of a timeline 640 points wide over 100 seconds, used from
    /// `from` to `to`, without frames, and the track of its bar.
    fn long_timeline(from: f64, to: f64) -> (egui_kittest::Harness<'static, Trim>, Rect) {
        timeline_of(from, to, None)
    }

    /// As [`long_timeline`], of `fps` frames a second where given.
    fn timeline_of(
        from: f64,
        to: f64,
        fps: Option<f64>,
    ) -> (egui_kittest::Harness<'static, Trim>, Rect) {
        use egui_kittest::kittest::{NodeT, Queryable};
        let frames = fps.and_then(|fps| Frames::even(100.0, fps, Cut::Video));
        let mut harness =
            egui_kittest::Harness::builder().with_size(vec2(640.0, 80.0)).build_ui_state(
                move |ui, (start, used, time): &mut Trim| {
                    let (mut playing, mut in_frames) = (false, false);
                    show(
                        ui,
                        Timeline {
                            length: 100.0,
                            start,
                            used,
                            time,
                            playing: &mut playing,
                            limit: 3.0,
                            past: PastLimit::Cut,
                            frames: frames.as_ref(),
                            in_frames: &mut in_frames,
                        },
                    );
                },
                (Some(from), Some(to - from), from),
            );
        widgets::test_fonts(&harness);
        harness.run_steps(2);
        // the bar, which says what becomes of the part past the limit
        let text = PastLimit::Cut.text();
        let bar = harness
            .query_all(egui_kittest::kittest::by())
            .find(|node| node.accesskit_node().description().as_ref() == Some(&text))
            .unwrap()
            .rect();
        (harness, track_of(bar))
    }

    type Trim = (Option<f64>, Option<f64>, f64);

    /// Drags in `harness` from `from` by `by` points across, a step at a
    /// time, with `modifiers` held.
    fn drag_across(
        harness: &mut egui_kittest::Harness<'_, Trim>,
        from: egui::Pos2,
        by: f32,
        modifiers: egui::Modifiers,
    ) {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        harness.event(egui::Event::PointerMoved(from));
        harness.event(press(from, true));
        harness.step();
        for step in 1..=4 {
            harness.event(egui::Event::PointerMoved(from + vec2(by * step as f32 / 4.0, 0.0)));
            harness.step();
        }
        harness.event(press(from + vec2(by, 0.0), false));
        harness.step();
        harness.step();
    }

    #[test]
    fn drags_either_end_of_a_short_part_of_a_long_input() {
        // three tenths of a second of a hundred, both ends within reach of
        // either: the one on the pointer's side moves
        let (mut harness, track) = long_timeline(50.0, 50.3);
        let x_of = |seconds: f32| track.left() + seconds / 100.0 * track.width();
        let ends = |harness: &egui_kittest::Harness<'_, Trim>| {
            let (start, used, _) = *harness.state();
            (start.unwrap(), start.unwrap() + used.unwrap())
        };
        drag_across(
            &mut harness,
            pos2(x_of(50.3) + 3.0, track.center().y),
            40.0,
            Default::default(),
        );
        let (from, to) = ends(&harness);
        assert!((from - 50.0).abs() < 1e-6 && to > 55.0, "{from} to {to}");
        let (mut harness, track) = long_timeline(50.0, 50.3);
        drag_across(
            &mut harness,
            pos2(x_of(50.0) - 3.0, track.center().y),
            -40.0,
            Default::default(),
        );
        let (from, to) = ends(&harness);
        assert!(from < 45.0 && (to - 50.3).abs() < 1e-6, "{from} to {to}");
        // met, as the least part used, the way the pointer goes
        for (by, start_moves) in [(-40.0, true), (40.0, false)] {
            let (mut harness, track) = long_timeline(50.0, 50.05);
            let middle = pos2(x_of(50.025), track.center().y);
            drag_across(&mut harness, middle, by, Default::default());
            let (from, to) = ends(&harness);
            assert_eq!(from < 49.0, start_moves, "{by}: {from} to {to}");
            assert_eq!(to > 51.0, !start_moves, "{by}: {from} to {to}");
        }
    }

    #[test]
    fn reads_out_an_end_snapped_to_frames_beside_the_pointer_dragging_it() {
        // a frame every two seconds, a dozen points wide
        let (mut harness, track) = timeline_of(50.0, 52.0, Some(0.5));
        let x_of = |seconds: f32| track.left() + seconds / 100.0 * track.width();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let from = pos2(x_of(52.0) + 2.0, track.center().y);
        harness.event(egui::Event::PointerMoved(from));
        harness.event(press(from, true));
        harness.step();
        for step in 1..=6 {
            let at = from + vec2(5.0 * step as f32, 0.0);
            harness.event(egui::Event::PointerMoved(at));
            harness.step();
            // once dragged, past egui's threshold
            if step == 1 {
                continue;
            }
            // what the end is, said just left of the pointer, under the track
            let (start, used, _) = *harness.state();
            let end = widgets::seconds(start.unwrap() + used.unwrap());
            let said = harness.output().shapes.iter().find_map(|clipped| match &clipped.shape {
                Shape::Text(text) if text.galley.text() == end => {
                    let place = text.galley.rect.translate(text.pos.to_vec2());
                    (place.top() > track.bottom()).then_some(place)
                }
                _ => None,
            });
            let said = said.unwrap_or_else(|| panic!("{step}: {end} not said"));
            let left_of = at.x - said.right();
            assert!((6.0..=12.0).contains(&left_of), "{step}: {said:?}, {left_of} left of {at:?}");
        }
        harness.event(press(from + vec2(30.0, 0.0), false));
        harness.step();
    }

    #[test]
    fn reads_out_where_the_pointer_is_and_where_a_drag_takes_an_end_at_once() {
        let (mut harness, track) = long_timeline(50.0, 50.3);
        let x_of = |seconds: f32| track.left() + seconds / 100.0 * track.width();
        // where a text was drawn
        let place = |harness: &egui_kittest::Harness<'_, Trim>, said: &str| {
            harness.output().shapes.iter().find_map(|clipped| match &clipped.shape {
                Shape::Text(text) if text.galley.text() == said => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
        };
        // hovered, with tooltips as slow as they are: just left of the
        // pointer, clear of its arrow, which points up and left from where
        // it is, under the track, within the room under the row, in text
        // as large as it reads
        harness.event(egui::Event::PointerMoved(pos2(x_of(20.0), track.center().y)));
        harness.step();
        let said = place(&harness, "20.00 s").expect("said at once");
        let bar = track.expand2(vec2(8.0, (BAR - TRACK) / 2.0));
        assert!(said.top() > track.bottom() && said.bottom() < bar.bottom() + 12.0, "{said:?}");
        let left_of = x_of(20.0) - said.right();
        assert!((6.0..=12.0).contains(&left_of), "{said:?}: {left_of} left of the pointer");
        let size = harness.output().shapes.iter().find_map(|clipped| match &clipped.shape {
            Shape::Text(text) if text.galley.text() == "20.00 s" => {
                Some(text.galley.job.sections[0].format.font_id.size)
            }
            _ => None,
        });
        assert!(size.is_some_and(|size| (11.0..=12.0).contains(&size)), "{size:?}");
        // near the bar's left end, where it wouldn't fit there: right of
        // the pointer, past its arrow
        harness.event(egui::Event::PointerMoved(pos2(x_of(1.0), track.center().y)));
        harness.step();
        let said = place(&harness, "1.00 s").expect("said at once");
        assert!(said.left() >= x_of(1.0) + 12.0 && said.left() <= x_of(1.0) + 24.0, "{said:?}");
        assert!(said.left() >= bar.left(), "{said:?}");
        // the end dragged, where it is at each step, with the button held
        let from = pos2(x_of(50.3) + 3.0, track.center().y);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(from));
        harness.event(press(from, true));
        harness.step();
        for step in 1..=4 {
            harness.event(egui::Event::PointerMoved(from + vec2(10.0 * step as f32, 0.0)));
            harness.step();
            let (start, used, _) = *harness.state();
            let end = start.unwrap() + used.unwrap();
            assert!(end > 50.3, "{end}");
            assert!(place(&harness, &widgets::seconds(end)).is_some(), "{step}: {end}");
        }
        harness.event(press(from + vec2(40.0, 0.0), false));
        harness.step();
    }

    #[test]
    fn seeks_as_the_button_goes_down() {
        let (mut harness, track) = long_timeline(10.0, 90.0);
        let at = pos2(track.left() + 0.4 * track.width(), track.center().y);
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
        assert!((harness.state().2 - 40.0).abs() < 0.01, "{}", harness.state().2);
    }

    #[test]
    fn reads_out_the_start_of_the_part_slid_under_it() {
        // slid by its end, which the pointer is by, its start said under
        // the start, not by the pointer, where it would read as the time
        // there
        let (mut harness, track) = timeline_of(20.0, 23.0, None);
        let x_of = |seconds: f64| track.left() + (seconds / 100.0) as f32 * track.width();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::SHIFT,
        };
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
        let from = pos2(x_of(23.0), track.center().y);
        harness.event(egui::Event::PointerMoved(from));
        harness.event(press(from, true));
        harness.step();
        for step in 1..=4 {
            harness.event(egui::Event::PointerMoved(from + vec2(10.0 * step as f32, 0.0)));
            harness.step();
        }
        let start = harness.state().0.unwrap();
        assert!(start > 20.0, "{start}");
        let said = widgets::seconds(start);
        let place = harness.output().shapes.iter().find_map(|clipped| match &clipped.shape {
            Shape::Text(text) if text.galley.text() == said => {
                let place = text.galley.rect.translate(text.pos.to_vec2());
                (place.top() > track.bottom()).then_some(place)
            }
            _ => None,
        });
        let place = place.unwrap_or_else(|| panic!("{said} not said"));
        let left_of = x_of(start) - place.right();
        assert!((6.0..=12.0).contains(&left_of), "{place:?}, {left_of} left of its start");
        harness.event(press(from + vec2(40.0, 0.0), false));
        harness.step();
    }

    #[test]
    fn slides_the_used_part_by_an_end_with_shift() {
        // with frames and without, by either end
        for fps in [None, Some(10.0)] {
            for end in [20.0, 23.0] {
                let (mut harness, track) = timeline_of(20.0, 23.0, fps);
                let x_of = |seconds: f32| track.left() + seconds / 100.0 * track.width();
                let from = pos2(x_of(end), track.center().y);
                // ten seconds on
                let by = x_of(30.0) - x_of(20.0);
                harness.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
                drag_across(&mut harness, from, by, egui::Modifiers::SHIFT);
                let (start, used, time) = *harness.state();
                let (start, used) = (start.unwrap(), used.unwrap());
                assert!((start - 30.0).abs() < 0.11, "{fps:?}, {end}: {start}");
                assert!((used - 3.0).abs() < 1e-6, "{fps:?}, {end}: {used}");
                assert!((time - start).abs() < 0.01, "{fps:?}, {end}: {time}");
                // and no further than the input's end
                let from = pos2(x_of(end + 10.0), track.center().y);
                drag_across(&mut harness, from, 10.0 * by, egui::Modifiers::SHIFT);
                let (start, used, _) = *harness.state();
                let (start, end) = part(100.0, start, used);
                assert!((start - 97.0).abs() < 0.01 && end == 100.0, "{fps:?}, {end}: {start}");
            }
        }
    }

    #[test]
    fn reads_out_where_the_pointer_is_on_the_results_bar() {
        let mut harness =
            egui_kittest::Harness::builder().with_size(vec2(640.0, 80.0)).build_ui_state(
                |ui, (time, playing): &mut (f64, bool)| player(ui, 10.0, time, playing),
                (0.0, false),
            );
        widgets::test_fonts(&harness);
        harness.run_steps(2);
        // on the bar, in the middle of the row
        harness.event(egui::Event::PointerMoved(pos2(300.0, 8.0 + BAR / 2.0)));
        harness.step();
        let said = painted(&harness);
        let readout = said.iter().filter(|text| !text.contains('/') && text.ends_with(" s"));
        assert_eq!(readout.count(), 1, "{said:?}");
        // and seeks as the button goes down
        harness.event(egui::Event::PointerButton {
            pos: pos2(300.0, 8.0 + BAR / 2.0),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
        assert!(harness.state().0 > 3.0, "{}", harness.state().0);
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

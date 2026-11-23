//! Progress bars and gauges, easing their fill and sweeping a sheen across
//! work in progress.

use eframe::egui::{self, Color32};

use super::paint::hatch;
use crate::theme;

/// Draws a usage gauge bar (`used` vs `limit`), turning red when `used > limit`
/// and wrapping `text` across multiple lines when narrow.
pub(crate) fn gauge(ui: &mut egui::Ui, used: u64, limit: u64, text: String) {
    let share = used as f32 / limit.max(1) as f32;
    let colour = if share > 1.0 { theme::NEGATIVE } else { theme::POSITIVE };
    let width = ui.available_width().min(380.0);
    let size = egui::vec2(width, bar_height(ui, &[&text], width));
    bar_sized(ui, size, Fill::Share(share), &text, colour.gamma_multiply(0.55));
}

/// Fill state of a [`bar`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Fill {
    /// Static completed fraction (`0.0..=1.0`).
    Share(f32),
    /// In-progress fraction (`0.0..=1.0`) with an animated sheen sweeping across
    /// it so slow steps do not look frozen.
    Working(f32),
    /// In-progress work with unknown fraction (animated sheen only).
    Unknown,
}

impl Fill {
    /// Returns [`Fill::Working`] if `share` is known, or [`Fill::Unknown`]
    /// otherwise.
    pub(crate) fn working(share: Option<f32>) -> Fill {
        share.map_or(Fill::Unknown, Fill::Working)
    }
}

/// Draws a single-line progress bar filled according to `fill` with `text`
/// overlaid.
pub(crate) fn bar(ui: &mut egui::Ui, fill: Fill, text: &str, colour: Color32) {
    let width = ui.available_width().min(380.0);
    bar_sized(ui, egui::vec2(width, BAR), fill, text, colour);
}

/// Single-line [`bar`] height.
const BAR: f32 = 22.0;
/// Font size for text overlaid on a [`bar`].
const BAR_FONT: f32 = 12.5;

/// Draws a [`bar`] sized tall enough for both `text` and `widest` (the longest
/// string the bar may display during its run) so wrapped lines fit in full
/// without changing the bar's height as progress updates.
pub(crate) fn steady_bar(ui: &mut egui::Ui, fill: Fill, text: &str, widest: &str, colour: Color32) {
    let width = ui.available_width().min(380.0);
    let size = egui::vec2(width, bar_height(ui, &[text, widest], width));
    bar_drawn(ui, size, fill, (text, text), colour, None);
}

/// Returns the height needed for a bar of `width` to display the tallest
/// wrapped string in `texts`.
fn bar_height(ui: &egui::Ui, texts: &[&str], width: f32) -> f32 {
    let font = egui::FontId::proportional(BAR_FONT);
    let lines = |text: &str| {
        let galley =
            ui.painter().layout(text.to_owned(), font.clone(), Color32::WHITE, width - 16.0);
        galley.rows.len().max(1)
    };
    let most = texts.iter().map(|text| lines(text)).max().unwrap_or(1);
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    BAR + (most - 1) as f32 * row
}

/// Draws a [`bar`] with explicit dimensions `size`.
pub(crate) fn bar_sized(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fill: Fill,
    text: &str,
    colour: Color32,
) {
    bar_drawn(ui, size, fill, (text, text), colour, None);
}

/// Draws a thin progress bar without overlaid text, announced to screen readers
/// as `name`.
pub(crate) fn thin_bar(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fill: Fill,
    name: &str,
    colour: Color32,
) {
    bar_drawn(ui, size, fill, ("", name), colour, None);
}

/// Draws a completed [`bar`] for a discarded trial step (such as an oversize
/// encode): filled in a neutral spent shade with the outcome suffix from byte
/// `from` onwards highlighted in the warning colour, and sized for `widest` to
/// match the height of the preceding [`steady_bar`].
pub(crate) fn bar_spent(ui: &mut egui::Ui, text: &str, widest: &str, from: usize) {
    let width = ui.available_width().min(380.0);
    let (fill, ending) = (theme::spent(ui.visuals()), theme::warn(ui.visuals()));
    let size = egui::vec2(width, bar_height(ui, &[text, widest], width));
    bar_drawn(ui, size, Fill::Share(1.0), (text, text), fill, Some((from, ending)));
}

/// Exponential easing time constant (seconds to cover `1 - 1/e` of an upward
/// change) for bar fill so infrequent progress updates animate smoothly.
const FILL_EASING: f32 = 0.12;
/// Duration in seconds for the sheen band to sweep across an active bar.
const SHEEN: f64 = 1.6;

/// Returns the displayed fill fraction for bar `id`, easing smoothly upwards
/// and snapping immediately when the target drops (e.g. a new encode pass) or
/// reaches `1.0`.
fn shown_share(ctx: &egui::Context, id: egui::Id, fill: Fill) -> Option<f32> {
    let (Fill::Share(share) | Fill::Working(share)) = fill else { return None };
    let share = share.clamp(0.0, 1.0);
    let jump = |shown: f32| share < shown || share >= 1.0;
    Some(crate::motion::approach(ctx, id.with("fill"), share, FILL_EASING, jump))
}

/// Returns the normalised position (`0.0..=1.0`) of the sheen band at `time`,
/// using smoothstep easing so it eases in and out at the ends of the bar.
fn sheen_at(time: f64) -> f32 {
    let phase = (time / SHEEN).fract() as f32;
    phase * phase * (3.0 - 2.0 * phase)
}

/// Renders a progress bar of `size`, optionally colouring the text suffix from
/// `ending` onwards and exposing `name` to screen readers.
fn bar_drawn(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fill: Fill,
    (text, name): (&str, &str),
    colour: Color32,
    ending: Option<(usize, Color32)>,
) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    // Report the label and percentage (when known) to screen readers, matching
    // egui's ProgressBar.
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, enabled, name);
        if let Fill::Share(share) | Fill::Working(share) = fill {
            info.value = Some(f64::from(share.clamp(0.0, 1.0) * 100.0).round());
        }
        info
    });
    let rounding = (size.y / 2.0).min(4.0);
    let painter = ui.painter_at(rect);
    let (track, outline) = theme::track(ui.visuals());
    painter.rect_filled(rect, rounding, track);
    // Clip the full rounded bar rect to the filled width so near-zero fills do
    // not draw a rounded cap wider than the fill.
    if let Some(share) = shown_share(ui.ctx(), response.id, fill) {
        let width = rect.width() * share;
        let part = egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height()));
        ui.painter_at(part).rect_filled(rect, rounding, colour);
    }
    let moving = crate::motion::motion(ui.ctx());
    // When motion is disabled, draw static diagonal hatching for indeterminate bars.
    if !moving && fill == Fill::Unknown {
        hatch(&painter, rect, rect.left(), colour);
    }
    if moving && !matches!(fill, Fill::Share(_)) {
        let band = rect.width() * 0.3;
        let across = sheen_at(ui.input(|input| input.time));
        let left = rect.left() - band + across * (rect.width() + band);
        let part = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(left + band, rect.bottom()),
        );
        // Use a translucent highlight over a known share so it shows across
        // both filled and unfilled regions.
        let shade = match fill {
            Fill::Unknown => colour,
            _ => ui.visuals().strong_text_color().gamma_multiply(0.14),
        };
        ui.painter_at(part.intersect(rect)).rect_filled(rect, rounding, shade);
        ui.ctx().request_repaint();
    }
    painter.rect_stroke(rect, rounding, outline, egui::StrokeKind::Inside);
    // Wrap across as many rows as the bar height allows, truncating (with a
    // full hover tooltip) only when it still overflows.
    let colour = ui.visuals().strong_text_color();
    let font = egui::FontId::proportional(BAR_FONT);
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let mut job = egui::text::LayoutJob { break_on_newline: false, ..Default::default() };
    let (from, ending) = ending.unwrap_or((text.len(), colour));
    let from = (0..=from.min(text.len())).rev().find(|at| text.is_char_boundary(*at)).unwrap_or(0);
    for (part, colour) in [(&text[..from], colour), (&text[from..], ending)] {
        job.append(part, 0.0, egui::TextFormat::simple(font.clone(), colour));
    }
    let rows = 1 + ((rect.height() - BAR) / row.max(1.0)).round().clamp(0.0, 8.0) as usize;
    job.wrap = if rows == 1 {
        egui::text::TextWrapping::truncate_at_width(rect.width() - 16.0)
    } else {
        egui::text::TextWrapping {
            max_rows: rows,
            ..egui::text::TextWrapping::wrap_at_width(rect.width() - 16.0)
        }
    };
    let galley = painter.layout_job(job);
    let at = egui::pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0);
    let elided = galley.elided;
    painter.galley(at, galley, colour);
    if elided {
        response.on_hover_text(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Verifies that progress bars expose their text label and percentage to
    /// screen readers.
    #[test]
    fn tells_screen_readers_what_a_bar_shows() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::NodeT;
        let mut harness = Harness::new_ui(|ui| {
            gauge(ui, 300, 256, "300 KiB of 256 KiB".to_owned());
            bar(ui, Fill::Working(0.25), "Encoding", Color32::WHITE);
            bar(ui, Fill::Unknown, "Reading", Color32::WHITE);
            thin_bar(ui, egui::vec2(100.0, 6.0), Fill::Working(0.5), "Converting", Color32::WHITE);
        });
        harness.run_steps(2);
        let bars: Vec<(Option<String>, Option<f64>)> = harness
            .query_all_by_role(Role::ProgressIndicator)
            .map(|node| (node.accesskit_node().label(), node.accesskit_node().numeric_value()))
            .collect();
        let said = |text: &str, value| (Some(text.to_owned()), value);
        let expected = [
            said("300 KiB of 256 KiB", Some(100.0)),
            said("Encoding", Some(25.0)),
            said("Reading", None),
            said("Converting", Some(50.0)),
        ];
        assert_eq!(bars, expected);
    }

    #[test]
    fn eases_a_bar_up_and_jumps_it_down_or_whole() {
        // Track displayed bar fill across 60 Hz frames.
        type State = (Fill, Option<f32>);
        let show = |ui: &mut egui::Ui, (fill, shown): &mut State| {
            *shown = shown_share(ui.ctx(), egui::Id::new("bar"), *fill);
            bar(ui, *fill, "", Color32::RED);
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(120)
            .build_ui_state(show, (Fill::Working(0.2), None));
        harness.run_steps(2);
        assert_eq!(harness.state().1, Some(0.2));
        harness.state_mut().0 = Fill::Working(0.8);
        harness.step();
        // Eases gradually from the previous value on the first frame.
        let first = harness.state().1.unwrap();
        assert!(first > 0.2 && first < 0.35, "{first}");
        // Reaches ~1 - 1/e after one time constant and converges to the target.
        harness.run_steps(6);
        let mid = harness.state().1.unwrap();
        let expected = 0.8 - 0.6 * (-7.0 / 60.0 / FILL_EASING).exp();
        assert!((mid - expected).abs() < 0.03, "{mid}, not {expected}");
        harness.run_steps(60);
        assert_eq!(harness.state().1, Some(0.8));
        // Drops immediately when a new encode starts at a lower share.
        harness.state_mut().0 = Fill::Working(0.05);
        harness.step();
        assert_eq!(harness.state().1, Some(0.05));
        // Snaps immediately to 1.0 when complete and stops requesting repaints.
        harness.state_mut().0 = Fill::Share(1.0);
        harness.step();
        assert_eq!(harness.state().1, Some(1.0));
        harness.try_run().expect("still once there");
        // Static `Fill::Share` eases upwards and then settles.
        harness.state_mut().0 = Fill::Share(0.3);
        harness.step();
        harness.state_mut().0 = Fill::Share(0.6);
        harness.try_run().expect("still once there");
        assert_eq!(harness.state().1, Some(0.6));
        // With reduced motion, jumps immediately to the target share.
        crate::motion::set(&harness.ctx, false);
        harness.state_mut().0 = Fill::Share(0.9);
        harness.step();
        assert_eq!(harness.state().1, Some(0.9));
        // Indeterminate bar with reduced motion draws static hatching without repaints.
        harness.state_mut().0 = Fill::Unknown;
        harness.try_run().expect("still once there");
    }

    #[test]
    fn eases_the_band_over_a_bar_at_work_in_and_out() {
        assert_eq!(sheen_at(0.0), 0.0);
        // Slow at the start, halfway at the midpoint, fast across the middle.
        assert!(sheen_at(SHEEN * 0.1) < 0.05);
        assert!((sheen_at(SHEEN * 0.5) - 0.5).abs() < 1e-4);
        assert!(sheen_at(SHEEN * 0.55) - sheen_at(SHEEN * 0.45) > 0.14);
        assert!(sheen_at(SHEEN * 0.99) > 0.99);
        // Wraps around for the next cycle.
        assert!(sheen_at(SHEEN * 1.1) < 0.05);
    }
}

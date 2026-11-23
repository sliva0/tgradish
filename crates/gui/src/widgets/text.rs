//! Text measuring, and keeping widths and line breaks steady while the
//! numbers in a text change.

use eframe::egui::{self, RichText};

/// Measures `text` width in the button font plus `padding`.
pub(crate) fn text_width(ui: &egui::Ui, text: &str, padding: f32) -> f32 {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    galley.size().x + padding
}

/// Returns the index of the first string in `texts` (ordered longest to
/// shortest) that fits within `room` with `padding` in the button font,
/// allowing labels to switch to a shorter form before truncating.
pub(crate) fn fitting(ui: &egui::Ui, texts: &[&str], room: f32, padding: f32) -> Option<usize> {
    texts.iter().position(|text| text_width(ui, text, padding) <= room)
}

/// Sizes number fields in `add` to fit `digits` tabular digits plus `suffix`
/// so their width stays fixed while typing or dragging.
pub(crate) fn steady_numbers<R>(
    ui: &mut egui::Ui,
    digits: usize,
    suffix: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    steady_width(ui, &("0".repeat(digits) + suffix), add)
}

/// Sizes number fields in `add` to fit at least `widest` so they and adjacent
/// widgets do not shift while the number changes.
pub(crate) fn steady_width<R>(
    ui: &mut egui::Ui,
    widest: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        // Add 1 pt because pixel-snapped glyphs can render slightly wider than
        // their measured advance.
        ui.spacing_mut().interact_size.x =
            text_width(ui, widest, 2.0 * ui.spacing().button_padding.x).ceil() + 1.0;
        add(ui)
    })
    .inner
}

/// Breaks `text` at the line breaks of the widest template in `widest` (laid
/// out 3 pt narrower to allow for kerning differences next to non-zero digits).
pub(super) fn steady_text(ui: &egui::Ui, text: String, widest: &[String], width: f32) -> String {
    let lay_out = |text: &str| {
        egui::WidgetText::from(RichText::new(text).weak().size(12.5)).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            width - 3.0,
            egui::TextStyle::Body,
        )
    };
    let widest = widest
        .iter()
        .map(|widest| lay_out(widest))
        .max_by(|a, b| a.size().x.total_cmp(&b.size().x));
    match widest {
        Some(widest) => broken_as(&text, widest.text(), &breaks(&widest)),
        None => text,
    }
}

/// Returns the character offsets where wrapped lines end in `galley`.
fn breaks(galley: &egui::Galley) -> Vec<usize> {
    let mut at = 0;
    let mut breaks = Vec::new();
    let rows = galley.rows.len();
    for (index, row) in galley.rows.iter().enumerate() {
        at += row.char_count_including_newline().0;
        if index + 1 < rows && !row.ends_with_newline {
            breaks.push(at);
        }
    }
    breaks
}

/// Inserts newlines into `text` at the same positions relative to its shared
/// prefix and suffix with `like` as the character offsets in `breaks` (placing
/// any break that falls inside the differing number immediately after it).
fn broken_as(text: &str, like: &str, breaks: &[usize]) -> String {
    let (ours, theirs): (Vec<char>, Vec<char>) = (text.chars().collect(), like.chars().collect());
    let before = ours.iter().zip(&theirs).take_while(|(a, b)| a == b).count();
    let shorter = ours.len().min(theirs.len()) - before;
    let after =
        ours.iter().rev().zip(theirs.iter().rev()).take_while(|(a, b)| a == b).count().min(shorter);
    let mut at: Vec<usize> = breaks
        .iter()
        .map(|&at| {
            if at <= before {
                at
            } else if at >= theirs.len() - after {
                ours.len() - (theirs.len() - at)
            } else {
                ours.len() - after
            }
        })
        .collect();
    at.dedup();
    let mut broken = String::with_capacity(text.len() + at.len());
    for (index, c) in ours.iter().enumerate() {
        if at.contains(&index) && index > 0 {
            // Strip trailing space before an inserted line break just as
            // soft wrapping does.
            if broken.ends_with(' ') {
                broken.pop();
            }
            broken.push('\n');
            if *c == ' ' {
                continue;
            }
        }
        broken.push(*c);
    }
    broken
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{fixed, fps, seconds, speed, test_fonts, widest_seconds};

    #[test]
    fn breaks_lines_where_the_widest_number_would() {
        let like = "Plays all 00.00 s: Telegram is told";
        // Line breaks after "all" and after "s:" in the widest template.
        let breaks = [10, 19];
        let text = "Plays all 9.55 s: Telegram is told";
        assert_eq!(broken_as(text, like, &breaks), "Plays all\n9.55 s:\nTelegram is told");
        // A break falling inside the number itself is placed right after the number.
        assert_eq!(broken_as("a 1.5 b", "a 00.00 b", &[4]), "a 1.5\nb");
        // The widest template itself breaks at the exact offsets.
        assert_eq!(broken_as(like, like, &breaks), "Plays all\n00.00 s:\nTelegram is told");
        assert_eq!(fixed(2.0, 2), "2.00");
        assert_eq!((speed(1.5), fps(29.97)), ("1.50".to_owned(), "29.97".to_owned()));
        assert_eq!(widest_seconds(14.55), ["00.00 s"]);
        assert_eq!(widest_seconds(145.5), ["00.00 s", "0:00.00"]);
    }

    #[test]
    fn keeps_a_hint_on_its_lines_as_its_number_changes() {
        type State = (bool, bool);
        // At each width, record the line count and the line containing "told"
        // across different formatted durations.
        let show = |ui: &mut egui::Ui, (steady, plain): &mut State| {
            let widest = widest_seconds(145.5);
            let hint = |time: &str| format!("Plays all {time}: Telegram is told it is shorter");
            let widest: Vec<String> = widest.iter().map(|most| hint(most)).collect();
            for width in 120..320 {
                let lines = |text: String| {
                    let galley = egui::WidgetText::from(RichText::new(text).weak().size(12.5))
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Wrap),
                            width as f32,
                            egui::TextStyle::Body,
                        );
                    let on = galley.rows.iter().position(|row| row.text().contains("told"));
                    (galley.rows.len(), on)
                };
                let times = [0.5, 9.99, 14.55, 145.5].map(|time| hint(&seconds(time)));
                let kept =
                    times.clone().map(|text| lines(steady_text(ui, text, &widest, width as f32)));
                *steady &= kept.iter().all(|each| *each == kept[0]);
                let wrapped = times.map(lines);
                *plain &= wrapped.iter().all(|each| *each == wrapped[0]);
            }
        };
        let mut harness = egui_kittest::Harness::new_ui_state(show, (true, true));
        test_fonts(&harness);
        harness.run_steps(2);
        let (steady, plain) = *harness.state();
        // Plain wrapping shifts words across lines at some widths when the
        // digit count changes; `steady_text` never does.
        assert!(steady && !plain, "steady {steady}, plain {plain}");
    }
}

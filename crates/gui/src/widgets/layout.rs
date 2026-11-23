//! Settings layout: the spacing scale, section headings, labelled rows, and
//! the captions, notes, warnings and links set among controls.

use eframe::egui::{self, RichText, vec2};

use super::text::steady_text;
use crate::jobs::Problem;
use crate::l10n::fl;
use crate::theme;

/// Section heading font size in the heading weight, one step larger than body
/// text so sections stand out above inner medium-weight headings like Preset.
pub(crate) const HEADING: f32 = 16.0;
/// Dialog title and top-level heading font size.
pub(crate) const TITLE: f32 = 17.0;

/// Formats `text` in Ubuntu Medium at `size`.
pub(crate) fn medium(ui: &egui::Ui, text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(crate::fonts::medium_in(ui.ctx())).size(size).strong()
}

// Settings follow a single visual spacing scale:
// - 4 pt between a control and its caption
// - 8 pt between controls within a setting
// - 12 pt between settings
// - 16 pt above a section rule, 12 pt below the rule to its heading, and 8 pt
//   below the heading to its content.
// The constants below are adjusted for text's own vertical padding.

/// Vertical spacing between items within a setting.
const WITHIN: f32 = 8.0;
/// Extra space added after a setting row.
const AFTER_ROW: f32 = 4.0;
/// Space before a caption and the vertical trim subtracted from its bottom
/// line height.
const BEFORE_HINT: f32 = -4.0;
const HINT_TRIM: f32 = 1.0;
/// Space before a section rule, after the rule, and after the section heading.
const BEFORE_RULE: f32 = 4.0;
const AFTER_RULE: f32 = -5.0;
const AFTER_HEADING: f32 = -8.0;
/// Maximum caption width so long hints wrap into a readable block.
const HINT_WIDTH: f32 = 460.0;

/// Applies standard settings vertical spacing to `ui`.
pub(crate) fn settings_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.y = WITHIN;
}

/// Draws a section heading over a group of settings, preceded by a divider rule
/// unless it is the first item in its column.
pub(crate) fn section(ui: &mut egui::Ui, title: &str) {
    section_with(ui, title, |_| {});
}

/// Like [`section`], with additional widgets placed beside the title by
/// `beside`.
pub(crate) fn section_with(ui: &mut egui::Ui, title: &str, beside: impl FnOnce(&mut egui::Ui)) {
    section_rule(ui);
    ui.horizontal(|ui| {
        // Wrap onto multiple lines when a narrow column leaves no room.
        ui.add(egui::Label::new(medium(ui, title, HEADING)).wrap());
        beside(ui);
    });
    ui.add_space(AFTER_HEADING);
}

/// Draws a horizontal divider rule above a section unless it is at the top of
/// its column.
pub(crate) fn section_rule(ui: &mut egui::Ui) {
    // Nothing above it yet.
    if ui.min_rect().height() < 1.0 {
        return;
    }
    ui.add_space(BEFORE_RULE);
    let (line, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        line.x_range(),
        line.center().y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    ui.add_space(AFTER_RULE);
}

/// Draws a collapsible section with the standard section heading and a leading
/// disclosure triangle.
pub(crate) fn folding_section<R>(
    ui: &mut egui::Ui,
    title: &str,
    id: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    section_rule(ui);
    let heading = medium(ui, title, HEADING);
    let folding = egui::CollapsingHeader::new(heading).id_salt(id).show(ui, |ui| {
        ui.add_space(AFTER_HEADING);
        contents(ui)
    });
    folding.body_returned
}

/// Draws a hyperlink that opens on click rather than starting text selection.
pub(crate) fn link(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
    url: impl ToString,
) -> egui::Response {
    let selectable = ui.style().interaction.selectable_labels;
    ui.style_mut().interaction.selectable_labels = false;
    let response = ui.add(egui::Hyperlink::from_label_and_url(text, url));
    ui.style_mut().interaction.selectable_labels = selectable;
    response
}

/// Draws explanatory weak text that remains at full weak-text opacity even
/// inside a disabled scope, since it often explains why the control is
/// disabled.
pub(crate) fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    let text = RichText::new(text.into()).weak().size(12.5);
    if ui.is_enabled() {
        ui.label(text);
        return;
    }
    weak_text(ui, text, ui.available_width(), 0.0);
}

/// Lays out `text` within `width` and draws it at full weak-text opacity
/// regardless of disabled dimming, trimming `trim` points below the last line.
fn weak_text(ui: &mut egui::Ui, text: RichText, width: f32, trim: f32) -> egui::Response {
    let weak = ui.visuals().weak_text_color();
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Body,
    );
    let size = (galley.size() - vec2(0.0, trim)).max(egui::Vec2::ZERO);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    // Expose as a label to screen readers.
    let enabled = ui.is_enabled();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, galley.text()));
    let mut painter = ui.painter().clone();
    painter.set_opacity(1.0);
    painter.galley(rect.min, galley, weak);
    response
}

/// Draws a caption [`note`] tucked beneath a control and capped at
/// [`HINT_WIDTH`].
pub(crate) fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add_space(BEFORE_HINT);
    let text = RichText::new(text.into()).weak().size(12.5);
    let width = ui.available_width().min(HINT_WIDTH);
    weak_text(ui, text, width, HINT_TRIM);
}

/// Reserves the vertical space that [`hint`] would occupy for `text` without
/// drawing it, preventing controls below from shifting when a conditional hint
/// appears or disappears.
pub(crate) fn hint_room(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add_space(BEFORE_HINT);
    let text = RichText::new(text.into()).weak().size(12.5);
    let width = ui.available_width().min(HINT_WIDTH);
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Body,
    );
    ui.allocate_space((galley.size() - vec2(0.0, HINT_TRIM)).max(egui::Vec2::ZERO));
}

/// Draws a [`hint`] inside vertical space reserved for the tallest candidate
/// in `forms` at the current width so controls below stay still as the active
/// option changes.
pub(crate) fn hint_in_room(ui: &mut egui::Ui, text: impl Into<String>, forms: &[String]) {
    ui.add_space(BEFORE_HINT);
    let width = ui.available_width().min(HINT_WIDTH);
    let tallest = forms.iter().fold(0.0_f32, |tallest, form| {
        let galley = egui::WidgetText::from(RichText::new(form).weak().size(12.5)).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            width,
            egui::TextStyle::Body,
        );
        tallest.max(galley.size().y - HINT_TRIM)
    });
    let said = weak_text(ui, RichText::new(text.into()).weak().size(12.5), width, HINT_TRIM);
    let short = tallest - said.rect.height();
    if short > 0.0 {
        ui.add_space(short);
    }
}

/// Draws a [`hint`] containing a live number, breaking lines at the same
/// positions as the widest template in `widest` so words do not jump between
/// lines as the digit count changes.
pub(crate) fn steady_hint(ui: &mut egui::Ui, text: impl Into<String>, widest: &[String]) {
    ui.add_space(BEFORE_HINT);
    let width = ui.available_width().min(HINT_WIDTH);
    let text = steady_text(ui, text.into(), widest, width);
    weak_text(ui, RichText::new(text).weak().size(12.5), width, HINT_TRIM);
}

/// Shows a conversion-blocking warning under a setting with a `fix` action
/// button (with tooltip `hint`), returning `true` when clicked.
pub(crate) fn attention(ui: &mut egui::Ui, text: &str, fix: &str, hint: &str) -> bool {
    attention_explained(ui, text, "", fix, hint)
}

/// Like [`attention`], with an extra hover explanation `why` on the warning
/// label when non-empty.
pub(crate) fn attention_explained(
    ui: &mut egui::Ui,
    text: &str,
    why: &str,
    fix: &str,
    hint: &str,
) -> bool {
    let warning = RichText::new(format!("⚠ {text}")).color(theme::warn(ui.visuals()));
    let said = ui.add(egui::Label::new(warning).wrap());
    if !why.is_empty() {
        said.on_hover_text(why);
    }
    ui.button(fix).on_hover_text(hint).clicked()
}

/// Attaches an accessible button label `name` to an icon-only button
/// `response` so screen readers announce `name` instead of the raw symbol.
pub(crate) fn named(response: egui::Response, name: &str) -> egui::Response {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    response
}

/// Draws a grid label with an optional hover tooltip.
pub(crate) fn label(ui: &mut egui::Ui, text: &str, hint: &str) {
    let label = ui.label(text);
    if !hint.is_empty() {
        label.on_hover_text(hint);
    }
}

/// Styles and adds `button` as the primary action button: accent-filled with
/// contrasting medium-weight text (via [`medium`]) when `enabled`, or weak
/// disabled styling otherwise.
pub(crate) fn primary(
    ui: &mut egui::Ui,
    enabled: bool,
    button: egui::Button<'_>,
) -> egui::Response {
    ui.scope(|ui| {
        if enabled {
            theme::primary(ui.visuals_mut());
        } else {
            theme::primary_disabled(ui.visuals_mut());
        }
        ui.add_enabled(enabled, button)
    })
    .inner
}

/// Creates a single-line text field matching control button height with
/// vertically centred text.
pub(crate) fn text_field(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .min_size(vec2(0.0, theme::CONTROL))
        .vertical_align(egui::Align::Center)
}

/// Width of the left-hand label column in settings rows.
pub(crate) const LABELS: f32 = 150.0;

/// Draws a top-aligned settings row with a left label (and hover `hint`) and
/// right-hand `contents`. When `allowed` is `Err(why)`, the row is disabled and
/// hovering the label shows `why`.
pub(crate) fn row<R>(
    ui: &mut egui::Ui,
    text: &str,
    hint: &str,
    allowed: Result<(), &str>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = ui
        .add_enabled_ui(allowed.is_ok(), |ui| {
            ui.horizontal_top(|ui| {
                // Vertically centre the label against the first line of controls.
                let line = ui.spacing().interact_size.y;
                let layout = egui::Layout::top_down(egui::Align::Min);
                let label = ui
                    .allocate_ui_with_layout(vec2(LABELS, line), layout, |ui| {
                        ui.set_width(LABELS);
                        let text_height = ui.text_style_height(&egui::TextStyle::Body);
                        ui.add_space(((line - text_height) / 2.0).floor().max(0.0));
                        ui.add(egui::Label::new(text).wrap())
                    })
                    .inner;
                let labelled = label.id;
                match allowed {
                    Err(why) => {
                        label.on_disabled_hover_text(why);
                    }
                    Ok(()) if !hint.is_empty() => {
                        label.on_hover_text(hint);
                    }
                    Ok(()) => {}
                }
                // Record the label ID on a stack so unnamed controls inside the
                // row can reference it for accessibility.
                let labels = |ui: &egui::Ui, change: &dyn Fn(&mut Vec<egui::Id>)| {
                    ui.data_mut(|data| change(data.get_temp_mut_or_default(row_labels_id())));
                };
                labels(ui, &|labels| labels.push(labelled));
                let inner = ui.vertical(contents).inner;
                labels(ui, &|labels| {
                    labels.pop();
                });
                inner
            })
            .inner
        })
        .inner;
    ui.add_space(AFTER_ROW);
    inner
}

/// egui memory ID for the stack of active [`row`] label IDs (innermost last).
fn row_labels_id() -> egui::Id {
    egui::Id::new("row labels")
}

/// Returns the label ID of the innermost [`row`] currently being drawn.
pub(super) fn row_label(ui: &egui::Ui) -> Option<egui::Id> {
    ui.data(|data| data.get_temp::<Vec<egui::Id>>(row_labels_id())?.last().copied())
}

/// Displays Telegram validation problems for an output, or an acceptance label
/// when `problems` is empty.
pub(crate) fn problems(ui: &mut egui::Ui, problems: &[Problem]) {
    if problems.is_empty() {
        ui.colored_label(theme::good(ui.visuals()), format!("✔ {}", fl!("telegram-accepts")));
    }
    for problem in problems {
        let (colour, mark) = if problem.refused {
            (theme::bad(ui.visuals()), "✖")
        } else {
            (theme::warn(ui.visuals()), "⚠")
        };
        ui.colored_label(colour, format!("{mark} {}", problem.text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_clicks_on_links_rather_than_selecting() {
        type State = Option<egui::Rect>;
        let show = |ui: &mut egui::Ui, at: &mut State| {
            ui.style_mut().interaction.selectable_labels = true;
            *at = Some(link(ui, "source code", "https://example.org").rect);
        };
        let mut harness = egui_kittest::Harness::new_ui_state(show, None);
        harness.run();
        let at = harness.state().unwrap().center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(at));
        harness.event(press(true));
        harness.step();
        assert_eq!(harness.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand);
        harness.event(press(false));
        harness.step();
        let opened = &harness.output().platform_output.commands;
        assert!(matches!(opened.as_slice(), [egui::OutputCommand::OpenUrl(_)]), "{opened:?}");
    }
}

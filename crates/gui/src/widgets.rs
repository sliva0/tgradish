//! Shared GUI widgets and formatting helpers.

use eframe::egui::{self, Color32, RichText, vec2};

use crate::jobs::Problem;
use crate::l10n::fl;
use crate::theme;

pub(crate) fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

/// Formats seconds as `1:02.50` or `2.50 s` to the nearest hundredth, rounding
/// first so 119.996 becomes `2:00.00` rather than `1:60.00`.
pub(crate) fn seconds(value: f64) -> String {
    let hundredths = (value * 100.0).round();
    if hundredths >= 6000.0 {
        let hundredths = hundredths as u64;
        let (minutes, rest) = (hundredths / 6000, hundredths % 6000);
        format!("{minutes}:{:02}.{:02}", rest / 100, rest % 100)
    } else {
        format!("{value:.2} s")
    }
}

/// Parses seconds typed in [`seconds`] format (`1:02.50` or `2.50 s`) or as a
/// bare number.
pub(crate) fn parse_seconds(text: &str) -> Option<f64> {
    let text = text.trim();
    let text = text.strip_suffix('s').unwrap_or(text).trim();
    let value = match text.split_once(':') {
        Some((minutes, rest)) => {
            let minutes: u32 = minutes.trim().parse().ok()?;
            let rest: f64 = rest.trim().parse().ok()?;
            (0.0..60.0).contains(&rest).then(|| f64::from(minutes) * 60.0 + rest)?
        }
        None => text.parse().ok()?,
    };
    value.is_finite().then_some(value)
}

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

// Live numbers (while dragging, playing, or converting) use a fixed number of
// decimal places so their width stays constant: speeds `×1.50`, seconds
// `2.50 s`, frame rates `29.97 fps`. Ubuntu's digits are tabular with no
// inter-digit kerning, so widths only change when crossing a power of ten
// (9.99 or 99.99); `steady_width` and `steady_hint` reserve room for the
// widest expected value. Static or finished numbers use `trimmed` for cleaner
// reading.

/// Decimal places for speeds and frame rates.
pub(crate) const SPEED_DECIMALS: usize = 2;
pub(crate) const FPS_DECIMALS: usize = 2;

/// Formats `value` with `decimals` fixed decimal places.
pub(crate) fn fixed(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

/// Formats a speed factor without the `×` prefix (`1.50`).
pub(crate) fn speed(value: f64) -> String {
    fixed(value, SPEED_DECIMALS)
}

/// Formats a frame rate without its unit (`29.97`, `30.00`).
pub(crate) fn fps(value: f64) -> String {
    fixed(value, FPS_DECIMALS)
}

/// Replaces every ASCII digit in `text` with `0` to measure the width of any
/// number with the same digit count.
pub(crate) fn zeros(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { '0' } else { c }).collect()
}

/// Returns zero-masked templates for the widest [`seconds`] representations
/// between `0` and `most` (`00.00 s` and, if `most >= 59.995`, `0:00.00`).
pub(crate) fn widest_seconds(most: f64) -> Vec<String> {
    let mut widest = vec![zeros(&seconds(most.clamp(0.0, 59.99)))];
    if most >= 59.995 {
        widest.push(zeros(&seconds(most)));
    }
    widest
}

/// Formats `value` with up to two decimal places, stripping trailing zeros
/// (`60`, `29.97`).
pub(crate) fn trimmed(value: f64) -> String {
    trimmed_to(value, 2)
}

/// Formats `value` with up to `decimals` decimal places, stripping trailing
/// zeros.
pub(crate) fn trimmed_to(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

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

/// Scopes `add` under an ID derived from `(part, file)` and records `file` as
/// the active file being drawn so per-widget state (such as in-progress number
/// edits tracked by [`typed_number`]) stays bound to that file when switching
/// selection.
pub(crate) fn of_file<R>(
    ui: &mut egui::Ui,
    part: &str,
    file: u64,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    of_file_in(ui, egui::UiBuilder::new(), part, file, add)
}

/// Like [`of_file`], using a custom `UiBuilder`.
pub(crate) fn of_file_in<R>(
    ui: &mut egui::Ui,
    place: egui::UiBuilder,
    part: &str,
    file: u64,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let id = egui::Id::new((part, file));
    let drawing = egui::Id::new(DRAWING);
    ui.data_mut(|data| data.insert_temp(drawing, file));
    let added = ui.scope_builder(place.id(id), add).inner;
    ui.data_mut(|data| data.remove::<u64>(drawing));
    added
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

/// Breaks `text` at the line breaks of the widest template in `widest` (laid
/// out 3 pt narrower to allow for kerning differences next to non-zero digits).
fn steady_text(ui: &egui::Ui, text: String, widest: &[String], width: f32) -> String {
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
fn row_label(ui: &egui::Ui) -> Option<egui::Id> {
    ui.data(|data| data.get_temp::<Vec<egui::Id>>(row_labels_id())?.last().copied())
}

/// A single option in a [`segments`] row: `(value, label, hover_hint)`.
pub(crate) type Choice<T> = (T, String, String);

/// Availability of an option in [`segments`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pick {
    Yes,
    /// Disabled, with a tooltip explaining why.
    No(String),
    /// Dimmed because another setting conflicts with it, but still clickable to
    /// switch that setting (explained by the tooltip string).
    Changes(String),
}

/// Draws a segmented button control where `enabled` returns `Ok(())` or an
/// explanation for disabled choices. Returns `true` when the selection changes.
pub(crate) fn segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    enabled: impl Fn(T) -> Result<(), String>,
) -> bool {
    picking(ui, value, choices, |option| match enabled(option) {
        Ok(()) => Pick::Yes,
        Err(why) => Pick::No(why),
    })
}

/// Like [`segments`], taking a [`Pick`] state per option.
pub(crate) fn picking<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
) -> bool {
    let room = ui.available_width();
    picking_in(ui, value, choices, pick, room, true)
}

/// Draws [`segments`] on a single line of width `room` without wrapping,
/// truncating labels that do not fit and showing the full label on hover.
pub(crate) fn segments_on_line<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    enabled: impl Fn(T) -> Result<(), String>,
    room: f32,
) -> bool {
    let pick = |option| match enabled(option) {
        Ok(()) => Pick::Yes,
        Err(why) => Pick::No(why),
    };
    picking_in(ui, value, choices, pick, room, false)
}

/// Horizontal button padding used when segments are tight on space before
/// wrapping or truncating.
const TIGHT_PADDING: f32 = 4.0;

/// Lays out segments of `widths` separated by `gap` within `room`: returns the
/// index range for each line and the maximum width any segment on that line may
/// take (truncating the widest segments first when a line overflows `room`).
fn flow(widths: &[f32], gap: f32, room: f32, wrap: bool) -> Vec<(std::ops::Range<usize>, f32)> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut width = 0.0;
    for (index, each) in widths.iter().enumerate() {
        let wider = width + if index > start { gap } else { 0.0 } + each;
        if wrap && index > start && wider > room {
            lines.push(start..index);
            (start, width) = (index, *each);
        } else {
            width = wider;
        }
    }
    lines.push(start..widths.len());
    lines
        .into_iter()
        .map(|line| {
            // Trim the widest segments first by only as much as needed.
            let gaps = gap * line.len().saturating_sub(1) as f32;
            let mut sorted: Vec<f32> = widths[line.clone()].to_vec();
            sorted.sort_by(f32::total_cmp);
            let mut most = f32::INFINITY;
            let mut left = room - gaps;
            for (index, each) in sorted.iter().enumerate() {
                let share = left / (sorted.len() - index) as f32;
                if *each > share {
                    most = share.floor().max(1.0);
                    break;
                }
                left -= each;
            }
            (line, most)
        })
        .collect()
}

/// Draws [`picking`] within `room`, reducing padding first, then wrapping across
/// lines if `wrap` is true, and finally truncating labels if still too wide.
fn picking_in<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
    room: f32,
    wrap: bool,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        let gap = 1.0;
        ui.spacing_mut().item_spacing = vec2(gap, 4.0);
        let widths = |ui: &egui::Ui, padding: f32| -> Vec<f32> {
            choices.iter().map(|(_, text, _)| text_width(ui, text, 2.0 * padding)).collect()
        };
        let total = |widths: &[f32]| widths.iter().sum::<f32>() + gap * (widths.len() - 1) as f32;
        let mut natural = widths(ui, ui.spacing().button_padding.x);
        if total(&natural) > room {
            let tight = widths(ui, TIGHT_PADDING);
            if total(&tight) <= room || !wrap {
                ui.spacing_mut().button_padding.x = TIGHT_PADDING;
                natural = tight;
            }
        }
        let radius = ui.visuals().widgets.inactive.corner_radius.nw;
        for (line, most) in flow(&natural, gap, room, wrap) {
            ui.horizontal(|ui| {
                for index in line.clone() {
                    let (option, text, hint) = &choices[index];
                    let option = *option;
                    let round = |at_end: bool| if at_end { radius } else { 0 };
                    let (first, last) = (index == line.start, index + 1 == line.end);
                    let corners = egui::CornerRadius {
                        nw: round(first),
                        sw: round(first),
                        ne: round(last),
                        se: round(last),
                    };
                    let selected = *value == option;
                    let button = egui::Button::new(text.as_str()).truncate().corner_radius(corners);
                    let button = as_choice(ui, button, selected);
                    let cut = natural[index] > most;
                    let response = ui
                        .scope(|ui| {
                            if most.is_finite() {
                                ui.set_max_width(most);
                            }
                            match pick(option) {
                                Pick::No(why) => {
                                    ui.add_enabled(false, button).on_disabled_hover_text(why)
                                }
                                Pick::Changes(what) => {
                                    greyed(ui, true, |ui| ui.add(button)).on_hover_text(what)
                                }
                                Pick::Yes if !hint.is_empty() => {
                                    ui.add(button).on_hover_text(hint.as_str())
                                }
                                Pick::Yes => ui.add(button),
                            }
                        })
                        .inner;
                    // Show the full label on hover when truncated.
                    let response =
                        if cut { response.on_hover_text(text.as_str()) } else { response };
                    if response.clicked() && !selected {
                        *value = option;
                        changed = true;
                    }
                }
            });
        }
    });
    changed
}

/// Styles `button` as a choice button (used by [`segments`] and standalone
/// choices): framed at control height, and lit and outlined with the accent
/// when `selected`. Every choice is drawn so.
pub(crate) fn as_choice<'a>(
    ui: &egui::Ui,
    button: egui::Button<'a>,
    selected: bool,
) -> egui::Button<'a> {
    let height = ui.spacing().interact_size.y;
    let button =
        button.selected(selected).frame(true).frame_when_inactive(true).min_size(vec2(0.0, height));
    if selected { button.stroke(theme::on_outline(ui.visuals())) } else { button }
}

/// Draws a standalone choice button styled like a [`segments`] option inside a
/// `horizontal_wrapped` row. If `text` exceeds a full line, it moves to its own
/// line and wraps across multiple lines, truncating (with a full hover tooltip)
/// only if a single word exceeds the line width.
pub(crate) fn choice(
    ui: &mut egui::Ui,
    selected: bool,
    text: &str,
    enabled: bool,
) -> egui::Response {
    let button = as_choice(ui, egui::Button::new(text), selected);
    let line = ui.max_rect().width();
    let padding = 2.0 * ui.spacing().button_padding.x;
    if text_width(ui, text, padding) <= line {
        return ui.add_enabled(enabled, button.wrap_mode(egui::TextWrapMode::Extend));
    }
    // Start a line of its own.
    if ui.cursor().min.x > ui.max_rect().min.x + 0.5 {
        ui.end_row();
    }
    let longest = text.split_whitespace().map(|word| text_width(ui, word, padding));
    let cut = longest.fold(0.0, f32::max) > line;
    let button = if cut { button.truncate() } else { button.wrap() };
    let response = ui
        .scope(|ui| {
            ui.set_max_width(line);
            ui.add_enabled(enabled, button)
        })
        .inner;
    if cut { response.on_hover_text(text) } else { response }
}

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

/// Draws [`segments`] for a setting that can override a preset default `base`.
pub(crate) fn preset_segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    base: T,
    choices: &[Choice<T>],
) -> bool {
    preset_picking(ui, value, base, choices, |_| Pick::Yes)
}

/// Like [`preset_segments`], taking a [`Pick`] state per option.
pub(crate) fn preset_picking<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    base: T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
) -> bool {
    let mut current = value.unwrap_or(base);
    let mut changed = false;
    ui.horizontal(|ui| {
        let room = ui.available_width() - RESET - ui.spacing().item_spacing.x;
        if picking_in(ui, &mut current, choices, pick, room, true) {
            *value = (current != base).then_some(current);
            changed = true;
        }
        if reset(ui, value.is_some()) {
            *value = None;
            changed = true;
        }
    });
    changed
}

/// Draws `contents` with weak inactive text colour when `greyed` is true while
/// keeping it interactive (for options that conflict with another setting but
/// switch it when clicked; hovering shows normal active contrast).
pub(crate) fn greyed<R>(
    ui: &mut egui::Ui,
    greyed: bool,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        if greyed {
            let weak = ui.visuals().weak_text_color();
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.fg_stroke.color = weak;
            widgets.noninteractive.fg_stroke.color = weak;
        }
        contents(ui)
    })
    .inner
}

/// Draws the hint caption for the currently selected option in `choices`.
pub(crate) fn chosen_hint<T: PartialEq + Copy>(ui: &mut egui::Ui, value: T, choices: &[Choice<T>]) {
    if let Some((_, _, hint)) = choices.iter().find(|(option, ..)| *option == value)
        && !hint.is_empty()
    {
        self::hint(ui, hint.as_str());
    }
}

/// Width and height of the slot reserved for a [`reset`] button.
const RESET: f32 = 20.0;

/// Draws a small reset-to-preset button when `shown` is true.
pub(crate) fn reset(ui: &mut egui::Ui, shown: bool) -> bool {
    reset_to(ui, shown, &fl!("back-to-preset"))
}

/// Draws a frameless accent-coloured reset button with tooltip `hint` when
/// `shown` is true, or reserves its space when hidden so adjacent widgets do
/// not shift.
pub(crate) fn reset_to(ui: &mut egui::Ui, shown: bool, hint: &str) -> bool {
    let size = vec2(RESET, RESET);
    if !shown {
        ui.allocate_exact_size(size, egui::Sense::hover());
        return false;
    }
    let glyph = RichText::new("⟲").size(13.0).color(theme::accent_text(ui.visuals()));
    let button = ui.add(egui::Button::new(glyph).small().frame(false).min_size(size));
    named(button, hint).on_hover_text(hint).clicked()
}

/// Draws a checkbox for a boolean setting that can override a preset default
/// `base`, paired with a reset button.
pub(crate) fn flag(
    ui: &mut egui::Ui,
    value: &mut Option<bool>,
    base: bool,
    text: &str,
    hint: &str,
) -> bool {
    let mut checked = value.unwrap_or(base);
    let mut changed = false;
    ui.horizontal(|ui| {
        let response = ui.checkbox(&mut checked, text);
        let response = if hint.is_empty() { response } else { response.on_hover_text(hint) };
        if response.changed() {
            *value = (checked != base).then_some(checked);
            changed = true;
        }
        if reset(ui, value.is_some()) {
            *value = None;
            changed = true;
        }
    });
    changed
}

/// Draws a segmented toggle between an automatic (`None`) mode (whose label
/// and hover explanation come from `automatic`) and a custom (`Some(T)`)
/// number field beside it. When automatic, the number field is read-only and
/// displays `shown` (or an em-dash if `None`); switching to Custom initialises
/// the value from `shown.unwrap_or(start)`.
pub(crate) fn auto_number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    automatic: (&str, &str),
    shown: Option<T>,
    start: T,
    edit: impl FnOnce(Number<'_>) -> Number<'_>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut custom = value.is_some();
        let choices = [
            (false, automatic.0.to_owned(), automatic.1.to_owned()),
            (true, fl!("custom"), String::new()),
        ];
        if segments(ui, &mut custom, &choices, |_| Ok(())) {
            *value = custom.then(|| shown.unwrap_or(start));
            changed = true;
        }
        let set = value.is_some();
        // When automatic or disabled, display `shown` unclamped (since an
        // automatic value can lie outside the custom range) and ignore changes
        // so clamping does not mark it modified.
        let live = set && ui.is_enabled();
        let mut number = value.or(shown).unwrap_or(start);
        let mut field = edit(Number::new(&mut number)).clamp_existing_to_range(live);
        // Show an em-dash without units when the automatic value is unknown.
        if !set && shown.is_none() {
            field = field.custom_formatter(|_, _| "—".to_owned()).prefix("").suffix("");
        }
        let response = number_field(ui, set, field).on_disabled_hover_text(fl!("custom", "hint"));
        if live && response.changed() {
            *value = Some(number);
            changed = true;
        }
    });
    changed
}

/// Draws `field` as an interactive box when `set` is true, or as frameless weak
/// text when `set` is false so only editable numbers appear boxed.
pub(crate) fn number_field(ui: &mut egui::Ui, set: bool, field: Number<'_>) -> egui::Response {
    if set {
        return ui.add(field);
    }
    ui.scope(|ui| {
        let visuals = ui.visuals_mut();
        visuals.override_text_color = visuals.weak_text_color;
        let widgets = &mut visuals.widgets;
        for state in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            state.bg_stroke = egui::Stroke::NONE;
            (state.bg_fill, state.weak_bg_fill) = (Color32::TRANSPARENT, Color32::TRANSPARENT);
        }
        // Keep full weak-text opacity unless the surrounding scope is disabled.
        visuals.disabled_alpha = 1.0;
        ui.add_enabled(false, field)
    })
    .inner
}

// All numeric inputs use `Number` for consistent typing, arrow-key stepping,
// dragging, and scroll-wheel handling. Typed text is committed only when editing
// finishes (Enter, Tab, or clicking outside) so dependent values (such as crop
// dimensions coupled to an aspect ratio) do not react to intermediate digits
// like `1` on the way to `16`, and Escape can restore the initial value.

/// Maximum drag distance in points needed to scrub across a finite range,
/// accelerating drag speed on wide ranges.
const SCRUB_ACROSS: f64 = 400.0;
/// Duration in seconds for which a number field flashes an error outline after
/// rejecting invalid text input.
const REFUSED_FOR: f64 = 0.6;

/// Custom stepping callback `(current_value, step_count, large_step) -> next_value`.
type Stepper<'a> = Box<dyn 'a + Fn(f64, f64, bool) -> f64>;
type Formatter<'a> = Box<dyn 'a + Fn(f64, std::ops::RangeInclusive<usize>) -> String>;
type Parser<'a> = Box<dyn 'a + Fn(&str) -> Option<f64>>;

/// Numeric input widget similar to [`egui::DragValue`] that commits typed edits
/// only on completion and restores the initial value on Escape.
///
/// While focused for typing, Up/Down arrow keys and the scroll wheel step the
/// value by [`steps`](Number::steps) (or the larger step with Shift). Dragging
/// traverses the full range within at most [`SCRUB_ACROSS`] points (ten times
/// slower with Shift). Accepts typed units and decimal commas; invalid input is
/// discarded and flashes the error outline.
pub(crate) struct Number<'a> {
    get_set: Box<dyn 'a + FnMut(Option<f64>) -> f64>,
    integral: bool,
    range: std::ops::RangeInclusive<f64>,
    clamp_existing: bool,
    speed: f64,
    steps: (f64, f64),
    stepper: Option<Stepper<'a>>,
    prefix: String,
    suffix: String,
    /// Additional unit suffix accepted when parsing typed text even though not
    /// displayed.
    reads: &'static str,
    max_decimals: Option<usize>,
    formatter: Option<Formatter<'a>>,
    parser: Option<Parser<'a>>,
    /// Explicit screen-reader label; falls back to the enclosing [`row`] label.
    name: Option<String>,
}

/// In-progress text editing state for a focused [`Number`] field.
#[derive(Clone, Debug, Default)]
struct Typing {
    text: String,
    /// Formatted string produced from the initial/stepped value; if `text` still
    /// matches `written`, no edit is committed.
    written: String,
    /// Value when editing began, restored on Escape.
    start: f64,
    /// Value corresponding to `written`; if the underlying value changes
    /// externally, `text` is refreshed from the new value.
    value: f64,
    /// Accumulated fractional scroll-wheel notches.
    wheel: f32,
    /// Last pass in which the field held focus; stale edits from fields that
    /// scrolled or switched out of view are discarded.
    pass: u64,
}

/// egui temp-data keys: the number field being typed in (with its file), the
/// pass in which [`take_typed_number`] asks it to commit, and the file being
/// drawn (set by [`of_file`]).
const TYPED: &str = "number typed";
const TAKE_TYPED: &str = "take number typed";
const DRAWING: &str = "file drawn";
/// Pass number in which a number field last committed or rejected typed input.
const TAKEN: &str = "number taken";

/// Identifies the most recently edited number field and the file whose settings
/// it belongs to, so pending text can be committed before switching away from
/// that file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Typed {
    pub(crate) field: egui::Id,
    pub(crate) file: Option<u64>,
}

/// Returns `true` if a [`Number`] field currently has keyboard focus for typing.
pub(crate) fn typing_number(ctx: &egui::Context) -> bool {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
    typed.is_some_and(|typed| ctx.memory(|memory| memory.has_focus(typed.field)))
}

/// Returns the file ID owning an uncommitted [`Number`] text edit, or `None` if
/// no edit is pending (or the field has been off-screen for more than two
/// passes).
pub(crate) fn typed_number(ctx: &egui::Context) -> Option<u64> {
    draft(ctx).and_then(|typed| typed.file)
}

/// Returns the [`Typed`] record for a field with uncommitted text modifications.
fn draft(ctx: &egui::Context) -> Option<Typed> {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)))?;
    let typing = ctx.data(|data| data.get_temp::<Typing>(typed.field))?;
    // Discard stale state if the field has not been drawn recently.
    if ctx.cumulative_pass_nr() > typing.pass + 2 {
        ctx.data_mut(|data| data.remove::<Typing>(typed.field));
        return None;
    }
    (typing.text != typing.written).then_some(typed)
}

/// egui temp-data key recording the pass on which a deferred user action (such
/// as committing a typed number or applying a deferred stepper click) ran.
const ACTED: &str = "acted for the user";

/// Returns `true` if a deferred user action completed during the current pass.
pub(crate) fn acted_now(ctx: &egui::Context) -> bool {
    let acted = ctx.data(|data| data.get_temp::<u64>(egui::Id::new(ACTED)));
    acted == Some(ctx.cumulative_pass_nr())
}

/// Returns `true` if a number field committed or rejected typed input during
/// the current pass.
pub(crate) fn number_taken_now(ctx: &egui::Context) -> bool {
    let taken = ctx.data(|data| data.get_temp::<u64>(egui::Id::new(TAKEN)));
    taken == Some(ctx.cumulative_pass_nr())
}

/// Returns `true` if a number field has uncommitted text or committed it on
/// this pass, allowing another control clicked at the same time to let the
/// typed edit commit as its own undo step first.
fn number_typed_now(ctx: &egui::Context) -> bool {
    draft(ctx).is_some() || number_taken_now(ctx)
}

/// Surrenders focus from any active number field so its typed value is committed
/// when drawn this frame.
pub(crate) fn finish_typed_number(ctx: &egui::Context) {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
    if let Some(typed) = typed {
        ctx.memory_mut(|memory| memory.surrender_focus(typed.field));
    }
}

/// Discards any uncommitted text in the active number field without applying it
/// (used when its file is removed).
pub(crate) fn forget_typed_number(ctx: &egui::Context) {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
    if let Some(typed) = typed {
        ctx.memory_mut(|memory| memory.surrender_focus(typed.field));
        ctx.data_mut(|data| data.remove::<Typing>(typed.field));
    }
}

/// Requests that the currently focused number field commit its typed text this
/// frame (like Enter) while keeping keyboard focus, for shortcuts that
/// immediately use the updated value.
pub(crate) fn take_typed_number(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(TAKE_TYPED), pass));
}

impl<'a> Number<'a> {
    pub(crate) fn new<T: egui::emath::Numeric>(value: &'a mut T) -> Self {
        let get_set = move |set: Option<f64>| {
            if let Some(set) = set {
                *value = T::from_f64(set);
            }
            value.to_f64()
        };
        Number {
            get_set: Box::new(get_set),
            integral: T::INTEGRAL,
            range: f64::NEG_INFINITY..=f64::INFINITY,
            clamp_existing: false,
            speed: 1.0,
            steps: (1.0, 10.0),
            stepper: None,
            prefix: String::new(),
            suffix: String::new(),
            reads: "",
            max_decimals: None,
            formatter: None,
            parser: None,
            name: None,
        }
    }

    /// Sets an explicit screen-reader label when the enclosing [`row`] label is
    /// insufficient or absent.
    pub(crate) fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub(crate) fn range<N: egui::emath::Numeric>(
        mut self,
        range: std::ops::RangeInclusive<N>,
    ) -> Self {
        self.range = range.start().to_f64()..=range.end().to_f64();
        self
    }

    /// Controls whether out-of-range values are clamped immediately when
    /// displayed rather than only when edited.
    pub(crate) fn clamp_existing_to_range(mut self, clamp: bool) -> Self {
        self.clamp_existing = clamp;
        self
    }

    /// Sets the minimum drag increment per point (which also determines default
    /// decimal precision for floating-point fields, matching egui).
    pub(crate) fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// Sets the step increments for Up/Down keys and the scroll wheel (`small`,
    /// and `large` with Shift; defaults to `1` and `10`).
    pub(crate) fn steps(mut self, small: f64, large: f64) -> Self {
        self.steps = (small, large);
        self
    }

    /// Overrides fixed step increments with a custom stepping function
    /// `(value, steps, large) -> next_value` (e.g. stepping by frames or whole
    /// seconds).
    pub(crate) fn stepper(mut self, stepper: impl 'a + Fn(f64, f64, bool) -> f64) -> Self {
        self.stepper = Some(Box::new(stepper));
        self
    }

    pub(crate) fn prefix(mut self, prefix: impl ToString) -> Self {
        self.prefix = prefix.to_string();
        self
    }

    pub(crate) fn suffix(mut self, suffix: impl ToString) -> Self {
        self.suffix = suffix.to_string();
        self
    }

    /// Accepts `unit` when parsing typed input even though it is not shown in
    /// the display format.
    pub(crate) fn reads(mut self, unit: &'static str) -> Self {
        self.reads = unit;
        self
    }

    pub(crate) fn max_decimals(mut self, decimals: usize) -> Self {
        self.max_decimals = Some(decimals);
        self
    }

    pub(crate) fn custom_formatter(
        mut self,
        formatter: impl 'a + Fn(f64, std::ops::RangeInclusive<usize>) -> String,
    ) -> Self {
        self.formatter = Some(Box::new(formatter));
        self
    }

    /// Sets a custom parser for typed input after units and whitespace have
    /// been stripped and decimal commas normalised to dots.
    pub(crate) fn custom_parser(mut self, parser: impl 'a + Fn(&str) -> Option<f64>) -> Self {
        self.parser = Some(Box::new(parser));
        self
    }

    fn get(&mut self) -> f64 {
        (self.get_set)(None)
    }

    fn set(&mut self, value: f64) {
        (self.get_set)(Some(value));
    }

    fn clamped(&self, value: f64) -> f64 {
        let (low, high) = (*self.range.start(), *self.range.end());
        value.clamp(low.min(high), high.max(low))
    }

    /// Rounds `value` if integral and clamps it to the configured range.
    fn kept(&self, value: f64) -> f64 {
        self.clamped(if self.integral { value.round() } else { value })
    }

    /// Steps `value` by `steps` (using the large increment when `large` is
    /// true).
    fn stepped(&self, value: f64, steps: f64, large: bool) -> f64 {
        let next = match &self.stepper {
            Some(stepper) => stepper(value, steps, large),
            None => value + steps * if large { self.steps.1 } else { self.steps.0 },
        };
        // Strip floating-point drift from decimal steps such as 0.30000000000000004.
        self.kept(egui::emath::round_to_decimals(next, 9))
    }

    /// Drag increment per point.
    fn scrub_speed(&self) -> f64 {
        let span = self.range.end() - self.range.start();
        // Ignore unbounded or huge default ranges.
        match span.is_finite() && span < 1e9 {
            true => self.speed.max(span / SCRUB_ACROSS),
            false => self.speed,
        }
    }

    /// Returns the decimal-place range to display: always `0..=0` for integer
    /// types (regardless of `speed` or UI scale below 1.0 where a point exceeds
    /// a pixel), and derived from `aim_radius() / speed` otherwise.
    fn decimals(&self, ui: &egui::Ui) -> std::ops::RangeInclusive<usize> {
        if self.integral {
            return 0..=0;
        }
        let aim = f64::from(ui.input(|input| input.aim_radius()));
        let auto = (aim / self.speed.abs()).log10().ceil().clamp(0.0, 15.0) as usize;
        let most = self.max_decimals.unwrap_or(auto + 2);
        auto.min(most)..=most
    }

    fn format(&self, ui: &egui::Ui, value: f64) -> String {
        let decimals = self.decimals(ui);
        match &self.formatter {
            Some(formatter) => formatter(value, decimals),
            None => ui.style().number_formatter.format(value, decimals),
        }
    }

    /// Parses typed `text` into a finite number.
    fn parse(&self, text: &str) -> Option<f64> {
        let text = lenient(text, &[self.prefix.trim(), self.suffix.trim(), self.reads.trim()]);
        let value = match &self.parser {
            Some(parser) => parser(&text),
            None => text.parse().ok(),
        };
        value.filter(|value: &f64| value.is_finite())
    }

    /// Commits typed text if it changed from `typing.written`, clamping and
    /// storing valid numbers or flashing the refusal outline on parse failure.
    fn take(&mut self, ui: &egui::Ui, id: egui::Id, typing: &Typing) -> Option<f64> {
        if typing.text == typing.written {
            return None;
        }
        // Record that a user edit was processed this pass (e.g. when focus left
        // the field on the previous frame due to clicking a later control).
        let pass = ui.ctx().cumulative_pass_nr();
        ui.data_mut(|data| {
            data.insert_temp(egui::Id::new(TAKEN), pass);
            data.insert_temp(egui::Id::new(ACTED), pass);
        });
        match self.parse(&typing.text) {
            Some(value) => {
                let value = self.kept(value);
                self.set(value);
                Some(value)
            }
            None => {
                let now = ui.input(|input| input.time);
                ui.data_mut(|data| data.insert_temp(id.with("refused"), now));
                None
            }
        }
    }

    /// Creates a [`Typing`] state initialised with the formatted text of
    /// `value` and initial value `start`.
    fn typing(&self, ui: &egui::Ui, value: f64, start: f64) -> Typing {
        let text = self.format(ui, value);
        Typing { written: text.clone(), text, start, value, ..Typing::default() }
    }

    /// Returns the base value for arrow/wheel stepping while editing: the
    /// currently typed number if valid, otherwise `value`.
    fn typed_or(&self, typing: &Typing, value: f64) -> f64 {
        let typed = (typing.text != typing.written).then(|| self.parse(&typing.text)).flatten();
        typed.map_or(value, |typed| self.kept(typed))
    }

    /// Restores `typing.start` on Escape and consumes the Escape key if the
    /// field had uncommitted text or stepped changes (leaving Escape unconsumed
    /// if nothing changed so an enclosing dialog can close).
    fn put_back(&mut self, ui: &egui::Ui, typing: &Typing, value: f64) -> f64 {
        if typing.text != typing.written || value != typing.start {
            escape_taken(ui);
        }
        self.set(typing.start);
        typing.start
    }
}

/// Normalises typed number text by stripping whitespace (including thousands
/// separators) and leading/trailing `units` (plus `x`/`X` when `×` is a unit),
/// and converting decimal commas to `.` and Unicode minus `−` to `-`.
fn lenient(text: &str, units: &[&str]) -> String {
    let mut text: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            ',' => '.',
            '−' => '-',
            c => c,
        })
        .collect();
    let mut all: Vec<&str> = units.iter().copied().filter(|unit| !unit.is_empty()).collect();
    if all.contains(&"×") {
        all.extend(["x", "X"]);
    }
    // Strip longer units first so `kbit/s` is removed before a bare `s`.
    all.sort_by_key(|unit| std::cmp::Reverse(unit.len()));
    loop {
        let before = text.len();
        for unit in &all {
            let unit_lower = unit.to_lowercase();
            let lower = text.to_lowercase();
            // Ensure byte lengths match after lower-casing (units are ASCII
            // except for `×`).
            if lower.len() != text.len() {
                continue;
            }
            if lower.starts_with(&unit_lower) {
                text = text[unit.len()..].trim().to_owned();
            } else if lower.ends_with(&unit_lower) {
                text = text[..text.len() - unit.len()].trim().to_owned();
            }
        }
        if text.len() == before {
            return text;
        }
    }
}

/// Returns `(plain, shifted)` scroll-wheel notches turned this frame (positive
/// upwards), ignoring Ctrl/Command zoom scrolls.
fn notches(ui: &egui::Ui) -> (f32, f32) {
    let line = ui.ctx().options(|options| options.input_options.line_scroll_speed);
    ui.input(|input| {
        let (mut plain, mut shifted) = (0.0, 0.0);
        for event in &input.raw.events {
            let egui::Event::MouseWheel { unit, delta, modifiers, .. } = event else { continue };
            if modifiers.command || modifiers.ctrl {
                continue;
            }
            // Some platforms translate Shift+wheel into horizontal scrolling.
            let turned = if modifiers.shift { delta.x + delta.y } else { delta.y };
            let turned = match unit {
                egui::MouseWheelUnit::Line => turned,
                egui::MouseWheelUnit::Point => turned / line.max(1.0),
                egui::MouseWheelUnit::Page => turned.signum(),
            };
            match modifiers.shift {
                true => shifted += turned,
                false => plain += turned,
            }
        }
        (plain, shifted)
    })
}

/// Consumes the Escape key press for this frame so an enclosing dialog or view
/// does not also react to it after a field reverted its edit.
pub(crate) fn escape_taken(ui: &egui::Ui) {
    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
}

/// Selects all characters of `text` in `TextEdit` `id` when editing starts so
/// newly typed characters replace the old value.
fn select_all(ui: &egui::Ui, id: egui::Id, text: &str) {
    use egui::text::{CCursor, CCursorRange};
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    let all = CCursorRange::two(CCursor::default(), CCursor::new(text.chars().count()));
    state.cursor.set_char_range(Some(all));
    state.store(ui.ctx(), id);
}

/// Moves the `TextEdit` cursor for `id` to the end of `text` after a step.
fn cursor_to_end(ui: &egui::Ui, id: egui::Id, text: &str) {
    use egui::text::{CCursor, CCursorRange};
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(text.chars().count()))));
    state.store(ui.ctx(), id);
}

impl egui::Widget for Number<'_> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        use egui::accesskit::{Action, ActionData};
        // Keep the same ID whether in text-edit or button mode, matching egui.
        let id = ui.next_auto_id();
        let enabled = ui.is_enabled();
        let editing = enabled
            && ui.memory_mut(|memory| {
                memory.interested_in_focus(id, ui.layer_id());
                memory.has_focus(id)
            });
        let old = self.get();
        let mut value = old;
        let (asked, steps_asked) = ui.input(|input| {
            let set = input.accesskit_action_requests(id, Action::SetValue).filter_map(|asked| {
                match &asked.data {
                    Some(ActionData::NumericValue(value)) => Some(*value),
                    _ => None,
                }
            });
            let steps = input.num_accesskit_action_requests(id, Action::Increment) as f64
                - input.num_accesskit_action_requests(id, Action::Decrement) as f64;
            (set.last(), steps)
        });
        if let Some(asked) = asked {
            value = self.kept(asked);
        }
        if self.clamp_existing {
            value = self.clamped(value);
        }
        if steps_asked != 0.0 {
            value = self.stepped(value, steps_asked, false);
        }
        if value != old {
            self.set(value);
        }
        let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
        let typing: Option<Typing> = ui.data(|data| data.get_temp(id));
        // If focus left before this widget was drawn (via Escape, Tab, or
        // another widget taking focus), commit or revert the pending edit unless
        // the value changed externally or the field has been off-screen for
        // more than two passes.
        if !editing && let Some(typing) = &typing {
            ui.data_mut(|data| data.remove::<Typing>(id));
            let lately = ui.ctx().cumulative_pass_nr() <= typing.pass + 2;
            if lately && typing.value == value {
                if escaped {
                    value = self.put_back(ui, typing, value);
                } else if let Some(taken) = self.take(ui, id, typing) {
                    value = taken;
                }
            }
        }
        let text_style = ui.style().drag_value_text_style.clone();
        let mut response = if editing {
            let mut typing = match typing.filter(|typing| typing.value == value) {
                Some(typing) => typing,
                None => {
                    let typing = self.typing(ui, value, value);
                    select_all(ui, id, &typing.text);
                    typing
                }
            };
            // When a shortcut (such as Convert) requests the current typed
            // value, commit it immediately while keeping focus.
            let take_asked = ui.data(|data| data.get_temp::<u64>(egui::Id::new(TAKE_TYPED)))
                == Some(ui.ctx().cumulative_pass_nr());
            if take_asked && typing.text != typing.written {
                if let Some(taken) = self.take(ui, id, &typing) {
                    value = taken;
                }
                typing = self.typing(ui, value, value);
                cursor_to_end(ui, id, &typing.text);
            }
            // Consume Shift+arrow presses before plain arrows because
            // `Modifiers::NONE` matches shifted presses too.
            let (small, large) = ui.input_mut(|input| {
                let mut count = |modifiers| {
                    input.count_and_consume_key(modifiers, egui::Key::ArrowUp) as f64
                        - input.count_and_consume_key(modifiers, egui::Key::ArrowDown) as f64
                };
                let large = count(egui::Modifiers::SHIFT);
                (count(egui::Modifiers::NONE), large)
            });
            if small != 0.0 || large != 0.0 {
                let mut next = self.typed_or(&typing, value);
                if large != 0.0 {
                    next = self.stepped(next, large, true);
                }
                if small != 0.0 {
                    next = self.stepped(next, small, false);
                }
                value = next;
                self.set(value);
                typing = Typing { wheel: typing.wheel, ..self.typing(ui, value, typing.start) };
                cursor_to_end(ui, id, &typing.text);
            }
            let response = ui.add(
                egui::TextEdit::singleline(&mut typing.text)
                    .clip_text(false)
                    .horizontal_align(ui.layout().horizontal_align())
                    .vertical_align(ui.layout().vertical_align())
                    .margin(ui.spacing().button_padding)
                    .min_size(ui.spacing().interact_size)
                    .id(id)
                    .desired_width(
                        ui.spacing().interact_size.x - 2.0 * ui.spacing().button_padding.x,
                    )
                    .font(text_style),
            );
            // Step with the scroll wheel only while the field is focused for
            // typing and under the pointer; otherwise let the wheel scroll the
            // enclosing container normally.
            if response.contains_pointer() {
                let (plain, shifted) = notches(ui);
                ui.input_mut(|input| input.smooth_scroll_delta = egui::Vec2::ZERO);
                typing.wheel += plain + shifted;
                let whole = typing.wheel.trunc();
                if whole != 0.0 {
                    typing.wheel -= whole;
                    let next = self.typed_or(&typing, value);
                    let next = self.stepped(next, f64::from(whole), shifted != 0.0);
                    value = next;
                    self.set(value);
                    typing = Typing { wheel: typing.wheel, ..self.typing(ui, value, typing.start) };
                    cursor_to_end(ui, id, &typing.text);
                    ui.ctx().request_repaint();
                }
            }
            // Surrender focus as soon as the pointer presses outside the field
            // (matching browser inputs) rather than on release, so the typed
            // edit commits as its own step on the frame before the click action
            // (which might switch to another file's settings).
            let elsewhere = ui.input(|input| {
                let pressed = input.pointer.any_pressed();
                let origin = input.pointer.press_origin();
                pressed && origin.is_some_and(|at| !response.interact_rect.contains(at))
            });
            if elsewhere {
                ui.memory_mut(|memory| memory.surrender_focus(id));
            }
            if ui.memory(|memory| memory.has_focus(id)) {
                (typing.value, typing.pass) = (value, ui.ctx().cumulative_pass_nr());
                let file = ui.data(|data| data.get_temp::<u64>(egui::Id::new(DRAWING)));
                ui.data_mut(|data| {
                    data.insert_temp(id, typing);
                    data.insert_temp(egui::Id::new(TYPED), Typed { field: id, file });
                });
            } else {
                // Editing finished via Enter or a pointer press outside.
                ui.data_mut(|data| data.remove::<Typing>(id));
                if escaped {
                    value = self.put_back(ui, &typing, value);
                } else if let Some(taken) = self.take(ui, id, &typing) {
                    value = taken;
                }
            }
            response
        } else {
            let text = format!("{}{}{}", self.prefix, self.format(ui, value), self.suffix);
            let button = egui::Button::new(RichText::new(text).text_style(text_style))
                .wrap_mode(egui::TextWrapMode::Extend)
                .sense(egui::Sense::click_and_drag())
                .min_size(ui.spacing().interact_size);
            let (low, high) = (*self.range.start(), *self.range.end());
            let cursor = if value <= low {
                egui::CursorIcon::ResizeEast
            } else if value < high {
                egui::CursorIcon::ResizeHorizontal
            } else {
                egui::CursorIcon::ResizeWest
            };
            let response = ui.add(button).on_hover_cursor(cursor);
            // Reset the unrounded drag accumulator when a pointer button is
            // pressed or released.
            if ui.input(|input| input.pointer.any_pressed() || input.pointer.any_released()) {
                ui.data_mut(|data| data.remove::<f64>(id));
            }
            if response.clicked() {
                ui.data_mut(|data| data.remove::<Typing>(id));
                ui.memory_mut(|memory| memory.request_focus(id));
            } else if response.dragged() {
                ui.set_cursor_icon(cursor);
                let delta = response.drag_delta();
                // Increase when dragging right or up.
                let points = f64::from(delta.x - delta.y);
                let fine = ui.input(|input| input.modifiers.shift_only());
                let speed = self.scrub_speed() / if fine { 10.0 } else { 1.0 };
                if points != 0.0 {
                    let precise = ui.data(|data| data.get_temp::<f64>(id)).unwrap_or(value);
                    let precise = precise + points * speed;
                    let aim = f64::from(ui.input(|input| input.aim_radius())) * speed;
                    let nice =
                        egui::emath::smart_aim::best_in_range_f64(precise - aim, precise + aim);
                    let decimals = *self.decimals(ui).start();
                    value = self.kept(egui::emath::round_to_decimals(nice, decimals));
                    self.set(value);
                    ui.data_mut(|data| data.insert_temp(id, precise));
                }
            }
            response
        };
        // Flash an inside error outline over the field when invalid input was
        // rejected so layout does not shift.
        let refused = id.with("refused");
        if let Some(at) = ui.data(|data| data.get_temp::<f64>(refused)) {
            let age = ui.input(|input| input.time) - at;
            if (0.0..REFUSED_FOR).contains(&age) {
                // Hold solid for the same duration when reduced motion is active.
                let moving = crate::motion::motion(ui.ctx());
                let fade = if moving { 1.0 - (age / REFUSED_FOR) as f32 } else { 1.0 };
                let colour = theme::bad(ui.visuals()).gamma_multiply(fade);
                let corner = ui.visuals().widgets.inactive.corner_radius;
                let stroke = egui::Stroke::new(1.5, colour);
                ui.painter().rect_stroke(response.rect, corner, stroke, egui::StrokeKind::Inside);
                if moving {
                    ui.ctx().request_repaint();
                } else {
                    let left = std::time::Duration::from_secs_f64(REFUSED_FOR - age);
                    ui.ctx().request_repaint_after(left);
                }
            } else {
                ui.data_mut(|data| data.remove::<f64>(refused));
            }
        }
        if self.get() != old {
            response.mark_changed();
        }
        response.widget_info(|| egui::WidgetInfo::drag_value(enabled, value));
        let shown = format!("{}{}{}", self.prefix, self.format(ui, value), self.suffix);
        let (low, high, step) = (*self.range.start(), *self.range.end(), self.steps.0);
        let row = row_label(ui);
        ui.ctx().accesskit_node_builder(response.id, |builder| {
            if low.is_finite() {
                builder.set_min_numeric_value(low);
            }
            if high.is_finite() {
                builder.set_max_numeric_value(high);
            }
            builder.set_numeric_value_step(step);
            builder.add_action(Action::SetValue);
            if value < high {
                builder.add_action(Action::Increment);
            }
            if value > low {
                builder.add_action(Action::Decrement);
            }
            builder.clear_label();
            // Use the explicit name if set, otherwise reference the enclosing row label.
            match (&self.name, row) {
                (Some(name), _) => builder.set_label(name.as_str()),
                (None, Some(label)) => builder.push_labelled_by(label.accesskit_id()),
                (None, None) => {}
            }
            // Report the formatted value with units when not editing; while
            // typing, leave the text it holds.
            if !editing {
                builder.set_value(shown);
            }
        });
        response
    }
}

/// Hold delay and repeat intervals (initial and accelerated) for [`stepper`]
/// buttons, in seconds.
const REPEAT_AFTER: f64 = 0.4;
const REPEAT_EVERY: f64 = 1.0 / 8.0;
const FASTER_AFTER: f64 = 1.5;
const FASTER_EVERY: f64 = 1.0 / 20.0;

/// Returns the total number of steps a [`stepper`] button held for `held`
/// seconds should have fired (including the initial press).
fn steps_held(held: f64) -> u32 {
    let slow = match held < REPEAT_AFTER {
        true => 0,
        false => {
            let most = ((FASTER_AFTER - REPEAT_AFTER) / REPEAT_EVERY).ceil() as u32;
            (((held - REPEAT_AFTER) / REPEAT_EVERY).floor() as u32 + 1).min(most)
        }
    };
    let fast = match held < FASTER_AFTER {
        true => 0,
        false => ((held - FASTER_AFTER) / FASTER_EVERY).floor() as u32 + 1,
    };
    1 + slow + fast
}

/// Returns the time in seconds after `held` until the next [`stepper`] repeat.
fn next_step(held: f64) -> f64 {
    if held < REPEAT_AFTER {
        REPEAT_AFTER - held
    } else if held < FASTER_AFTER {
        let next =
            REPEAT_AFTER + (((held - REPEAT_AFTER) / REPEAT_EVERY).floor() + 1.0) * REPEAT_EVERY;
        next.min(FASTER_AFTER) - held
    } else {
        FASTER_EVERY - (held - FASTER_AFTER) % FASTER_EVERY
    }
}

/// Draws a square `−` or `+` stepper button labelled `text` with accessible
/// name and tooltip `name`. Returns `(response, stepped)`, firing once on press
/// (or keyboard/screen-reader click) and repeating while held under the pointer
/// (accelerating after [`FASTER_AFTER`]) until released or disabled.
pub(crate) fn stepper(
    ui: &mut egui::Ui,
    text: &str,
    enabled: bool,
    name: &str,
) -> (egui::Response, bool) {
    // Square button matching control row height.
    let side = ui.spacing().interact_size.y;
    // Include drag sense because egui drops click state on long presses.
    let sense = egui::Sense::click_and_drag();
    let button = egui::Button::new(text).min_size(vec2(side, side)).sense(sense);
    let response = named(ui.add_enabled(enabled, button), name);
    let (id, here) = (response.id.with("held"), response.id.with("stepped here"));
    let now = ui.input(|input| input.time);
    // Hold start timestamp and count of steps taken so far.
    let since: Option<(f64, u32)> = ui.data(|data| data.get_temp(id));
    // Skip due steps while the pointer is outside the button (including the
    // frame after a UI-scale step before egui re-hits the button under the
    // pointer).
    let held = enabled && response.is_pointer_button_down_on();
    let over = response.contains_pointer();
    // If clicked while a number field had uncommitted text (e.g. via screen
    // reader), defer the step by one frame so the typed number commits as its
    // own step first, firing only if this stepper remained visible and enabled
    // on the next pass and no new typing started.
    let pending = response.id.with("pending");
    let typed = number_typed_now(ui.ctx());
    let pass = ui.ctx().cumulative_pass_nr();
    let live = enabled && ui.is_enabled();
    let put_off = match ui.data_mut(|data| data.remove_temp::<u64>(pending)) {
        Some(seen) if seen + 1 == pass && live && draft(ui.ctx()).is_none() => {
            if typed {
                ui.data_mut(|data| data.insert_temp(pending, pass));
                ui.ctx().request_repaint();
            }
            !typed
        }
        _ => false,
    };
    let stepped = match (held, since) {
        // Defer the initial press by one frame when a number field is committing
        // typed text so the step applies on top of the committed value.
        (true, None) if typed => {
            ui.data_mut(|data| data.insert_temp(id, (now, 0_u32)));
            false
        }
        (true, None) => {
            if over {
                ui.data_mut(|data| data.insert_temp(id, (now, 1_u32)));
            }
            over
        }
        (true, Some((from, taken))) => {
            let due = steps_held(now - from);
            ui.data_mut(|data| data.insert_temp(id, (from, due)));
            over && due > taken
        }
        (false, since) => {
            ui.data_mut(|data| data.remove::<(f64, u32)>(id));
            // Handle keyboard/screen-reader clicks or presses released before a
            // deferred step fired.
            let clicked = since.is_none_or(|(_, taken)| taken == 0) && response.clicked();
            if clicked && typed {
                finish_typed_number(ui.ctx());
                ui.data_mut(|data| data.insert_temp(pending, pass));
                ui.ctx().request_repaint();
            }
            clicked && !typed
        }
    };
    let stepped = stepped || put_off;
    if put_off {
        // Record that a deferred user action fired on this pass.
        ui.data_mut(|data| data.insert_temp(egui::Id::new(ACTED), pass));
    }
    if held {
        let from = since.map_or(now, |(from, _)| from);
        let wait = next_step(now - from).max(1e-3);
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    // Suppress the hover tooltip after stepping until the pointer leaves and
    // re-enters: when a step changes the UI scale, egui sees the pointer move
    // and would otherwise show a tooltip that blocks subsequent click detection.
    if stepped {
        ui.data_mut(|data| data.insert_temp(here, true));
    } else if !over && !held {
        ui.data_mut(|data| data.remove::<bool>(here));
    }
    let quiet = ui.data(|data| data.get_temp::<bool>(here)).is_some();
    let response = if quiet { response } else { response.on_hover_text(name) };
    (response, stepped)
}

#[cfg(test)]
mod number_tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn reads_units_and_decimal_commas() {
        assert_eq!(lenient(" 2,5 ", &[]), "2.5");
        assert_eq!(lenient("16 px", &["px"]), "16");
        assert_eq!(lenient("16px", &["px"]), "16");
        assert_eq!(lenient("×1.5", &["×"]), "1.5");
        assert_eq!(lenient("1.5x", &["×"]), "1.5");
        assert_eq!(lenient("600 kbit/s", &["kbit/s"]), "600");
        assert_eq!(lenient("29,97 FPS", &["fps"]), "29.97");
        assert_eq!(lenient("−3", &[]), "-3");
        assert_eq!(lenient("1 000,5", &[]), "1000.5");
        // Only strip configured units.
        assert_eq!(lenient("3 fps", &["px"]), "3fps");
    }

    #[test]
    fn steps_held_down_faster_after_a_while() {
        assert_eq!(steps_held(0.0), 1);
        assert_eq!(steps_held(0.39), 1);
        assert_eq!(steps_held(0.41), 2);
        // 8 steps per second.
        assert_eq!(steps_held(0.41 + 0.125), 3);
        assert_eq!(steps_held(1.45), 10);
        // 20 steps per second after 1.5 s.
        assert_eq!(steps_held(1.51), 11);
        assert_eq!(steps_held(2.01), 21);
        assert!((next_step(0.1) - 0.3).abs() < 1e-9);
        assert!((next_step(0.45) - 0.075).abs() < 1e-9);
        assert!((next_step(1.52) - 0.03).abs() < 1e-9);
    }

    /// Builds a test harness for a [`Number`] field holding `value` and tracks
    /// its bounding rect.
    fn field_of<T: egui::emath::Numeric + Send + Sync>(
        value: T,
        field: impl Fn(Number<'_>) -> Number<'_> + 'static,
    ) -> Harness<'static, (T, Option<egui::Rect>)> {
        field_at(value, field, 1.0)
    }

    /// Like [`field_of`], at `ppp` pixels per point.
    fn field_at<T: egui::emath::Numeric + Send + Sync>(
        value: T,
        field: impl Fn(Number<'_>) -> Number<'_> + 'static,
        ppp: f32,
    ) -> Harness<'static, (T, Option<egui::Rect>)> {
        let mut harness = Harness::builder().with_pixels_per_point(ppp).build_ui_state(
            move |ui, (value, rect): &mut (T, Option<egui::Rect>)| {
                // Size to the widest expected value, matching the GUI.
                let added = steady_numbers(ui, 4, " px", |ui| ui.add(field(Number::new(value))));
                *rect = Some(added.rect);
            },
            (value, None),
        );
        test_fonts(&harness);
        harness.run_steps(2);
        harness
    }

    fn focus<T>(harness: &mut Harness<'_, (T, Option<egui::Rect>)>) {
        harness.get_by_role(egui::accesskit::Role::SpinButton).focus();
        harness.run_steps(2);
    }

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

    /// Verifies that number fields inherit their enclosing row's label for
    /// screen readers unless given an explicit name.
    #[test]
    fn names_a_field_by_its_row_unless_named() {
        use egui_kittest::kittest::NodeT;
        let mut harness = Harness::new_ui_state(
            |ui, values: &mut [u32; 4]| {
                let [a, b, c, d] = values;
                row(ui, "Speed", "", Ok(()), |ui| {
                    ui.add(Number::new(a));
                    ui.add(Number::new(b).name("Columns"));
                });
                ui.add(Number::new(c));
                ui.add(Number::new(d).name("Rows"));
            },
            [1, 2, 3, 4],
        );
        harness.run_steps(2);
        let names: Vec<Option<String>> = harness
            .query_all_by_role(egui::accesskit::Role::SpinButton)
            .map(|node| node.accesskit_node().label())
            .collect();
        let named = |name: &str| Some(name.to_owned());
        assert_eq!(names, [named("Speed"), named("Columns"), None, named("Rows")]);
    }

    #[test]
    fn takes_what_is_typed_once_done_with_it() {
        let mut harness = field_of(8_u32, |field| field.range(1..=100).suffix(" px"));
        let rect = harness.state().1;
        focus(&mut harness);
        harness.event(egui::Event::Text("16".into()));
        harness.run_steps(2);
        // Not committed until editing finishes.
        assert_eq!(harness.state().0, 8);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 16);
        // Tab commits too, stripping units and normalising commas.
        focus(&mut harness);
        harness.event(egui::Event::Text("40 px".into()));
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 40);
        // Out-of-range values are clamped.
        focus(&mut harness);
        harness.event(egui::Event::Text("500".into()));
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 100);
        // Bounding rect stays unchanged throughout.
        assert_eq!(harness.state().1, rect);
        let mut fraction = field_of(1.0_f64, |field| field.range(0.0..=10.0));
        focus(&mut fraction);
        fraction.event(egui::Event::Text("2,5".into()));
        fraction.key_press(egui::Key::Tab);
        fraction.run_steps(2);
        assert_eq!(fraction.state().0, 2.5);
        let mut speed = field_of(1.0_f64, |field| field.range(0.1..=10.0).prefix("×"));
        focus(&mut speed);
        speed.event(egui::Event::Text("×1,5".into()));
        speed.key_press(egui::Key::Enter);
        speed.run_steps(2);
        assert_eq!(speed.state().0, 1.5);
        // Spaces used as thousands separators are accepted, matching egui.
        fn rate(field: Number<'_>) -> Number<'_> {
            field.range(1.0..=50_000.0).suffix(" kbit/s")
        }
        for typed in ["1 200", "1\u{a0}200 kbit/s", "1\u{202f}200"] {
            let mut bitrate = field_of(600.0_f64, rate);
            focus(&mut bitrate);
            bitrate.event(egui::Event::Text(typed.into()));
            bitrate.key_press(egui::Key::Enter);
            bitrate.run_steps(2);
            assert_eq!(bitrate.state().0, 1200.0, "{typed:?}");
        }
    }

    #[test]
    fn puts_back_what_it_held_on_escape() {
        let mut harness = field_of(8_u32, |field| field.range(1..=100));
        let rect = harness.state().1;
        focus(&mut harness);
        harness.event(egui::Event::Text("16".into()));
        harness.run_steps(2);
        // Step with arrow key, type more, then cancel with Escape.
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 17);
        harness.event(egui::Event::Text("3".into()));
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 8);
        assert!(!harness.ctx.text_edit_focused());
        assert_eq!(harness.state().1, rect);
        // Undo inside the text field before committing restores the original
        // text and leaves the underlying value untouched.
        focus(&mut harness);
        harness.event(egui::Event::Text("16".into()));
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 8);
    }

    #[test]
    fn forgets_a_step_put_off_once_greyed_out() {
        // Number field and stepper button that can be disabled between frames.
        let show = |ui: &mut egui::Ui, (value, live, stepped): &mut (i32, bool, u32)| {
            ui.add(Number::new(value));
            ui.add_enabled_ui(*live, |ui| {
                if stepper(ui, "+", true, "Larger").1 {
                    *stepped += 1;
                }
            });
        };
        let mut harness = Harness::new_ui_state(show, (10, true, 0));
        harness.run_steps(2);
        harness.get_by_role(egui::accesskit::Role::SpinButton).focus();
        harness.run_steps(2);
        harness.event(egui::Event::Text("20".into()));
        harness.run_steps(2);
        // Clicking the stepper via AccessKit commits the typed number first and
        // defers the step.
        harness.get_by_label("Larger").click_accesskit();
        harness.step();
        harness.step();
        assert!(harness.state().0 != 10 && harness.state().2 == 0, "{:?}", harness.state());
        harness.state_mut().1 = false;
        harness.run_steps(3);
        harness.state_mut().1 = true;
        harness.run_steps(3);
        assert_eq!(harness.state().2, 0);
    }

    #[test]
    fn keeps_its_value_for_what_is_not_a_number_and_says_so() {
        let mut harness = field_of(8_u32, |field| field.range(1..=100));
        let rect = harness.state().1;
        focus(&mut harness);
        harness.event(egui::Event::Text("lots".into()));
        harness.key_press(egui::Key::Enter);
        harness.run_steps(1);
        assert_eq!(harness.state().0, 8);
        let refused = |harness: &Harness<'_, (u32, Option<egui::Rect>)>| {
            let typed = harness.ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
            let id = typed.unwrap().field.with("refused");
            harness.ctx.data(|data| data.get_temp::<f64>(id)).is_some()
        };
        assert!(refused(&harness));
        assert_eq!(harness.state().1, rect);
        // Flash clears after a few 0.25 s steps.
        harness.run_steps(4);
        assert!(!refused(&harness));
    }

    #[test]
    fn steps_by_its_keys_while_typed_in() {
        let stepped = |field: fn(Number<'_>) -> Number<'_>, from: f64, shift: bool| {
            let mut harness = field_of(from, field);
            focus(&mut harness);
            let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
            harness.key_press_modifiers(modifiers, egui::Key::ArrowUp);
            harness.run_steps(2);
            // Arrow steps apply immediately without waiting for focus loss.
            let up = harness.state().0;
            harness.key_press_modifiers(modifiers, egui::Key::ArrowDown);
            harness.key_press_modifiers(modifiers, egui::Key::ArrowDown);
            harness.run_steps(2);
            (up, harness.state().0)
        };
        fn px(field: Number<'_>) -> Number<'_> {
            field.range(0..=1000).suffix(" px")
        }
        assert_eq!(stepped(px, 50.0, false), (51.0, 49.0));
        assert_eq!(stepped(px, 50.0, true), (60.0, 40.0));
        fn speed(field: Number<'_>) -> Number<'_> {
            field.range(0.1..=10.0).steps(0.05, 0.25)
        }
        assert_eq!(stepped(speed, 1.0, false), (1.05, 0.95));
        assert_eq!(stepped(speed, 1.0, true), (1.25, 0.75));
        // Clamped at range boundaries.
        assert_eq!(stepped(px, 1000.0, true), (1000.0, 980.0));
    }

    #[test]
    fn drags_across_its_range_in_a_few_hundred_points() {
        let mut harness = field_of(0_u32, |field| field.range(0..=1000).speed(0.25));
        let rect = harness.state().1.unwrap();
        let from = rect.center();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(from));
        harness.event(press(from, true));
        harness.run_steps(1);
        // Drag 400 points to the right in small increments.
        for step in 1..=40 {
            harness.event(egui::Event::PointerMoved(from + vec2(10.0 * step as f32, 0.0)));
            harness.run_steps(1);
        }
        let to = from + vec2(400.0, 0.0);
        harness.event(press(to, false));
        harness.run_steps(2);
        let dragged = harness.state().0;
        assert!(dragged >= 950, "{dragged}");
        assert_eq!(harness.state().1, Some(rect));
    }

    /// Returns the displayed or edited text value exposed by the field in
    /// `harness`.
    fn text_of<T>(harness: &Harness<'_, (T, Option<egui::Rect>)>) -> String {
        use egui_kittest::kittest::NodeT;
        let field = harness.get_by_role(egui::accesskit::Role::SpinButton);
        field.accesskit_node().value().unwrap_or_default().to_owned()
    }

    #[test]
    fn shows_whole_numbers_whole_at_any_scale() {
        for ppp in [0.5, 0.8, 0.9, 0.99, 1.0, 1.25, 2.0] {
            let mut harness = field_at(235_i64, |field| field.range(0..=500).suffix(" px"), ppp);
            assert_eq!(text_of(&harness), "235 px", "at {ppp}");
            let rect = harness.state().1.unwrap();
            let from = rect.center();
            // Hovered.
            harness.event(egui::Event::PointerMoved(from));
            harness.run_steps(2);
            assert_eq!(text_of(&harness), "235 px", "hovered at {ppp}");
            // Mid-drag and after release.
            let press = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            harness.event(press(from, true));
            harness.run_steps(1);
            for step in 1..=7 {
                harness.event(egui::Event::PointerMoved(from + vec2(3.7 * step as f32, 0.0)));
                harness.run_steps(1);
                let value = harness.state().0;
                assert_eq!(text_of(&harness), format!("{value} px"), "dragged at {ppp}");
            }
            harness.event(press(from + vec2(25.9, 0.0), false));
            harness.run_steps(2);
            let value = harness.state().0;
            assert!(value > 235, "{value} at {ppp}");
            assert_eq!(text_of(&harness), format!("{value} px"), "after a drag at {ppp}");
            // Focused for typing and stepped with arrow keys.
            focus(&mut harness);
            assert_eq!(text_of(&harness), value.to_string(), "typed in at {ppp}");
            harness.key_press(egui::Key::ArrowUp);
            harness.run_steps(2);
            assert_eq!(text_of(&harness), (value + 1).to_string(), "stepped at {ppp}");
            harness.key_press(egui::Key::Enter);
            harness.run_steps(2);
            assert_eq!(text_of(&harness), format!("{} px", value + 1), "done with at {ppp}");
        }
        // Floating-point fields display the decimals implied by their speed.
        for ppp in [1.0, 2.0] {
            let harness = field_at(1.5_f64, |field| field.range(0.0..=10.0).speed(0.01), ppp);
            assert_eq!(text_of(&harness), "1.50", "at {ppp}");
        }
    }

    #[test]
    fn steps_by_the_wheel_only_while_typed_in() {
        type State = (u32, f32);
        let show = |ui: &mut egui::Ui, (value, scrolled): &mut State| {
            let area = egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                ui.add(Number::new(value).range(0..=100));
                ui.allocate_space(vec2(100.0, 400.0));
            });
            *scrolled = area.state.offset.y;
        };
        let mut harness = Harness::new_ui_state(show, (50, 0.0));
        harness.run_steps(2);
        let field = harness.get_by_role(egui::accesskit::Role::SpinButton).rect();
        let wheel = |harness: &mut Harness<'_, State>, notches: f32| {
            harness.event(egui::Event::PointerMoved(field.center()));
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, notches),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run_steps(6);
        };
        // Hovering without focus scrolls the area and leaves the number unchanged.
        wheel(&mut harness, -1.0);
        assert_eq!(harness.state().0, 50);
        let scrolled = harness.state().1;
        assert!(scrolled > 0.0);
        // Scroll back up, focus for typing, and verify the wheel steps the
        // number without scrolling the area.
        wheel(&mut harness, 3.0);
        assert_eq!(harness.state().1, 0.0);
        harness.get_by_role(egui::accesskit::Role::SpinButton).focus();
        harness.run_steps(2);
        wheel(&mut harness, 2.0);
        assert_eq!(*harness.state(), (52, 0.0));
        wheel(&mut harness, -1.0);
        assert_eq!(*harness.state(), (51, 0.0));
    }

    #[test]
    fn repeats_a_step_held_down() {
        type State = (u32, bool);
        let show = |ui: &mut egui::Ui, (steps, enabled): &mut State| {
            if stepper(ui, "+", *enabled, "Larger").1 {
                *steps += 1;
            }
        };
        let mut harness = Harness::builder().with_step_dt(0.01).build_ui_state(show, (0, true));
        theme::apply(&harness.ctx, None);
        harness.run_steps(2);
        let at = harness.get_by_label("Larger").rect();
        assert!(at.height() >= 28.0 && at.width() >= 28.0, "{at:?}");
        let button = |pressed| egui::Event::PointerButton {
            pos: at.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(at.center()));
        harness.run_steps(1);
        // Single click: one step.
        harness.event(button(true));
        harness.event(button(false));
        harness.run_steps(2);
        assert_eq!(harness.state().0, 1);
        // Held down: 1 on press, 8/s after 0.4 s, and 20/s after 1.5 s.
        harness.state_mut().0 = 0;
        harness.event(button(true));
        harness.run_steps(1);
        assert_eq!(harness.state().0, 1);
        harness.run_steps(35);
        assert_eq!(harness.state().0, 1);
        harness.run_steps(10);
        assert_eq!(harness.state().0, 2);
        harness.run_steps(55);
        // At 1.0 s: 0.4, 0.525 .. 0.9.
        assert_eq!(harness.state().0, 6);
        harness.run_steps(101);
        // At 2.01 s: 9 slow steps up to 1.4 s, plus 11 fast steps from 1.5 to 2.0 s.
        assert_eq!(harness.state().0, 1 + 9 + 11);
        // Released: stops repeating.
        harness.event(button(false));
        harness.run_steps(100);
        assert_eq!(harness.state().0, 21);
        // Pointer moved off the button while held: pauses repeating.
        harness.state_mut().0 = 0;
        harness.event(button(true));
        harness.run_steps(50);
        harness.event(egui::Event::PointerMoved(at.center() + vec2(0.0, 100.0)));
        harness.run_steps(1);
        let left = harness.state().0;
        harness.run_steps(100);
        assert_eq!(harness.state().0, left);
        harness.event(button(false));
        harness.run_steps(2);
        // Disabled while held (e.g. reaching the range limit): stops repeating.
        harness.event(egui::Event::PointerMoved(at.center()));
        harness.state_mut().0 = 0;
        harness.event(button(true));
        harness.run_steps(50);
        harness.state_mut().1 = false;
        harness.run_steps(1);
        let ended = harness.state().0;
        harness.run_steps(100);
        assert_eq!(harness.state().0, ended);
    }
}

/// Draws an [`auto_number`] for a setting that may also be defined by the
/// active preset. When `preset` is `Some(p)`, the automatic segment is labelled
/// as [`from_preset`] says ("As preset") and shows `p` (which also seeds custom
/// edits); when `preset` is `None`, it uses `automatic` and `shown`.
pub(crate) fn preset_number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    preset: Option<T>,
    automatic: (&str, &str),
    shown: Option<T>,
    start: T,
    edit: impl FnOnce(Number<'_>) -> Number<'_>,
) -> bool {
    match preset {
        Some(preset) => {
            let presets = from_preset();
            auto_number(ui, value, (&presets.0, &presets.1), Some(preset), preset, edit)
        }
        None => auto_number(ui, value, automatic, shown, start, edit),
    }
}

/// Returns the `(label, hover_hint)` pair for an option that inherits its value
/// from the active preset.
pub(crate) fn from_preset() -> (String, String) {
    (fl!("from-preset"), fl!("from-preset", "hint"))
}

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

/// Right-hand gutter width reserved for the scroll bar whether visible or not
/// so content width stays constant.
pub(crate) const GUTTER: f32 = 12.0;
/// Scroll bar width at the outer edge of [`GUTTER`], flush with adjacent
/// buttons while leaving the inner half of the gutter as a gap.
pub(crate) const SCROLL_BAR: f32 = 6.0;
/// Minimum scroll-bar handle length so long lists remain easy to grab.
const LEAST_HANDLE: f32 = 24.0;
/// Height of the top and bottom scroll-overflow fade gradients.
pub(crate) const SCROLL_FADE: f32 = 24.0;

/// Shows `area` with the standard scroll style: always-visible solid scroll bar
/// when content overflows, a permanently reserved [`GUTTER`], and top/bottom
/// overflow fade gradients.
pub(crate) fn scrolled<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let width = ui.available_width();
    let outer = ui.style().clone();
    ui.scope(|ui| {
        let mut style = egui::style::ScrollStyle::solid();
        style.bar_width = SCROLL_BAR;
        style.bar_inner_margin = GUTTER - SCROLL_BAR;
        style.bar_outer_margin = 0.0;
        style.handle_min_length = LEAST_HANDLE;
        style.fade = egui::style::ScrollFadeStyle { strength: 1.0, size: SCROLL_FADE };
        ui.spacing_mut().scroll = style;
        // egui paints the track in `extreme_bg_color` and the handle in a
        // widget's fill (here a checkbox's, the view colour, the same as the
        // track), so the handle would not show; override both.
        let colours = theme::scroll_bar(ui.visuals());
        let visuals = ui.visuals_mut();
        visuals.extreme_bg_color = colours.track;
        let round = egui::CornerRadius::same((SCROLL_BAR / 2.0) as u8);
        let widgets = &mut visuals.widgets;
        for (state, handle) in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active]
            .into_iter()
            .zip(colours.handle)
        {
            state.bg_fill = handle;
            state.corner_radius = round;
        }
        // Scope the temp key to this specific `Ui` because egui assigns the
        // same default ID to side-by-side scroll areas.
        let tallest = ui.unique_id().with("tallest while held");
        let shown = area.show(ui, |ui| {
            // Restore the outer widget style for content inside the scroll area.
            ui.set_style(outer);
            ui.set_max_width(width - GUTTER);
            let added = add(ui);
            // Prevent content height from shrinking relative to the immediately
            // preceding pass while a pointer press that started inside this
            // scroll area is held down: otherwise, when scrolled to the bottom,
            // holding a repeat button that shortens content below it would shift
            // the button out from under the pointer. Presses that started
            // elsewhere (such as dragging a panel divider) allow normal reflow.
            let pass = ui.ctx().cumulative_pass_nr();
            let before = ui.data(|data| data.get_temp::<(u64, f32)>(tallest));
            let here = tallest.with("pressed here");
            let (pressed, down, origin) = ui.input(|input| {
                let pointer = &input.pointer;
                (pointer.any_pressed(), pointer.any_down(), pointer.press_origin())
            });
            let held_here = if pressed {
                // Check against the viewport rect recorded on the previous frame.
                let seen = ui.data(|data| data.get_temp::<egui::Rect>(tallest.with("seen")));
                let layer = ui.layer_id();
                let on = origin.zip(seen).is_some_and(|(at, seen)| {
                    seen.contains(at) && ui.ctx().layer_id_at(at) == Some(layer)
                });
                ui.data_mut(|data| data.insert_temp(here, on));
                on
            } else {
                ui.data(|data| data.get_temp::<bool>(here)).unwrap_or(false)
            };
            if !down {
                ui.data_mut(|data| data.remove::<bool>(here));
            }
            if let Some((_, before)) = before.filter(|&(at, _)| at + 1 == pass)
                && down
                && held_here
            {
                // Expand from `min_rect().top()` rather than calling
                // `set_min_height`, which would add height after the cursor and
                // grow every frame.
                ui.expand_to_include_y(ui.min_rect().top() + before);
            }
            let height = ui.min_rect().height();
            ui.data_mut(|data| data.insert_temp(tallest, (pass, height)));
            added
        });
        // Record the visible viewport rect including the scroll-bar gutter.
        let seen = egui::Rect::from_min_size(
            shown.inner_rect.min,
            vec2(width.max(shown.inner_rect.width()), shown.inner_rect.height()),
        );
        ui.data_mut(|data| data.insert_temp(tallest.with("seen"), seen));
        shown
    })
    .inner
}

/// Draws a transparency checkerboard over `rect` anchored at `rect.min`.
pub(crate) fn checkerboard(painter: &egui::Painter, rect: egui::Rect, square: f32) {
    checkerboard_from(painter, rect, square, rect.min);
}

/// Draws a transparency checkerboard over `rect` with squares aligned to
/// `origin` so panning or zooming an image moves over a stationary stage grid.
pub(crate) fn checkerboard_from(
    painter: &egui::Painter,
    rect: egui::Rect,
    square: f32,
    origin: egui::Pos2,
) {
    let (light, dark) = if painter.ctx().global_style().visuals.dark_mode {
        (Color32::from_gray(92), Color32::from_gray(72))
    } else {
        (Color32::from_gray(240), Color32::from_gray(212))
    };
    painter.rect_filled(rect, 0.0, light);
    let clip = painter.clip_rect().intersect(rect);
    if clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    // Snap the origin to whole physical pixels so square edges stay crisp.
    let origin = egui::emath::GuiRounding::round_to_pixels(origin, painter.pixels_per_point());
    // Iterate only over the grid cells intersecting the clip rect.
    let first_column = ((clip.left() - origin.x) / square).floor() as i64;
    let first_row = ((clip.top() - origin.y) / square).floor() as i64;
    let columns = ((clip.right() - origin.x) / square).ceil() as i64;
    let rows = ((clip.bottom() - origin.y) / square).ceil() as i64;
    for row in first_row..rows {
        let start = first_column + (row + first_column).rem_euclid(2);
        for column in (start..columns).step_by(2) {
            let min = origin + egui::vec2(column as f32 * square, row as f32 * square);
            let cell = egui::Rect::from_min_size(min, egui::vec2(square, square)).intersect(rect);
            if cell.is_positive() {
                painter.rect_filled(cell, 0.0, dark);
            }
        }
    }
}

/// Draws a vertical marker tick at `x` spanning `top..=bottom` with a 2 pt core
/// and 1 pt contrasting casing (via [`theme::over_fill`]) snapped to whole
/// screen pixels so it remains sharp over any background or accent colour.
/// Returns the `(left, right)` outer X coordinates.
pub(crate) fn tick(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    x: f32,
    (top, bottom): (f32, f32),
) -> (f32, f32) {
    let cased = theme::over_fill(visuals);
    let ppp = painter.pixels_per_point();
    let pixels = |points: f32| (points * ppp).round().max(1.0);
    let (core, casing) = (pixels(2.0), pixels(1.0));
    // Snap the core's left edge to a physical pixel boundary centred on `x`.
    let left = (x * ppp - core / 2.0).round();
    let rect = |left: f32, right: f32, grow: f32| {
        egui::Rect::from_min_max(
            egui::pos2(left / ppp, ((top * ppp).round() - grow) / ppp),
            egui::pos2(right / ppp, ((bottom * ppp).round() + grow) / ppp),
        )
    };
    let outer = rect(left - casing, left + core + casing, casing);
    painter.rect_filled(outer, 0.0, cased.casing);
    painter.rect_filled(rect(left, left + core, 0.0), 0.0, cased.core);
    (outer.left(), outer.right())
}

/// Draws a double-stroked outline inside `rect` using [`theme::over_fill`]
/// (outer core, inner casing) so it stays visible over any fill or accent hue.
pub(crate) fn cased_outline(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: egui::Rect,
    radius: f32,
) {
    let cased = theme::over_fill(visuals);
    let line = |colour| egui::Stroke::new(1.0, colour);
    painter.rect_stroke(rect, radius, line(cased.core), egui::StrokeKind::Inside);
    let inner = rect.shrink(1.0);
    if inner.is_positive() {
        let radius = (radius - 1.0).max(0.0);
        painter.rect_stroke(inner, radius, line(cased.casing), egui::StrokeKind::Inside);
    }
}

/// Fills `rect` with 45° right-leaning diagonal stripes of `colour` (equal
/// stripe and gap widths) anchored at X coordinate `origin` so the pattern
/// stays stationary as `rect`'s edges move.
pub(crate) fn hatch(painter: &egui::Painter, rect: egui::Rect, origin: f32, colour: Color32) {
    /// Horizontal period between stripe centres along the bar.
    const PERIOD: f32 = 6.0;
    let clip = painter.clip_rect().intersect(rect);
    if !clip.is_positive() {
        return;
    }
    let painter = painter.with_clip_rect(clip);
    // At 45°, perpendicular stroke width is half the horizontal period divided by √2.
    let stroke = egui::Stroke::new(PERIOD / 2.0 / std::f32::consts::SQRT_2, colour);
    let rise = rect.height();
    let first = ((clip.left() - rise - origin) / PERIOD).floor() as i64;
    let last = ((clip.right() - origin) / PERIOD).ceil() as i64;
    for stripe in first..=last {
        let x = origin + stripe as f32 * PERIOD;
        let from = egui::pos2(x, rect.bottom() + 1.0);
        let to = egui::pos2(x + rise + 2.0, rect.top() - 1.0);
        painter.line_segment([from, to], stroke);
    }
}

/// Installs the GUI's fonts into a bare test `harness`. egui's default fonts
/// are disabled in Cargo features, and without glyphs a text cursor always
/// stays at index 0 so typed characters would be inserted backwards.
#[cfg(test)]
pub(crate) fn test_fonts<State>(harness: &egui_kittest::Harness<'_, State>) {
    harness.ctx.set_fonts(crate::fonts::fonts());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_scrolls_as_long_only_as_it_was_the_frame_before() {
        // Line count, visibility flag, and resulting scroll content height.
        type State = (usize, bool, f32);
        let show = |ui: &mut egui::Ui, (lines, shown, tall): &mut State| {
            if !*shown {
                return;
            }
            let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            *tall = scrolled(ui, area, |ui| {
                for line in 0..*lines {
                    ui.label(format!("line {line}"));
                }
            })
            .content_size
            .y;
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([300.0, 400.0])
            .build_ui_state(show, (10, true, 0.0));
        test_fonts(&harness);
        harness.run_steps(2);
        let tall = harness.state().2;
        // Hide for several frames, hold the pointer down, then re-show with fewer lines.
        harness.state_mut().1 = false;
        let at = egui::pos2(150.0, 350.0);
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(3);
        *harness.state_mut() = (2, true, 0.0);
        harness.run_steps(3);
        assert!(harness.state().2 < tall / 2.0, "{} of {tall}", harness.state().2);
    }

    #[test]
    fn keeps_what_scrolls_as_long_while_the_pointer_is_held() {
        // A button followed by trailing lines, scrolled to the bottom.
        type State = (usize, egui::Rect);
        let show = |ui: &mut egui::Ui, (lines, at): &mut State| {
            ui.set_height(200.0);
            let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            scrolled(ui, area, |ui| {
                for line in 0..20 {
                    ui.label(format!("line {line}"));
                }
                *at = ui.button("held").rect;
                for line in 0..*lines {
                    ui.label(format!("after {line}"));
                }
            });
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([300.0, 220.0])
            .build_ui_state(show, (3, egui::Rect::NOTHING));
        test_fonts(&harness);
        harness.run();
        harness.event(egui::Event::PointerMoved(egui::pos2(150.0, 100.0)));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -2000.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.run_steps(10);
        let at = harness.state().1.center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(at));
        harness.event(press(true));
        harness.step();
        // Removing trailing lines while the pointer is held keeps the button under the pointer.
        harness.state_mut().0 = 1;
        harness.run_steps(3);
        assert!(harness.state().1.contains(at), "{:?} from {at:?}", harness.state().1);
        // Releasing the pointer lets the scroll area shrink to its new end.
        harness.event(press(false));
        harness.run_steps(3);
        assert!(!harness.state().1.contains(at));
    }

    #[test]
    fn keeps_what_scrolls_as_long_only_for_a_press_that_began_on_it() {
        // Two side-by-side scroll areas with a button in each and trailing
        // lines in the second.
        type State = (usize, [egui::Rect; 2], f32);
        let show = |ui: &mut egui::Ui, (lines, at, tall): &mut State| {
            ui.set_height(200.0);
            ui.columns(2, |columns| {
                let area = egui::ScrollArea::vertical().id_salt("one").auto_shrink([false; 2]);
                scrolled(&mut columns[0], area, |ui| {
                    at[0] = ui.button("elsewhere").rect;
                });
                let area = egui::ScrollArea::vertical().id_salt("two").auto_shrink([false; 2]);
                *tall = scrolled(&mut columns[1], area, |ui| {
                    at[1] = ui.button("here").rect;
                    for line in 0..*lines {
                        ui.label(format!("line {line}"));
                    }
                })
                .content_size
                .y;
            });
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([400.0, 220.0])
            .build_ui_state(show, (30, [egui::Rect::NOTHING; 2], 0.0));
        test_fonts(&harness);
        harness.run();
        let tall = harness.state().2;
        // Measure the second area's content height when shortened while holding
        // button `on`.
        let shortened = |harness: &mut egui_kittest::Harness<'_, State>, on: usize| {
            let at = harness.state().1[on].center();
            let press = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            harness.event(egui::Event::PointerMoved(at));
            harness.event(press(true));
            harness.step();
            harness.state_mut().0 = 5;
            harness.run_steps(3);
            let held = harness.state().2;
            harness.event(press(false));
            harness.state_mut().0 = 30;
            harness.run_steps(3);
            held
        };
        // Holding inside the second area preserves its height; holding in the
        // first area lets the second shrink immediately.
        assert_eq!(shortened(&mut harness, 1), tall);
        let held = shortened(&mut harness, 0);
        assert!(held < tall - 50.0, "{held} of {tall}");
    }

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

    #[test]
    fn keeps_automatic_numbers_automatic() {
        // Automatic value outside the custom range (e.g. a 20× speed to fit a
        // 1-minute clip into 3 seconds).
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, value: &mut Option<f64>| {
                auto_number(ui, value, ("Automatic", ""), Some(20.0), 1.0, |field| {
                    field.range(0.1..=10.0)
                });
            },
            None,
        );
        harness.run();
        assert_eq!(*harness.state(), None);
        // Custom value inside a disabled scope whose current range no longer
        // includes it.
        let mut greyed = egui_kittest::Harness::new_ui_state(
            |ui, value: &mut Option<u32>| {
                ui.add_enabled_ui(false, |ui| {
                    auto_number(ui, value, ("All", ""), Some(4), 4, |field| field.range(1..=4));
                });
            },
            Some(20),
        );
        greyed.run();
        assert_eq!(*greyed.state(), Some(20));
        // Displayed as-is without clamping.
        use egui_kittest::kittest::Queryable;
        harness.get(egui_kittest::kittest::by().value("20"));
    }

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

    #[test]
    fn writes_times() {
        assert_eq!(seconds(2.5), "2.50 s");
        assert_eq!(seconds(62.5), "1:02.50");
        assert_eq!(seconds(119.996), "2:00.00");
        // Formatted times round-trip through `parse_seconds` to the hundredth.
        for (time, written) in [(59.996, "1:00.00"), (60.0, "1:00.00"), (119.996, "2:00.00")] {
            assert_eq!(seconds(time), written);
            let read = parse_seconds(written).unwrap();
            assert!((read - (time * 100.0_f64).round() / 100.0).abs() < 1e-9, "{written}");
        }
        assert_eq!(parse_seconds("2.50 s"), Some(2.5));
        assert_eq!(parse_seconds(" 4 "), Some(4.0));
        assert_eq!(parse_seconds("1:75"), None);
        assert_eq!(parse_seconds("x"), None);
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

    #[test]
    fn keeps_room_for_the_scroll_bar_whether_it_shows_or_not() {
        // Track available content width and inner rect width as the scroll bar
        // appears and disappears.
        type State = (usize, Option<(f32, f32)>);
        let show = |ui: &mut egui::Ui, (lines, seen): &mut State| {
            let area = egui::ScrollArea::vertical().max_height(200.0).auto_shrink([false, true]);
            let shown = scrolled(ui, area, |ui| {
                for _ in 0..*lines {
                    ui.allocate_exact_size(vec2(40.0, 20.0), egui::Sense::hover());
                }
                ui.available_width()
            });
            *seen = Some((shown.inner, shown.inner_rect.width()));
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([400.0, 300.0])
            .build_ui_state(show, (2, None));
        harness.run_steps(3);
        let (few, room) = harness.state().1.unwrap();
        harness.state_mut().0 = 40;
        harness.run_steps(30);
        let (many, narrower) = harness.state().1.unwrap();
        // Content width stays identical whether the scroll bar is drawn in the
        // reserved gutter or not.
        assert_eq!(few, many);
        assert!(narrower < room && few <= narrower, "{few} {narrower} {room}");
        assert_eq!(few, harness.ctx.content_rect().width() - 16.0 - GUTTER);
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

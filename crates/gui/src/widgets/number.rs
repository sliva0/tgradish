//! The numeric editor [`Number`] and its steppers, the typed-number state
//! other parts of the GUI commit or forget, and automatic or preset numbers.

use eframe::egui::{self, Color32, RichText, vec2};

use super::choice::segments;
use super::layout::{named, row_label};
use crate::l10n::fl;
use crate::theme;

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
    ///
    /// [`row`]: super::row
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

/// Returns `true` if a number field committed or rejected typed input during
/// the current pass.
fn number_taken_now(ctx: &egui::Context) -> bool {
    let taken = ctx.data(|data| data.get_temp::<u64>(egui::Id::new(TAKEN)));
    taken == Some(ctx.cumulative_pass_nr())
}

/// Returns `true` if a number field has uncommitted text or committed it on
/// this pass, allowing another control clicked at the same time to let the
/// typed edit commit as its own undo step first.
fn number_typed_now(ctx: &egui::Context) -> bool {
    draft(ctx).is_some() || number_taken_now(ctx)
}

/// Tells history `said` of the edit of the file being drawn (set by
/// [`of_file`]), if one is: controls elsewhere, as in dialogs, edit no
/// file's settings.
pub(crate) fn tell_history(ctx: &egui::Context, said: fn(&egui::Context, u64)) {
    if let Some(file) = ctx.data(|data| data.get_temp::<u64>(egui::Id::new(DRAWING))) {
        said(ctx, file);
    }
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
    ///
    /// [`row`]: super::row
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
        ui.data_mut(|data| data.insert_temp(egui::Id::new(TAKEN), pass));
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
        // a screen reader's change is done with as it is made
        if (asked.is_some() || steps_asked != 0.0) && value != old {
            tell_history(ui.ctx(), crate::history::commit);
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
                    tell_history(ui.ctx(), crate::history::cancel);
                } else {
                    if let Some(taken) = self.take(ui, id, typing) {
                        value = taken;
                    }
                    tell_history(ui.ctx(), crate::history::commit);
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
                // a step of its own, as the field goes on being typed in
                tell_history(ui.ctx(), crate::history::commit);
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
                // what its keys and the wheel step, and what is typed, is
                // one step once it is done with
                tell_history(ui.ctx(), crate::history::underway);
            } else {
                // Editing finished via Enter or a pointer press outside.
                ui.data_mut(|data| data.remove::<Typing>(id));
                if escaped {
                    value = self.put_back(ui, &typing, value);
                    tell_history(ui.ctx(), crate::history::cancel);
                } else {
                    if let Some(taken) = self.take(ui, id, &typing) {
                        value = taken;
                    }
                    tell_history(ui.ctx(), crate::history::commit);
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
                // a step once it is let go of
                tell_history(ui.ctx(), crate::history::underway);
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
            } else if response.drag_stopped() {
                tell_history(ui.ctx(), crate::history::commit);
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
    // Held, its steps are one edit, done with as it is let go of; a click
    // by the keyboard or a screen reader, or one put off, is a step alone.
    if held {
        tell_history(ui.ctx(), crate::history::underway);
    } else if since.is_some() || stepped {
        tell_history(ui.ctx(), crate::history::commit);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{row, steady_numbers, test_fonts};
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
}

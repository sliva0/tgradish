//! Small pieces the GUI is built from.

use eframe::egui::{self, Color32, RichText, vec2};

use crate::jobs::Problem;
use crate::l10n::fl;
use crate::theme;

pub(crate) fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

/// Seconds as `1:02.50` or `2.50 s`, to the hundredth: rounded first, so
/// 119.996 is `2:00.00`, not `1:60.00`.
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

/// Seconds typed as [`seconds`] writes them, `1:02.50` or `2.50 s`, or as
/// a number alone.
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

/// Size of a section's heading, in the headings' weight: a step larger than
/// the text under it, so that a section stands out of a column of settings
/// and stands over the headings within it, as Preset's, which are only in
/// that weight.
pub(crate) const HEADING: f32 = 16.0;
/// Size of a dialog's title, and of the other headings.
pub(crate) const TITLE: f32 = 17.0;

/// `text` in the headings' weight, Ubuntu Medium, at `size`.
pub(crate) fn medium(ui: &egui::Ui, text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(crate::fonts::medium_in(ui.ctx())).size(size).strong()
}

// Settings are spaced on one scale, of what shows between things: a
// caption 4 points under its control, 8 between the controls of a setting,
// 12 between settings, and a section's line 16 under what is above it and
// 12 over its heading, which is 8 over what it heads. The spaces below
// differ from those, as text has room of its own over and under it.

/// Space between the things of a setting.
const WITHIN: f32 = 8.0;
/// Space added after a setting, to the next.
const AFTER_ROW: f32 = 4.0;
/// Space before a caption, and how much less room it takes under it than
/// its lines leave, which is room under the text for nothing.
const BEFORE_HINT: f32 = -4.0;
const HINT_TRIM: f32 = 1.0;
/// Space before a section's line, after it, and after its heading.
const BEFORE_RULE: f32 = 4.0;
const AFTER_RULE: f32 = -5.0;
const AFTER_HEADING: f32 = -8.0;
/// How wide a caption gets, so a long one wraps into a block rather than
/// a long line.
const HINT_WIDTH: f32 = 460.0;

/// Spaces what `ui` holds as settings are spaced.
pub(crate) fn settings_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.y = WITHIN;
}

/// A heading over a group of settings, apart from the group before by a
/// line, unless it is the first of its column.
pub(crate) fn section(ui: &mut egui::Ui, title: &str) {
    section_with(ui, title, |_| {});
}

/// A [`section`] with more beside its title, as `beside` adds.
pub(crate) fn section_with(ui: &mut egui::Ui, title: &str, beside: impl FnOnce(&mut egui::Ui)) {
    section_rule(ui);
    ui.horizontal(|ui| {
        // onto more lines where a narrow column leaves it no room
        ui.add(egui::Label::new(medium(ui, title, HEADING)).wrap());
        beside(ui);
    });
    ui.add_space(AFTER_HEADING);
}

/// The line over a section, apart from the group before, unless it is the
/// first of its column; then nothing.
pub(crate) fn section_rule(ui: &mut egui::Ui) {
    // nothing above it yet
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

/// A section that folds away, headed as the others are, its triangle
/// before its heading.
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

// Numbers that change as something is dragged, as what follows from it
// does, or as something plays or works, have a fixed number of decimals
// for their kind, so that they keep their width: speeds ×1.50, seconds
// 2.50 s, frame rates 29.97 fps. Ubuntu's digits are all as wide as each
// other, and it has no kerning between them, so a number keeps its width
// while its digits change; only a number of other digits, past 9.99 or
// 99.99, is wider, which fields and lines of text that hold one leave room
// for, as [`steady_width`] and [`steady_hint`] do. Typed and finished
// values, as the result's, are [`trimmed`], which reads better.

/// The decimals of a speed, of a frame rate, and of seconds.
pub(crate) const SPEED_DECIMALS: usize = 2;
pub(crate) const FPS_DECIMALS: usize = 2;

/// `value` with `decimals` decimals, however many it needs.
pub(crate) fn fixed(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

/// A speed, as a factor, without its ×: 1.50.
pub(crate) fn speed(value: f64) -> String {
    fixed(value, SPEED_DECIMALS)
}

/// A frame rate, without its unit: 29.97, 30.00.
pub(crate) fn fps(value: f64) -> String {
    fixed(value, FPS_DECIMALS)
}

/// `text` with each digit a 0: as wide as any number of as many digits.
pub(crate) fn zeros(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { '0' } else { c }).collect()
}

/// The widest [`seconds`] writes a time from 0 to `most` seconds, as
/// either form it writes them in: `00.00 s` and `0:00.00`.
pub(crate) fn widest_seconds(most: f64) -> Vec<String> {
    let mut widest = vec![zeros(&seconds(most.clamp(0.0, 59.99)))];
    if most >= 59.995 {
        widest.push(zeros(&seconds(most)));
    }
    widest
}

/// A number with as few decimals as it needs, at most two: 60, 29.97.
pub(crate) fn trimmed(value: f64) -> String {
    trimmed_to(value, 2)
}

/// A number with as few decimals as it needs, at most `decimals`.
pub(crate) fn trimmed_to(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// How wide `text` is in buttons' font, plus `padding`.
pub(crate) fn text_width(ui: &egui::Ui, text: &str, padding: f32) -> f32 {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    galley.size().x + padding
}

/// Which of `texts`, its forms from the longest to the shortest, fits in
/// `room` with `padding` beside it, in buttons' font: the first that does,
/// if any. Words that have a shorter form where room is short take it
/// rather than being cut short.
pub(crate) fn fitting(ui: &egui::Ui, texts: &[&str], room: f32, padding: f32) -> Option<usize> {
    texts.iter().position(|text| text_width(ui, text, padding) <= room)
}

/// Number fields added by `add` as wide as `digits` digits and `suffix`
/// take, so that they keep their width as the number changes, typed or
/// dragged: Ubuntu's digits are all as wide as each other.
pub(crate) fn steady_numbers<R>(
    ui: &mut egui::Ui,
    digits: usize,
    suffix: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    steady_width(ui, &("0".repeat(digits) + suffix), add)
}

/// Number fields added by `add` as wide as `widest`, the widest text they
/// may show, at the least, so that they keep their width, and what comes
/// after them its place, as the number changes, typed or dragged.
pub(crate) fn steady_width<R>(
    ui: &mut egui::Ui,
    widest: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        // a point more, as glyphs placed on whole pixels may come out a
        // little wider than measured
        ui.spacing_mut().interact_size.x =
            text_width(ui, widest, 2.0 * ui.spacing().button_padding.x).ceil() + 1.0;
        add(ui)
    })
    .inner
}

/// Shows what `add` adds as `part` of the file `file`'s, with ids of its
/// own: what a widget keeps by its id, as the text being typed in a number
/// field, stays the file's, rather than going to the widget in its place
/// in another file's settings once that file is picked; and a number
/// typed in it is known to be the file's, as [`typed_number`] has it.
pub(crate) fn of_file<R>(
    ui: &mut egui::Ui,
    part: &str,
    file: u64,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    of_file_in(ui, egui::UiBuilder::new(), part, file, add)
}

/// An [`of_file`] placed as `place` says.
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

/// A link to `url`, which takes a click as a link, rather than starting a
/// selection as text does.
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

/// Weaker text explaining something. It stays as readable where what it
/// explains is greyed out, as it often says why.
pub(crate) fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    let text = RichText::new(text.into()).weak().size(12.5);
    if ui.is_enabled() {
        ui.label(text);
        return;
    }
    weak_text(ui, text, ui.available_width(), 0.0);
}

/// `text` laid out within `width` and drawn as weak text, without the
/// greying of what is around it, taking `trim` less room under it than
/// its lines leave.
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
    // read out as a label is
    let enabled = ui.is_enabled();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, galley.text()));
    let mut painter = ui.painter().clone();
    painter.set_opacity(1.0);
    painter.galley(rect.min, galley, weak);
    response
}

/// A [`note`] that is a control's caption: close under it, and no wider
/// than reads well.
pub(crate) fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add_space(BEFORE_HINT);
    let text = RichText::new(text.into()).weak().size(12.5);
    let width = ui.available_width().min(HINT_WIDTH);
    weak_text(ui, text, width, HINT_TRIM);
}

/// The room a [`hint`] of `text` takes, left empty: kept where the hint
/// shows only at times, so that nothing under it moves as it comes and
/// goes.
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

/// A [`hint`] in room for the tallest of `forms`, what it may say in turn,
/// laid out as wide: so that what is under it stays where it is as it
/// says another, as a control over it is stepped.
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

/// A [`hint`] holding a number that changes, as something is dragged or
/// plays: its lines break where they would with the widest number it may
/// hold, which `widest` says, the hint written with each of the widest
/// numbers of the forms the number may take. So the lines stay as they are
/// as the number changes, rather than a word going to the next line and
/// back as its digits come and go.
pub(crate) fn steady_hint(ui: &mut egui::Ui, text: impl Into<String>, widest: &[String]) {
    ui.add_space(BEFORE_HINT);
    let width = ui.available_width().min(HINT_WIDTH);
    let text = steady_text(ui, text.into(), widest, width);
    weak_text(ui, RichText::new(text).weak().size(12.5), width, HINT_TRIM);
}

/// `text` as [`steady_hint`] lays it out `width` wide: with its lines
/// broken where the widest of `widest` breaks, laid out a little narrower,
/// as kerning by the digits' neighbours may make another number a little
/// wider.
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

/// Where `galley`'s lines that wrap end, in characters from its start.
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

/// `text`, which is `like` with other digits in a part of it, broken onto
/// lines where `like` breaks at `breaks`: the same place in the part they
/// share before and after where they differ, and after that part where a
/// break falls within it.
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
            // the space the line ended with goes, as wrapping drops it
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

/// What keeps a file from being converted, said under the setting it is
/// of, in the warning colour, with a button that puts it right, named
/// `fix` and explained by `hint`: true once clicked.
pub(crate) fn attention(ui: &mut egui::Ui, text: &str, fix: &str, hint: &str) -> bool {
    attention_explained(ui, text, "", fix, hint)
}

/// [`attention`], with more of what to do about it said on hover over
/// `text`, as `why` says it, where that isn't empty.
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

/// `response`, a button that shows a symbol rather than words, named
/// `name` for screen readers, which would otherwise say the symbol.
pub(crate) fn named(response: egui::Response, name: &str) -> egui::Response {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    response
}

/// The label column of a grid, explained on hover.
pub(crate) fn label(ui: &mut egui::Ui, text: &str, hint: &str) {
    let label = ui.label(text);
    if !hint.is_empty() {
        label.on_hover_text(hint);
    }
}

/// `button` as the primary one, the one way on in its place: filled with
/// the accent, its words, and any shortcut, in colours that read on it, and
/// outlined in it. Its words go in the headings' weight, as [`medium`]
/// makes them. Unless `enabled`, a button as any other, in weaker text.
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

/// A field of one line, as high as a button beside it, its text in its
/// middle.
pub(crate) fn text_field(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .min_size(vec2(0.0, theme::CONTROL))
        .vertical_align(egui::Align::Center)
}

/// Room inside a dialog's frame.
const DIALOG_MARGIN: i8 = 16;

/// How long a dialog takes to fade in as it opens, in seconds. It is where
/// it goes, as large as it is, and takes the keyboard from its first
/// frame, so nothing waits for it.
const DIALOG_FADE: f64 = 0.11;
/// How dark the rest of the GUI goes behind a dialog.
const BACKDROP: f32 = 100.0;

/// A dialog over the rest of the GUI, `id` telling it from others, framed
/// as every dialog is: its [`dialog_title`] first, its [`dialog_buttons`]
/// last.
pub(crate) fn dialog(_ctx: &egui::Context, id: &str) -> Dialog {
    let id = egui::Id::new(id);
    Dialog { id, area: egui::Modal::default_area(id) }
}

/// A dialog to show: an [`egui::Modal`] that fades in, with what is
/// behind it, a little faster than egui's own easings.
pub(crate) struct Dialog {
    id: egui::Id,
    area: egui::Area,
}

impl Dialog {
    /// Places it as `area` says, rather than in the window's middle.
    pub(crate) fn area(self, area: egui::Area) -> Dialog {
        Dialog { area, ..self }
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
                frame.show(ui, content).inner
            })
    }
}

/// How far the dialog `id` has faded in: from nothing as it opens, that
/// is when it wasn't shown the pass before.
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

/// A dialog's title, cut short where it is long, with a cross in the
/// corner, level with it, that closes the dialog: true once clicked.
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

/// A dialog's title in words, in the headings' weight.
pub(crate) fn title_text(ui: &egui::Ui, text: impl Into<String>) -> RichText {
    medium(ui, text, TITLE)
}

/// A dialog's row of buttons, under a line: `add` adds them from the right,
/// the one that closes it first, then the primary one, then the others;
/// what it adds left to right after them goes at the row's left.
pub(crate) fn dialog_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    // as high as a button: a layout from the right takes all the height
    // left otherwise, which a dialog keeps as it was when the window, or
    // its scale, leaves it less room
    let row = vec2(ui.available_width(), theme::CONTROL);
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    ui.allocate_ui_with_layout(row, layout, add).inner
}

/// A button of a dialog's row, as wide as the others at the least.
pub(crate) fn dialog_button<'a>(text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text).min_size(vec2(90.0, theme::CONTROL))
}

/// Width of the labels before settings.
pub(crate) const LABELS: f32 = 150.0;

/// A setting: its label, explained on hover, and its controls beside it,
/// both from the top. Unless `allowed`, all of it is greyed out, and the
/// label's hover says why.
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
                // as high as the first line of controls, the text in its middle
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
                match allowed {
                    Err(why) => {
                        label.on_disabled_hover_text(why);
                    }
                    Ok(()) if !hint.is_empty() => {
                        label.on_hover_text(hint);
                    }
                    Ok(()) => {}
                }
                ui.vertical(contents).inner
            })
            .inner
        })
        .inner;
    ui.add_space(AFTER_ROW);
    inner
}

/// One choice of a [`segments`] row: its value, label and explanation.
pub(crate) type Choice<T> = (T, String, String);

/// Whether a choice of [`segments`] can be picked.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pick {
    Yes,
    /// Greyed out, as this says why.
    No(String),
    /// Greyed out, as another setting rules it out, yet picking it changes
    /// that setting, as this says.
    Changes(String),
}

/// Options side by side as one control, the chosen one lit. `enabled`
/// says which can be picked, with why not. True when changed.
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

/// [`segments`], each one picked as `pick` says.
pub(crate) fn picking<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[Choice<T>],
    pick: impl Fn(T) -> Pick,
) -> bool {
    let room = ui.available_width();
    picking_in(ui, value, choices, pick, room, true)
}

/// [`segments`] on one line `room` wide, for a row that has no room for
/// another: their labels cut short where they don't fit, said in full on
/// hover.
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

/// The padding of segments that are short of room, rather than wrapped.
const TIGHT_PADDING: f32 = 4.0;

/// How a row of segments `widths` wide, apart by `gap`, fits `room`: the
/// segments of each line, and the most a segment of it may take, where it
/// takes less than they would, which cuts its label short.
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
            // the widest cut down first, as little as it takes
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

/// [`picking`] within `room`: tighter, then onto more lines where it may
/// `wrap`, and only then its labels cut short.
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
                    // cut short: said in full
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

/// `button` as a choice is, of [`segments`] or on its own: framed, as high
/// as a control, and lit and outlined where `selected`. Every choice is
/// drawn so.
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

/// One of several choices, on a button of its own styled as a
/// [`segments`] option. They wrap as words do, in a `horizontal_wrapped`
/// row; one too long for a line has one of its own, its words on as many
/// lines as they take, and only a word too long for the line is cut
/// short, said in full on hover.
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
    // a line of its own
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

/// Radius of a [`swatch`]'s colour, and of the ring round it when chosen
/// or hovered.
const SWATCH: f32 = 10.0;
const SWATCH_RING: f32 = 12.5;

/// A colour to choose, round, in a square as high as a control: `colour`,
/// or where there is none, a wheel of every hue, for a colour of one's
/// own. Where `chosen`, it is ticked in black or white, whichever reads on
/// it, and ringed in the text colour, which reads on the window whatever
/// the colour; `name` is said on hover and to screen readers.
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
            // white in the middle, each hue at the edge
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
    // apart from the window, though it be the window's colour
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
        // the keyboard's focus rings it in the accent instead
        if !response.has_focus() {
            painter.circle_stroke(centre, SWATCH_RING, egui::Stroke::new(1.5, text));
        }
    } else if response.hovered() && enabled {
        painter.circle_stroke(centre, SWATCH_RING, egui::Stroke::new(1.0, text));
    }
    response.on_hover_text(name)
}

/// How far an arrow key moves a [`colour_picker`]'s marks, of the whole
/// way across, and with Shift held.
const PICKER_STEP: f32 = 0.01;
const PICKER_LEAP: f32 = 0.1;

/// The steps asked of a part of a [`colour_picker`] this frame: by arrow
/// keys while it has the focus, left and right, and where `upright` up
/// and down, which it then keeps from moving the focus; and by screen
/// readers, which ask the square for lighter or darker and the strip for
/// the hue on or back. Each step is [`PICKER_STEP`] of the way, or
/// [`PICKER_LEAP`] with Shift.
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
        // which egui takes only from a frame after the focus came, so one
        // is drawn at once, before a key can move the focus on
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

/// A picker of opaque colours: a square of the shades of one hue, more
/// saturated to the right and lighter to the top, and a strip of the hues
/// under it, each taken with the pointer or, with the focus, the arrow
/// keys. True when `colour` changed.
pub(crate) fn colour_picker(ui: &mut egui::Ui, colour: &mut Color32) -> bool {
    use egui::ecolor::HsvaGamma;
    // the hue kept while the colour is grey, which has none of its own
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
    // read out as sliders, at what they show now
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
    // their range, and the steps screen readers may ask for
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
    // each drawn as a grid of the colours at its corners
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
    // the keyboard's focus outlined outside too, as the colours inside
    // may be the accent's own
    let focus = egui::Stroke::new(2.0, theme::mark(ui.visuals()));
    for (rect, response) in [(square, &square_response), (strip, &strip_response)] {
        let stroke = if response.has_focus() { focus } else { outline };
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
    }
    // where the colour is, ringed in black and white, which shows on any
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

/// [`segments`] for a value a preset may set.
pub(crate) fn preset_segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    base: T,
    choices: &[Choice<T>],
) -> bool {
    preset_picking(ui, value, base, choices, |_| Pick::Yes)
}

/// [`preset_segments`], each one picked as `pick` says.
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

/// `contents` looking greyed out while `greyed`, yet live: for settings
/// another rules out, which picking them changes. Hovered, they look as
/// any other, which tells them from those greyed out for good.
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

/// The explanation of the chosen option, under a row of them.
pub(crate) fn chosen_hint<T: PartialEq + Copy>(ui: &mut egui::Ui, value: T, choices: &[Choice<T>]) {
    if let Some((_, _, hint)) = choices.iter().find(|(option, ..)| *option == value)
        && !hint.is_empty()
    {
        self::hint(ui, hint.as_str());
    }
}

/// Side of the slot a [`reset`] button keeps.
const RESET: f32 = 20.0;

/// The small button that goes back to the preset's value, where `shown`.
pub(crate) fn reset(ui: &mut egui::Ui, shown: bool) -> bool {
    reset_to(ui, shown, &fl!("back-to-preset"))
}

/// A small button back to what `hint` says, where `shown`, in the accent
/// so it reads as a button though it has no frame. Its room is kept where
/// it isn't shown, so what follows it doesn't move as it comes and goes.
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

/// A checkbox for a flag a preset may set.
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

/// A number that is either automatic (`None`) or set: the two as
/// segments, `automatic` and Custom, and the number beside them, greyed
/// out while automatic. It shows `shown` then, the automatic value if it
/// is known, and a newly set number starts from it, or from `start`.
/// `automatic` is the label of its segment, and what it means on hover.
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
        // greyed out here or around: shown as it is, even outside the range
        // a set value keeps to, and never stored, as egui would clamp it
        // and say it changed
        let live = set && ui.is_enabled();
        let mut number = value.or(shown).unwrap_or(start);
        let mut field = edit(Number::new(&mut number)).clamp_existing_to_range(live);
        // unknown: a dash, without the unit
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

/// A number field, where `set`; else the value it stands for as weak text
/// in its place, which can't be typed into, so a box means it can.
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
        // as weak as text, rather than greyed out further, unless all
        // around it is greyed out too
        visuals.disabled_alpha = 1.0;
        ui.add_enabled(false, field)
    })
    .inner
}

// Every number field is a [`Number`], so that they all take typing, keys,
// dragging and the wheel alike. What is typed is only taken as it is done
// with, by Enter, Tab or the pointer pressed elsewhere: values that follow
// from the field, as the crop does from a side of its ratio, would
// otherwise follow each digit on the way, as 1 of 16, and stay where that
// left them. So Escape can still put it back as it was.

/// How far dragging a number field takes it across its whole range, in
/// points, at the most: a field of a wide range is dragged faster than its
/// speed says.
const SCRUB_ACROSS: f64 = 400.0;
/// How long a number field's outline shows that what was typed in it
/// wasn't a number, in seconds.
const REFUSED_FOR: f64 = 0.6;

/// How a number field steps the number it holds, from it, by so many
/// steps, back where negative, larger ones where the second is true.
type Stepper<'a> = Box<dyn 'a + Fn(f64, f64, bool) -> f64>;
type Formatter<'a> = Box<dyn 'a + Fn(f64, std::ops::RangeInclusive<usize>) -> String>;
type Parser<'a> = Box<dyn 'a + Fn(&str) -> Option<f64>>;

/// A field of a number, as [`egui::DragValue`] is, set up the same way,
/// that takes what is typed only once it is done with; Escape puts back
/// what it held as typing began. Up and Down step it while it is typed
/// in, as does the wheel over it then, by its [`steps`](Number::steps),
/// with Shift by its larger ones. Dragging goes across its range in
/// [`SCRUB_ACROSS`] points at the most, ten times as slowly with Shift.
/// It reads its own units typed with the number, and a decimal comma;
/// what it can't read is not taken, and its outline flashes in the colour
/// of what went wrong.
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
    /// A unit read typed with the number, though not shown with it.
    reads: &'static str,
    max_decimals: Option<usize>,
    formatter: Option<Formatter<'a>>,
    parser: Option<Parser<'a>>,
}

/// What a number field being typed in holds.
#[derive(Clone, Debug, Default)]
struct Typing {
    text: String,
    /// The text as written from the value, which is left as it is unless
    /// changed.
    written: String,
    /// The value as typing began, which Escape puts back.
    start: f64,
    /// The value as the text was written or taken: another one is a
    /// change made elsewhere, after which the text is written again.
    value: f64,
    /// Notches of the wheel turned, short of a step.
    wheel: f32,
    /// The pass it last had the focus in: what was typed in a field that
    /// lost it longer ago, as by going out of sight, is dropped.
    pass: u64,
}

/// The id under which the number field being typed in is kept, with the
/// file it is of; the pass that a [`take_typed_number`] asks it to be taken
/// in; and the file whose settings are being drawn, as [`of_file`] says.
const TYPED: &str = "number typed";
const TAKE_TYPED: &str = "take number typed";
const DRAWING: &str = "file drawn";

/// The number field typed in last, and the file whose settings it is in,
/// where it is: so that what is typed in it is taken before the file goes
/// out of sight.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Typed {
    pub(crate) field: egui::Id,
    pub(crate) file: Option<u64>,
}

/// Whether a number field is being typed in.
pub(crate) fn typing_number(ctx: &egui::Context) -> bool {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
    typed.is_some_and(|typed| ctx.memory(|memory| memory.has_focus(typed.field)))
}

/// The file whose number field holds what was typed in it, not yet
/// taken: a number typed but not finished with; forgotten once its field
/// has been out of sight too long.
pub(crate) fn typed_number(ctx: &egui::Context) -> Option<u64> {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)))?;
    let typing = ctx.data(|data| data.get_temp::<Typing>(typed.field))?;
    // as the field itself would, drawn so long after
    if ctx.cumulative_pass_nr() > typing.pass + 2 {
        ctx.data_mut(|data| data.remove::<Typing>(typed.field));
        return None;
    }
    typed.file.filter(|_| typing.text != typing.written)
}

/// Has the number field typed in be done with, as a click elsewhere does:
/// it takes what was typed in it as it is next drawn, this frame.
pub(crate) fn finish_typed_number(ctx: &egui::Context) {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
    if let Some(typed) = typed {
        ctx.memory_mut(|memory| memory.surrender_focus(typed.field));
    }
}

/// Forgets what was typed in the number field typed in last, which is
/// not to be taken, as its file is gone.
pub(crate) fn forget_typed_number(ctx: &egui::Context) {
    let typed = ctx.data(|data| data.get_temp::<Typed>(egui::Id::new(TYPED)));
    if let Some(typed) = typed {
        ctx.memory_mut(|memory| memory.surrender_focus(typed.field));
        ctx.data_mut(|data| data.remove::<Typing>(typed.field));
    }
}

/// Has the number field being typed in take what is typed this frame, as
/// Enter would, but keep its focus: for a key that goes on to use it.
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
        }
    }

    pub(crate) fn range<N: egui::emath::Numeric>(
        mut self,
        range: std::ops::RangeInclusive<N>,
    ) -> Self {
        self.range = range.start().to_f64()..=range.end().to_f64();
        self
    }

    /// Whether a value outside the range is brought into it as the field
    /// is shown, rather than only as it is changed.
    pub(crate) fn clamp_existing_to_range(mut self, clamp: bool) -> Self {
        self.clamp_existing = clamp;
        self
    }

    /// How much dragging a point changes it, at the least, which also sets
    /// how many decimals it shows, as egui's fields have it.
    pub(crate) fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// What Up, Down and the wheel step it by: `small`, and `large` with
    /// Shift. Whole numbers step by 1 and 10 otherwise.
    pub(crate) fn steps(mut self, small: f64, large: f64) -> Self {
        self.steps = (small, large);
        self
    }

    /// Steps it as `stepper` says instead, from a value by so many steps,
    /// larger ones where asked, as by frames, or a second.
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

    /// Reads `unit` typed with the number too, though it doesn't show it.
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

    /// How it reads what is typed, once its units and spaces are taken
    /// off it and a comma made a point.
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

    /// `value` as the field keeps one: whole where it holds whole
    /// numbers, and within its range.
    fn kept(&self, value: f64) -> f64 {
        self.clamped(if self.integral { value.round() } else { value })
    }

    /// `value` stepped `steps` times, larger ones where `large` is true.
    fn stepped(&self, value: f64, steps: f64, large: bool) -> f64 {
        let next = match &self.stepper {
            Some(stepper) => stepper(value, steps, large),
            None => value + steps * if large { self.steps.1 } else { self.steps.0 },
        };
        // without what adding tenths leaves, as 0.30000000000000004
        self.kept(egui::emath::round_to_decimals(next, 9))
    }

    /// How fast dragging changes it, a point at a time.
    fn scrub_speed(&self) -> f64 {
        let span = self.range.end() - self.range.start();
        // a range egui's would have, as wide as numbers go, doesn't count
        match span.is_finite() && span < 1e9 {
            true => self.speed.max(span / SCRUB_ACROSS),
            false => self.speed,
        }
    }

    /// The decimals it shows, as egui's fields work them out.
    fn decimals(&self, ui: &egui::Ui) -> std::ops::RangeInclusive<usize> {
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

    /// What `text` typed says, if it is a number.
    fn parse(&self, text: &str) -> Option<f64> {
        let text = lenient(text, &[self.prefix.trim(), self.suffix.trim(), self.reads.trim()]);
        let value = match &self.parser {
            Some(parser) => parser(&text),
            None => text.parse().ok(),
        };
        value.filter(|value: &f64| value.is_finite())
    }

    /// Takes what is typed, unless it is as it was written: kept as the
    /// field keeps numbers, or, where it isn't one, nothing, the field's
    /// outline flashing.
    fn take(&mut self, ui: &egui::Ui, id: egui::Id, typing: &Typing) -> Option<f64> {
        if typing.text == typing.written {
            return None;
        }
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

    /// What the field holds as typing in it begins, or goes on from
    /// `value`, written as it shows.
    fn typing(&self, ui: &egui::Ui, value: f64, start: f64) -> Typing {
        let text = self.format(ui, value);
        Typing { written: text.clone(), text, start, value, ..Typing::default() }
    }

    /// What typed steps go from: the number typed, where one is, or else
    /// the value.
    fn typed_or(&self, typing: &Typing, value: f64) -> f64 {
        let typed = (typing.text != typing.written).then(|| self.parse(&typing.text)).flatten();
        typed.map_or(value, |typed| self.kept(typed))
    }
}

/// `text` as a number field reads it: without spaces, nor `units` at
/// either end, its own as it shows them, a comma for a decimal point, and
/// the minus sign egui writes as a hyphen. × is read as x too.
fn lenient(text: &str, units: &[&str]) -> String {
    let mut text: String = text
        .trim()
        .chars()
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
    // longer first, so that kbit/s goes before an s alone would
    all.sort_by_key(|unit| std::cmp::Reverse(unit.len()));
    loop {
        let before = text.len();
        for unit in &all {
            let unit_lower = unit.to_lowercase();
            let lower = text.to_lowercase();
            // the same length lower-cased, as units are written in ASCII
            // but for ×
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

/// The wheel's notches turned over a number field this frame, up the
/// positive way, as Shift says for each, and without those that zoom.
fn notches(ui: &egui::Ui) -> (f32, f32) {
    let line = ui.ctx().options(|options| options.input_options.line_scroll_speed);
    ui.input(|input| {
        let (mut plain, mut shifted) = (0.0, 0.0);
        for event in &input.raw.events {
            let egui::Event::MouseWheel { unit, delta, modifiers, .. } = event else { continue };
            if modifiers.command || modifiers.ctrl {
                continue;
            }
            // Shift turns the wheel sideways on some systems
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

/// Takes this frame's Escape, which put back what a field held: it is the
/// field's, not a dialog's around it to close by, nor anything else's.
pub(crate) fn escape_taken(ui: &egui::Ui) {
    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
}

/// Selects all of `text` in the field `id`, as typing in it begins, so
/// that what is typed takes its place.
fn select_all(ui: &egui::Ui, id: egui::Id, text: &str) {
    use egui::text::{CCursor, CCursorRange};
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    let all = CCursorRange::two(CCursor::default(), CCursor::new(text.chars().count()));
    state.cursor.set_char_range(Some(all));
    state.store(ui.ctx(), id);
}

/// Puts the cursor of the field `id` at the end of `text`, as after a step.
fn cursor_to_end(ui: &egui::Ui, id: egui::Id, text: &str) {
    use egui::text::{CCursor, CCursorRange};
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(text.chars().count()))));
    state.store(ui.ctx(), id);
}

impl egui::Widget for Number<'_> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        use egui::accesskit::{Action, ActionData};
        // the same whether typed in or not, as egui's fields have it
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
        // the focus gone before the field was drawn, as by Escape, Tab or
        // another field taking it: what was typed is done with, unless the
        // value changed elsewhere since, or the field went out of sight
        // with the focus a while ago
        if !editing && let Some(typing) = &typing {
            ui.data_mut(|data| data.remove::<Typing>(id));
            let lately = ui.ctx().cumulative_pass_nr() <= typing.pass + 2;
            if lately && typing.value == value {
                if escaped {
                    value = typing.start;
                    self.set(value);
                    escape_taken(ui);
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
            // a key that uses the value, as Convert's: taken first, the
            // focus kept
            let take_asked = ui.data(|data| data.get_temp::<u64>(egui::Id::new(TAKE_TYPED)))
                == Some(ui.ctx().cumulative_pass_nr());
            if take_asked && typing.text != typing.written {
                if let Some(taken) = self.take(ui, id, &typing) {
                    value = taken;
                }
                typing = self.typing(ui, value, value);
                cursor_to_end(ui, id, &typing.text);
            }
            // Shift's first, as those without it take Shift's too
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
            // the wheel steps it while it is typed in and under the
            // pointer, rather than scrolling what it is in; only over it
            // otherwise, it scrolls as ever
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
            // done with as the pointer goes down elsewhere, as in a browser,
            // rather than as it comes up: what was typed is then taken in
            // the frame before what the click does, which may show another
            // file's settings, and is a step of its own
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
                // done with by Enter, or a press elsewhere
                ui.data_mut(|data| data.remove::<Typing>(id));
                if escaped {
                    value = typing.start;
                    self.set(value);
                    escape_taken(ui);
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
            // the value dragged to, unrounded, from the press on
            if ui.input(|input| input.pointer.any_pressed() || input.pointer.any_released()) {
                ui.data_mut(|data| data.remove::<f64>(id));
            }
            if response.clicked() {
                ui.data_mut(|data| data.remove::<Typing>(id));
                ui.memory_mut(|memory| memory.request_focus(id));
            } else if response.dragged() {
                ui.set_cursor_icon(cursor);
                let delta = response.drag_delta();
                // larger to the right and up
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
        // what was typed refused: the outline in the colour of what went
        // wrong a moment, fading, over what is drawn, so nothing moves
        let refused = id.with("refused");
        if let Some(at) = ui.data(|data| data.get_temp::<f64>(refused)) {
            let age = ui.input(|input| input.time) - at;
            if (0.0..REFUSED_FOR).contains(&age) {
                // still, for as long, without motion
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
            // as it shows, with its units; typed in, the text it holds
            if !editing {
                builder.set_value(shown);
            }
        });
        response
    }
}

/// How long a stepper's button is held before it steps again, how often
/// it steps then, and how long until it steps faster, how often then.
const REPEAT_AFTER: f64 = 0.4;
const REPEAT_EVERY: f64 = 1.0 / 8.0;
const FASTER_AFTER: f64 = 1.5;
const FASTER_EVERY: f64 = 1.0 / 20.0;

/// How many steps a stepper's button held for `held` seconds has taken:
/// one as it was pressed, and those it repeats.
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

/// How long after `held` seconds a stepper's button next steps.
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

/// A − or + button, `text`, as tall as a row and as wide, named `name`
/// and saying it on hover: true as it steps, once as it is pressed, or
/// clicked from the keyboard, and again while it is held there, faster
/// after a while, until it is let go of or greyed out, as at the end of
/// what it steps through; not while the pointer is off it.
pub(crate) fn stepper(
    ui: &mut egui::Ui,
    text: &str,
    enabled: bool,
    name: &str,
) -> (egui::Response, bool) {
    // as tall as a row, and square
    let side = ui.spacing().interact_size.y;
    // dragging too, as egui forgets a press that lasts too long for a click
    let sense = egui::Sense::click_and_drag();
    let button = egui::Button::new(text).min_size(vec2(side, side)).sense(sense);
    let response = named(ui.add_enabled(enabled, button), name);
    let (id, here) = (response.id.with("held"), response.id.with("stepped here"));
    let now = ui.input(|input| input.time);
    // since when it is held, and how many steps it took since
    let since: Option<(f64, u32)> = ui.data(|data| data.get_temp(id));
    // pressed on it, and held; while the pointer is off it, the steps
    // due then are skipped, as for the frame its scale changes, before
    // egui finds it under the pointer again
    let held = enabled && response.is_pointer_button_down_on();
    let over = response.contains_pointer();
    let stepped = match (held, since) {
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
            // a click it wasn't held for, as by the keyboard
            since.is_none() && response.clicked()
        }
    };
    if held {
        let from = since.map_or(now, |(from, _)| from);
        let wait = next_step(now - from).max(1e-3);
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    // what it is said once the pointer comes back to it, rather than
    // after a step as it rests there: the scale a step changes moves the
    // pointer for egui, which then doesn't take the press for a click,
    // after which it would say nothing
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
        // only its own
        assert_eq!(lenient("3 fps", &["px"]), "3 fps");
    }

    #[test]
    fn steps_held_down_faster_after_a_while() {
        assert_eq!(steps_held(0.0), 1);
        assert_eq!(steps_held(0.39), 1);
        assert_eq!(steps_held(0.41), 2);
        // 8 a second
        assert_eq!(steps_held(0.41 + 0.125), 3);
        assert_eq!(steps_held(1.45), 10);
        // 20 a second after 1.5 s
        assert_eq!(steps_held(1.51), 11);
        assert_eq!(steps_held(2.01), 21);
        assert!((next_step(0.1) - 0.3).abs() < 1e-9);
        assert!((next_step(0.45) - 0.075).abs() < 1e-9);
        assert!((next_step(1.52) - 0.03).abs() < 1e-9);
    }

    /// A field of `value` set up by `field`, in a window, typed in once
    /// focused, and where it is.
    fn field_of<T: egui::emath::Numeric + Send + Sync>(
        value: T,
        field: impl Fn(Number<'_>) -> Number<'_> + 'static,
    ) -> Harness<'static, (T, Option<egui::Rect>)> {
        let mut harness = Harness::new_ui_state(
            move |ui, (value, rect): &mut (T, Option<egui::Rect>)| {
                // as wide as its widest, as the GUI has them
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

    #[test]
    fn takes_what_is_typed_once_done_with_it() {
        let mut harness = field_of(8_u32, |field| field.range(1..=100).suffix(" px"));
        let rect = harness.state().1;
        focus(&mut harness);
        harness.event(egui::Event::Text("16".into()));
        harness.run_steps(2);
        // not until it is done with
        assert_eq!(harness.state().0, 8);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 16);
        // Tab is done with it too, units, commas and all
        focus(&mut harness);
        harness.event(egui::Event::Text("40 px".into()));
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 40);
        // and outside the range, kept within it
        focus(&mut harness);
        harness.event(egui::Event::Text("500".into()));
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 100);
        // where it is, as it was all along
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
    }

    #[test]
    fn puts_back_what_it_held_on_escape() {
        let mut harness = field_of(8_u32, |field| field.range(1..=100));
        let rect = harness.state().1;
        focus(&mut harness);
        harness.event(egui::Event::Text("16".into()));
        harness.run_steps(2);
        // stepped, then typed in, then let go of
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 17);
        harness.event(egui::Event::Text("3".into()));
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert_eq!(harness.state().0, 8);
        assert!(!harness.ctx.text_edit_focused());
        assert_eq!(harness.state().1, rect);
        // what is typed is undone as in any text field, before it is taken:
        // back to the number as it was, which is left as it is
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
        // a moment, then gone, a quarter of a second a frame
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
            // the value at once, not waiting to be done with
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
        // stopping at its ends
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
        // 400 points to the right, a little at a time
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
        // over it alone: the area scrolls, and it stays as it is
        wheel(&mut harness, -1.0);
        assert_eq!(harness.state().0, 50);
        let scrolled = harness.state().1;
        assert!(scrolled > 0.0);
        // back up, then typed in: a step a notch, the area kept still
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
        // a click: one step
        harness.event(button(true));
        harness.event(button(false));
        harness.run_steps(2);
        assert_eq!(harness.state().0, 1);
        // held: one as it is pressed, then after 0.4 s at 8 a second, and
        // after 1.5 s at 20
        harness.state_mut().0 = 0;
        harness.event(button(true));
        harness.run_steps(1);
        assert_eq!(harness.state().0, 1);
        harness.run_steps(35);
        assert_eq!(harness.state().0, 1);
        harness.run_steps(10);
        assert_eq!(harness.state().0, 2);
        harness.run_steps(55);
        // at 1.0 s: 0.4, 0.525 .. 0.9
        assert_eq!(harness.state().0, 6);
        harness.run_steps(101);
        // at 2.01 s: 9 slow, to 1.4 s, and 1.5 to 2.0 fast
        assert_eq!(harness.state().0, 1 + 9 + 11);
        // let go: no more
        harness.event(button(false));
        harness.run_steps(100);
        assert_eq!(harness.state().0, 21);
        // pressed and left: stops
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
        // greyed out, at its end: stops
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

/// An [`auto_number`] for a value a preset may set. While the preset sets
/// one, that is what the automatic choice stands for: it is named the
/// preset's and shows the preset's value, which a number of the user's own
/// starts from. Without one, the choice is `automatic`, showing `shown`.
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

/// The label of a choice that stands for the preset's value, and its hint.
pub(crate) fn from_preset() -> (String, String) {
    (fl!("from-preset"), fl!("from-preset", "hint"))
}

/// How much of a limit something uses: a bar, red when over.
pub(crate) fn gauge(ui: &mut egui::Ui, used: u64, limit: u64, text: String) {
    let share = used as f32 / limit.max(1) as f32;
    let colour = if share > 1.0 { theme::NEGATIVE } else { theme::POSITIVE };
    bar(ui, Fill::Share(share), &text, colour.gamma_multiply(0.55));
}

/// How far a [`bar`] got.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Fill {
    /// This share of it, done.
    Share(f32),
    /// This share, still at work: a band moves over it, so a share that
    /// takes a while to change doesn't look stuck.
    Working(f32),
    /// At work, how far along unknown: only the band.
    Unknown,
}

impl Fill {
    /// At work: this share, or unknown without one.
    pub(crate) fn working(share: Option<f32>) -> Fill {
        share.map_or(Fill::Unknown, Fill::Working)
    }
}

/// A bar filled as `fill` says, `text` over it.
pub(crate) fn bar(ui: &mut egui::Ui, fill: Fill, text: &str, colour: Color32) {
    let width = ui.available_width().min(380.0);
    bar_sized(ui, egui::vec2(width, BAR), fill, text, colour);
}

/// Height of a [`bar`] of a line.
const BAR: f32 = 22.0;
/// The font of what a bar says.
const BAR_FONT: f32 = 12.5;

/// A [`bar`] as tall as `widest` needs, the longest it may say as it
/// goes on, its words wrapped onto as many lines as they take where the
/// bar is narrow, so that it says all of `text` and keeps its height as
/// that changes.
pub(crate) fn steady_bar(ui: &mut egui::Ui, fill: Fill, text: &str, widest: &str, colour: Color32) {
    let width = ui.available_width().min(380.0);
    let size = egui::vec2(width, bar_height(ui, &[text, widest], width));
    bar_drawn(ui, size, fill, text, colour, None);
}

/// How tall a bar `width` wide is to say the longest of `texts` whole.
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

/// A [`bar`] of `size`.
pub(crate) fn bar_sized(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fill: Fill,
    text: &str,
    colour: Color32,
) {
    bar_drawn(ui, size, fill, text, colour, None);
}

/// A full [`bar`] of a step done with, as an encode too large to keep: in
/// a neutral shade rather than a colour, as it is an ordinary step, its
/// `text` from byte `from` on, which says how it went, in the warning
/// colour.
pub(crate) fn bar_spent(ui: &mut egui::Ui, text: &str, from: usize) {
    let width = ui.available_width().min(380.0);
    let (fill, ending) = (theme::spent(ui.visuals()), theme::warn(ui.visuals()));
    let size = egui::vec2(width, bar_height(ui, &[text], width));
    bar_drawn(ui, size, Fill::Share(1.0), text, fill, Some((from, ending)));
}

/// How long a bar's fill takes to go most of the way to a share it moved
/// up to, a share `1 - 1/e` of it, in seconds: progress comes a few times a
/// second, and would step otherwise.
const FILL_EASING: f32 = 0.12;
/// How long the band over a bar at work takes to cross it, in seconds.
const SHEEN: f64 = 1.6;

/// How much of the bar `id` shows filled for `fill`: the share eased up
/// toward, and at once where it went down, as for a new encode, or is
/// whole; `None` where how far is unknown.
fn shown_share(ctx: &egui::Context, id: egui::Id, fill: Fill) -> Option<f32> {
    let (Fill::Share(share) | Fill::Working(share)) = fill else { return None };
    let share = share.clamp(0.0, 1.0);
    let jump = |shown: f32| share < shown || share >= 1.0;
    Some(crate::motion::approach(ctx, id.with("fill"), share, FILL_EASING, jump))
}

/// Where the band over a bar at work is at `time`, from 0, before its
/// start, to 1, past its end: slow at each end and fast across the middle,
/// so that it eases in and out of sight rather than sweeping on and on.
fn sheen_at(time: f64) -> f32 {
    let phase = (time / SHEEN).fract() as f32;
    phase * phase * (3.0 - 2.0 * phase)
}

/// A [`bar`] of `size`, its text from the byte `ending` gives on in the
/// colour it gives.
fn bar_drawn(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fill: Fill,
    text: &str,
    colour: Color32,
    ending: Option<(usize, Color32)>,
) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rounding = (size.y / 2.0).min(4.0);
    let painter = ui.painter_at(rect);
    let (track, outline) = theme::track(ui.visuals());
    painter.rect_filled(rect, rounding, track);
    // the whole bar's shape, cut to how far it got: no blob at its start
    // when it is nearly empty
    if let Some(share) = shown_share(ui.ctx(), response.id, fill) {
        let width = rect.width() * share;
        let part = egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height()));
        ui.painter_at(part).rect_filled(rect, rounding, colour);
    }
    let moving = crate::motion::motion(ui.ctx());
    // still, at work with how far unknown: stripes over it all
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
        // over a share, a sheen that shows on what is filled and what isn't
        let shade = match fill {
            Fill::Unknown => colour,
            _ => ui.visuals().strong_text_color().gamma_multiply(0.14),
        };
        ui.painter_at(part.intersect(rect)).rect_filled(rect, rounding, shade);
        ui.ctx().request_repaint();
    }
    painter.rect_stroke(rect, rounding, outline, egui::StrokeKind::Inside);
    // on as many lines as the bar is tall enough for, and cut short only
    // where it isn't, said in full on hover
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

/// Telegram's problems with a result, or that it has none.
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

/// Room at the right of what scrolls for its bar, kept whether the bar
/// shows or not, so nothing moves as it comes and goes.
pub(crate) const GUTTER: f32 = 12.0;
/// How far what is cut off at either end of what scrolls fades out.
pub(crate) const SCROLL_FADE: f32 = 24.0;

/// Shows `area`, with what `add` adds in it, as everything that scrolls
/// is shown: that there is more shows, as its bar is always drawn where
/// it doesn't all fit, rather than as the pointer comes near, in a gutter
/// kept for it either way, and what is cut off fades out at the bottom
/// until it is scrolled to its end, and at the top once scrolled.
pub(crate) fn scrolled<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let width = ui.available_width();
    let outer = ui.spacing().scroll;
    ui.scope(|ui| {
        let mut style = egui::style::ScrollStyle::solid();
        style.bar_outer_margin = GUTTER - style.bar_width - style.bar_inner_margin;
        style.fade = egui::style::ScrollFadeStyle { strength: 1.0, size: SCROLL_FADE };
        ui.spacing_mut().scroll = style;
        area.show(ui, |ui| {
            // what scrolls within as it would elsewhere
            ui.spacing_mut().scroll = outer;
            ui.set_max_width(width - GUTTER);
            let added = add(ui);
            // never shorter while the pointer is held than as the frame
            // before left it: scrolled to its end, what is in sight would
            // move down as it shrinks, and from under the pointer, as a
            // button held to repeat whose step shortens what follows
            let tallest = ui.id().with("tallest while held");
            let before = ui.data(|data| data.get_temp::<f32>(tallest));
            if let Some(before) = before
                && ui.input(|input| input.pointer.any_down())
            {
                ui.set_min_height(before);
            }
            let height = ui.min_rect().height();
            ui.data_mut(|data| data.insert_temp(tallest, height));
            added
        })
    })
    .inner
}

/// Draws a checkerboard, so transparency shows, its squares counted from
/// `rect`'s corner.
pub(crate) fn checkerboard(painter: &egui::Painter, rect: egui::Rect, square: f32) {
    checkerboard_from(painter, rect, square, rect.min);
}

/// A [`checkerboard`] over `rect` whose squares are counted from `origin`
/// rather than from its corner: a picture that zooms or pans over the
/// squares of the stage under it, which stay where they are.
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
    // on whole screen pixels, so the squares' edges stay sharp
    let origin = egui::emath::GuiRounding::round_to_pixels(origin, painter.pixels_per_point());
    // only the squares in view, counted from the origin, which may be
    // anywhere
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

/// A tick across a bar at `x`, from `top` to `bottom`, that shows on
/// whatever is under it, the accent of any hue among it: a core two
/// points wide, cased a point wide all round, as [`theme::over_fill`]
/// colours them, on whole screen pixels so it stays sharp. Gives the
/// tick's outer edges, left and right.
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
    // the core's left edge on a whole pixel, as near the middle as it goes
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

/// An outline of `rect` that shows on whatever is in it and around it, the
/// accent of any hue among it: a line of [`theme::over_fill`]'s core,
/// cased inside.
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

/// Hatches `rect` with stripes of `colour` leaning right, as wide as the
/// gaps between them, counted from `origin`, so that they stay where they
/// are as `rect`'s edges move over them.
pub(crate) fn hatch(painter: &egui::Painter, rect: egui::Rect, origin: f32, colour: Color32) {
    /// How far apart the stripes are, along the bar.
    const PERIOD: f32 = 6.0;
    let clip = painter.clip_rect().intersect(rect);
    if !clip.is_positive() {
        return;
    }
    let painter = painter.with_clip_rect(clip);
    // at 45°, half of each period across the stripes
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

/// Gives a test's bare `harness` the GUI's fonts. egui's own are left
/// out of the build, and text without glyphs has no place for a cursor but
/// its start, so what is typed would go in backwards.
#[cfg(test)]
pub(crate) fn test_fonts<State>(harness: &egui_kittest::Harness<'_, State>) {
    harness.ctx.set_fonts(crate::fonts::fonts());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_scrolls_as_long_while_the_pointer_is_held() {
        // a button with lines after it, scrolled to their end, and where
        // the button is
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
        // shorter while it is held: what is at the end stays put
        harness.state_mut().0 = 1;
        harness.run_steps(3);
        assert!(harness.state().1.contains(at), "{:?} from {at:?}", harness.state().1);
        // and once let go of, scrolled as far as it then goes
        harness.event(press(false));
        harness.run_steps(3);
        assert!(!harness.state().1.contains(at));
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
        // an automatic value outside the range a set one keeps to, like the
        // speed that fits a minute into 3 seconds
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
        // a set value, greyed out with all around it, outside the range
        // the row has now
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
        // shown as it is, not clamped to the range
        use egui_kittest::kittest::Queryable;
        harness.get(egui_kittest::kittest::by().value("20"));
    }

    #[test]
    fn breaks_lines_where_the_widest_number_would() {
        let like = "Plays all 00.00 s: Telegram is told";
        // broken after "all" and after "s:" in the widest
        let breaks = [10, 19];
        let text = "Plays all 9.55 s: Telegram is told";
        assert_eq!(broken_as(text, like, &breaks), "Plays all\n9.55 s:\nTelegram is told");
        // within the number itself: after it
        assert_eq!(broken_as("a 1.5 b", "a 00.00 b", &[4]), "a 1.5\nb");
        // and the widest itself as it is
        assert_eq!(broken_as(like, like, &breaks), "Plays all\n00.00 s:\nTelegram is told");
        assert_eq!(fixed(2.0, 2), "2.00");
        assert_eq!((speed(1.5), fps(29.97)), ("1.50".to_owned(), "29.97".to_owned()));
        assert_eq!(widest_seconds(14.55), ["00.00 s"]);
        assert_eq!(widest_seconds(145.5), ["00.00 s", "0:00.00"]);
    }

    #[test]
    fn keeps_a_hint_on_its_lines_as_its_number_changes() {
        type State = (bool, bool);
        // at each width, the hint of each time in its lines: their number,
        // and the line the time is on
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
        // as wrapped plainly, a time of more digits moves words to the
        // next line at some width; never so here
        assert!(steady && !plain, "steady {steady}, plain {plain}");
    }

    #[test]
    fn writes_times() {
        assert_eq!(seconds(2.5), "2.50 s");
        assert_eq!(seconds(62.5), "1:02.50");
        assert_eq!(seconds(119.996), "2:00.00");
        // what is written reads back, to the hundredth
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
        // a bar's fill, as it shows each frame, a sixtieth of a second apart
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
        // eased from where it was: a little way at first
        let first = harness.state().1.unwrap();
        assert!(first > 0.2 && first < 0.35, "{first}");
        // most of the way a time constant on, and there in the end
        harness.run_steps(6);
        let mid = harness.state().1.unwrap();
        let expected = 0.8 - 0.6 * (-7.0 / 60.0 / FILL_EASING).exp();
        assert!((mid - expected).abs() < 0.03, "{mid}, not {expected}");
        harness.run_steps(60);
        assert_eq!(harness.state().1, Some(0.8));
        // a new encode, from nothing: at once
        harness.state_mut().0 = Fill::Working(0.05);
        harness.step();
        assert_eq!(harness.state().1, Some(0.05));
        // done: whole at once, and nothing drawn again after
        harness.state_mut().0 = Fill::Share(1.0);
        harness.step();
        assert_eq!(harness.state().1, Some(1.0));
        harness.try_run().expect("still once there");
        // a share done with: eased, then still
        harness.state_mut().0 = Fill::Share(0.3);
        harness.step();
        harness.state_mut().0 = Fill::Share(0.6);
        harness.try_run().expect("still once there");
        assert_eq!(harness.state().1, Some(0.6));
        // without motion, there at once
        crate::motion::set(&harness.ctx, false);
        harness.state_mut().0 = Fill::Share(0.9);
        harness.step();
        assert_eq!(harness.state().1, Some(0.9));
        // and at work with how far unknown, striped and still
        harness.state_mut().0 = Fill::Unknown;
        harness.try_run().expect("still once there");
    }

    #[test]
    fn fades_a_dialog_in_where_it_goes_from_its_first_frame() {
        // whether it is open, and each frame how much of it shows and where
        // what is in it is
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
                // from nothing, about halfway three frames on, whole in 0.11 s
                assert!(opacities[0] < 0.01 && opacities[3] > 0.3 && opacities[3] < 0.7);
                assert!(opacities[7..].iter().all(|&opacity| opacity == 1.0), "{opacities:?}");
            } else {
                assert!(opacities.iter().all(|&opacity| opacity == 1.0), "{opacities:?}");
            }
            // where it goes from the first frame it is seen, once measured
            assert!(places[1..].iter().all(|&place| place == places[9]), "{places:?}");
            harness.try_run().expect("still once shown");
            harness.state_mut().0 = false;
            harness.run_steps(2);
        }
    }

    #[test]
    fn keeps_room_for_the_scroll_bar_whether_it_shows_or_not() {
        // how many lines to show, and how wide they were and where they
        // were seen from, as the bar comes and goes
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
        // the bar in the gutter kept for it, which the lines never had
        assert_eq!(few, many);
        assert!(narrower < room && few <= narrower, "{few} {narrower} {room}");
        assert_eq!(few, harness.ctx.content_rect().width() - 16.0 - GUTTER);
    }

    #[test]
    fn eases_the_band_over_a_bar_at_work_in_and_out() {
        assert_eq!(sheen_at(0.0), 0.0);
        // slow from the start, halfway at half its time, fast across it
        assert!(sheen_at(SHEEN * 0.1) < 0.05);
        assert!((sheen_at(SHEEN * 0.5) - 0.5).abs() < 1e-4);
        assert!(sheen_at(SHEEN * 0.55) - sheen_at(SHEEN * 0.45) > 0.14);
        assert!(sheen_at(SHEEN * 0.99) > 0.99);
        // and over again
        assert!(sheen_at(SHEEN * 1.1) < 0.05);
    }
}

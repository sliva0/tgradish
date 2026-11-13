//! Small pieces the window is built from.

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

/// Size of a section's heading, in the headings' weight: hardly larger
/// than the text under it, as the weight sets it apart.
pub(crate) const HEADING: f32 = 14.5;
/// Size of a dialog's title, and of the window's other headings.
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

/// Number fields added by `add` as wide as `digits` digits and `suffix`
/// take, so that they keep their width as the number changes, typed or
/// dragged: Ubuntu's digits are all as wide as each other.
pub(crate) fn steady_numbers<R>(
    ui: &mut egui::Ui,
    digits: usize,
    suffix: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        let text = "0".repeat(digits) + suffix;
        ui.spacing_mut().interact_size.x =
            text_width(ui, &text, 2.0 * ui.spacing().button_padding.x);
        add(ui)
    })
    .inner
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

/// The label column of a grid, explained on hover.
pub(crate) fn label(ui: &mut egui::Ui, text: &str, hint: &str) {
    let label = ui.label(text);
    if !hint.is_empty() {
        label.on_hover_text(hint);
    }
}

/// `button` as the primary one, the one way on in its place: filled with
/// the accent and outlined in it. Its words go in the headings' weight, as
/// [`medium`] makes them. Greyed out unless `enabled`.
pub(crate) fn primary(
    ui: &mut egui::Ui,
    enabled: bool,
    button: egui::Button<'_>,
) -> egui::Response {
    ui.scope(|ui| {
        theme::primary(ui.visuals_mut());
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

/// A dialog over the window, `id` telling it from others, framed as every
/// dialog is: its [`dialog_title`] first, its [`dialog_buttons`] last.
pub(crate) fn dialog(ctx: &egui::Context, id: &str) -> egui::Modal {
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(DIALOG_MARGIN);
    egui::Modal::new(egui::Id::new(id)).frame(frame)
}

/// A dialog's title, cut short where it is long, with a cross in the
/// corner, level with it, that closes the dialog: true once clicked.
pub(crate) fn dialog_title(ui: &mut egui::Ui, title: impl Into<egui::WidgetText>) -> bool {
    let title = title.into();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let side = ui.spacing().interact_size.y;
            let cross = egui::Button::new("✖").frame(false).min_size(vec2(side, side));
            let closed = ui.add(cross).on_hover_text(fl!("close")).clicked();
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
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add).inner
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
        let height = ui.spacing().interact_size.y;
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
                    let mut button = egui::Button::selectable(selected, text.as_str())
                        .truncate()
                        .corner_radius(corners)
                        .min_size(vec2(0.0, height));
                    button = if selected {
                        button.stroke(theme::on_outline(ui.visuals()))
                    } else {
                        button.frame_when_inactive(true)
                    };
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

/// `button` as a choice of [`segments`] is, on its own: lit, and outlined,
/// where `selected`.
pub(crate) fn as_choice<'a>(
    ui: &egui::Ui,
    button: egui::Button<'a>,
    selected: bool,
) -> egui::Button<'a> {
    let height = ui.spacing().interact_size.y;
    let button = button.selected(selected).frame_when_inactive(true).min_size(vec2(0.0, height));
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
    let height = ui.spacing().interact_size.y;
    let button = egui::Button::selectable(selected, text).min_size(vec2(0.0, height));
    let button = if selected {
        button.stroke(theme::on_outline(ui.visuals()))
    } else {
        button.frame_when_inactive(true)
    };
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

/// A picker of opaque colours: a square of the shades of one hue, more
/// saturated to the right and lighter to the top, and a strip of the hues
/// under it. `names` are what screen readers call the two. True when
/// `colour` changed.
pub(crate) fn colour_picker(ui: &mut egui::Ui, colour: &mut Color32, names: [&str; 2]) -> bool {
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
    let area = |ui: &mut egui::Ui, height: f32, name: &str| {
        let sense = egui::Sense::click_and_drag();
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, enabled, name));
        let at = response.interact_pointer_pos().map(|at| {
            let at = (at - rect.min) / rect.size();
            vec2(at.x.clamp(0.0, 1.0), at.y.clamp(0.0, 1.0))
        });
        (rect, at)
    };
    let (square, picked) = area(ui, 150.0, names[0]);
    if let Some(at) = picked {
        (hsv.s, hsv.v) = (at.x, 1.0 - at.y);
    }
    let (strip, picked) = area(ui, 16.0, names[1]);
    if let Some(at) = picked {
        hsv.h = at.x;
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
    for rect in [square, strip] {
        painter.rect_stroke(rect, 0.0, outline, egui::StrokeKind::Outside);
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
    ui.add(egui::Button::new(glyph).small().frame(false).min_size(size))
        .on_hover_text(hint)
        .clicked()
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
    edit: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>,
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
        let mut field = edit(egui::DragValue::new(&mut number)).clamp_existing_to_range(live);
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
pub(crate) fn number_field(
    ui: &mut egui::Ui,
    set: bool,
    field: egui::DragValue<'_>,
) -> egui::Response {
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
    edit: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>,
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
    bar_sized(ui, egui::vec2(width, 22.0), fill, text, colour);
}

/// A [`bar`] of `size`.
pub(crate) fn bar_sized(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    fill: Fill,
    text: &str,
    colour: Color32,
) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rounding = (size.y / 2.0).min(4.0);
    let painter = ui.painter_at(rect);
    let (track, outline) = theme::track(ui.visuals());
    painter.rect_filled(rect, rounding, track);
    // the whole bar's shape, cut to how far it got: no blob at its start
    // when it is nearly empty
    if let Fill::Share(share) | Fill::Working(share) = fill {
        let width = rect.width() * share.clamp(0.0, 1.0);
        let part = egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height()));
        ui.painter_at(part).rect_filled(rect, rounding, colour);
    }
    if !matches!(fill, Fill::Share(_)) {
        let band = rect.width() * 0.3;
        let time = ui.input(|input| input.time) as f32;
        let left = rect.left() - band + (time * 0.7).fract() * (rect.width() + band);
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
    // cut short where the bar is narrower, said in full on hover
    let colour = ui.visuals().strong_text_color();
    let font = egui::FontId::proportional(12.5);
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, colour);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 16.0);
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

/// Draws a checkerboard, so transparency shows.
pub(crate) fn checkerboard(painter: &egui::Painter, rect: egui::Rect, square: f32) {
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
    // only the squares in view
    let first_column = ((clip.left() - rect.left()) / square).floor().max(0.0) as i32;
    let first_row = ((clip.top() - rect.top()) / square).floor().max(0.0) as i32;
    let columns = ((clip.right() - rect.left()) / square).ceil() as i32;
    let rows = ((clip.bottom() - rect.top()) / square).ceil() as i32;
    for row in first_row..rows {
        let start = first_column + (row + first_column) % 2;
        for column in (start..columns).step_by(2) {
            let min = rect.min + egui::vec2(column as f32 * square, row as f32 * square);
            let cell = egui::Rect::from_min_size(min, egui::vec2(square, square)).intersect(rect);
            painter.rect_filled(cell, 0.0, dark);
        }
    }
}

/// Gives a test's bare `harness` the window's fonts. egui's own are left
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
}

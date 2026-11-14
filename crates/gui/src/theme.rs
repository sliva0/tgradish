//! The window's look: KDE's Breeze Light and Breeze Dark colour schemes and
//! Breeze's sizes, as far as egui's style has them, with the system's
//! accent where it has one, or one the user chose. Every colour that
//! follows the accent is made from the one given to [`apply`], so changing
//! it changes them all; any accent leaves text readable and outlines seen,
//! however light, dark or grey it is.

use eframe::egui::{self, Color32, CornerRadius, Stroke, vec2};

/// Breeze's accent, for systems without one.
pub(crate) const BREEZE_ACCENT: Color32 = Color32::from_rgb(61, 174, 233);

/// The accents Plasma's colour settings offer, in their order, by the
/// names `accent` in `[gui]` takes them by; Breeze's is the blue.
pub(crate) const ACCENTS: [(&str, Color32); 10] = [
    ("pink", Color32::from_rgb(233, 58, 154)),
    ("red", Color32::from_rgb(233, 61, 88)),
    ("orange", Color32::from_rgb(233, 100, 58)),
    ("yellow", Color32::from_rgb(232, 203, 45)),
    ("green", Color32::from_rgb(61, 212, 37)),
    ("teal", Color32::from_rgb(0, 211, 184)),
    ("blue", BREEZE_ACCENT),
    ("purple", Color32::from_rgb(184, 117, 220)),
    ("violet", Color32::from_rgb(146, 110, 228)),
    ("grey", Color32::from_rgb(104, 107, 111)),
];

/// What a colour scheme says, by Breeze's names for it.
struct Scheme {
    /// Window: panels and dialogs.
    window: Color32,
    window_alternate: Color32,
    /// View: text fields, lists and the like.
    view: Color32,
    button: Color32,
    /// Header: the stage pictures are shown on.
    header: Color32,
    text: Color32,
    /// Foreground inactive: weaker text.
    inactive: Color32,
    link: Color32,
    /// Text that says how things went: well, with a warning, and badly.
    positive: Color32,
    neutral: Color32,
    negative: Color32,
}

/// Breeze Light, with its weaker, link and status text darker, as Breeze's
/// own are too light for small text on its window colour: each reads at
/// 4.5:1 or more on the window, the stage pictures are shown on, and a
/// list row lit as selected.
const LIGHT: Scheme = Scheme {
    window: Color32::from_rgb(239, 240, 241),
    window_alternate: Color32::from_rgb(227, 229, 231),
    view: Color32::from_rgb(255, 255, 255),
    button: Color32::from_rgb(252, 252, 252),
    header: Color32::from_rgb(222, 224, 226),
    text: Color32::from_rgb(35, 38, 41),
    inactive: Color32::from_rgb(86, 97, 108),
    link: Color32::from_rgb(31, 108, 160),
    positive: Color32::from_rgb(0, 115, 57),
    neutral: Color32::from_rgb(160, 72, 0),
    negative: Color32::from_rgb(184, 36, 54),
};

/// Breeze Dark, its status text a little lighter, for the same reason.
const DARK: Scheme = Scheme {
    window: Color32::from_rgb(32, 35, 38),
    window_alternate: Color32::from_rgb(41, 44, 48),
    view: Color32::from_rgb(20, 22, 24),
    button: Color32::from_rgb(41, 44, 48),
    header: Color32::from_rgb(20, 22, 24),
    text: Color32::from_rgb(252, 252, 252),
    inactive: Color32::from_rgb(161, 169, 177),
    link: Color32::from_rgb(29, 153, 243),
    positive: Color32::from_rgb(61, 200, 120),
    neutral: Color32::from_rgb(250, 140, 40),
    negative: Color32::from_rgb(245, 120, 130),
};

/// Fills that say how things went, as gauges and badges: Breeze's own,
/// the same in both schemes. Text takes [`good`], [`warn`] and [`bad`].
pub(crate) const POSITIVE: Color32 = Color32::from_rgb(39, 174, 96);
pub(crate) const NEUTRAL: Color32 = Color32::from_rgb(246, 116, 0);
pub(crate) const NEGATIVE: Color32 = Color32::from_rgb(218, 68, 83);

/// How high buttons and fields are, as Breeze's.
pub(crate) const CONTROL: f32 = 28.0;

/// Breeze's frame radius, for buttons, fields and dialogs.
const RADIUS: u8 = 5;

/// How much of the accent Breeze lets through where it marks something,
/// as opacity or as a mix (its `Blend_Value`).
const BLEND: f32 = 0.3;

/// How much of the accent a list row lets through, selected and hovered:
/// less than [`BLEND`], so the small coloured text on a row stays
/// readable; the selected row is outlined in the accent as well.
const ROW_SELECTED: f32 = 0.15;
const ROW_HOVERED: f32 = 0.07;

/// The least a chosen segment or choice stands apart from a resting
/// button, as a step of lightness (CIE L*, 0 to 100): about the step
/// between Breeze's window and its buttons' outlines, which reads at a
/// glance, where a tint of a grey or a dark accent did not.
const CHOSEN_STEP: f32 = 10.0;

/// The least the selected list row stands apart from the window, in L*: as
/// much as the row's small coloured text leaves room for on Breeze Light,
/// which takes about 6; its outline in the accent marks it too.
const ROW_STEP: f32 = 5.0;

/// The most of the accent a chosen segment lets through, and a list row,
/// where less doesn't stand apart enough.
const MOST_CHOSEN: f32 = 0.5;
const MOST_ROW: f32 = 0.3;

/// The contrast of a primary button's words with its fill: more than
/// small text needs, so the shortcut beside them can be a weaker shade
/// that still reads.
const PRIMARY_WORDS: f32 = 7.0;

/// How far a hovered and a pressed primary button's fill moves from the
/// resting one, in L*.
const PRIMARY_HOVERED: f32 = 3.0;
const PRIMARY_PRESSED: f32 = 6.0;

/// The most a primary button's shortcut is mixed into its fill, as weaker
/// text, as long as it still reads.
const PRIMARY_HINT: f32 = 0.45;

/// How far a pressed button's fill moves towards the text colour: a
/// change of shade, not of colour, so that a button with the keyboard's
/// focus, which egui draws as pressed, doesn't look chosen.
const PRESSED: f32 = 0.08;

/// The contrast small text needs against what is under it.
const READABLE: f32 = 4.5;

/// The contrast an outline or a mark needs against the window to be seen,
/// as WCAG asks of controls and of the keyboard's focus.
const SEEN: f32 = 3.0;

/// The contrast against the window of the colour a fill lets some of
/// through, so that a little of it still shows: Breeze's blue has it on
/// Breeze Light, an accent as light as that window wouldn't.
const SHOWS: f32 = 2.0;

/// `from` moved `amount` of the way to `to`.
pub(crate) fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// `accent` at `opacity`, over whatever is under it, as Breeze lays it on
/// what it highlights.
pub(crate) fn tint(accent: Color32, opacity: f32) -> Color32 {
    let alpha = (opacity * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha)
}

/// The scheme `visuals` were made from.
fn scheme(visuals: &egui::Visuals) -> &'static Scheme {
    if visuals.dark_mode { &DARK } else { &LIGHT }
}

/// The accent of `visuals`, as it was given: what fills are tinted with.
/// It is kept as the fill of an open widget's box, which egui draws for
/// checkboxes and sliders, which are never open.
pub(crate) fn accent(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.open.bg_fill
}

/// The accent as an outline or a mark on the window, as hovered widgets
/// and the keyboard's focus are outlined: darkened or lightened towards
/// the text colour as far as it takes to be seen, as a light accent is too
/// faint on Breeze Light and a dark one on Breeze Dark.
pub(crate) fn mark(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.hovered.bg_stroke.color
}

/// The accent as small text or a glyph on the window: darkened or
/// lightened towards the text colour as far as it takes to read.
pub(crate) fn accent_text(visuals: &egui::Visuals) -> Color32 {
    let scheme = scheme(visuals);
    towards(accent(visuals), scheme.text, &[scheme.window], READABLE)
}

/// `colour` moved towards `text` until it stands out of each of
/// `backgrounds` at `ratio`.
fn towards(colour: Color32, text: Color32, backgrounds: &[Color32], ratio: f32) -> Color32 {
    (0..=20)
        .map(|step| mix(colour, text, step as f32 / 20.0))
        .find(|shade| backgrounds.iter().all(|background| contrast(*shade, *background) >= ratio))
        .unwrap_or(text)
}

/// `wanted`, or as much less of `accent` mixed into each of `backgrounds`
/// as it takes for each of `texts` to read on it.
fn bounded(accent: Color32, wanted: f32, texts: &[Color32], backgrounds: &[Color32]) -> f32 {
    let reads = |amount: f32| {
        // as a tint of it is blended
        backgrounds.iter().all(|background| {
            let under = over(*background, tint(accent, amount));
            texts.iter().all(|text| contrast(*text, under) >= READABLE)
        })
    };
    let steps = (wanted * 100.0).round() as i32;
    (0..=steps).rev().map(|step| step as f32 / 100.0).find(|amount| reads(*amount)).unwrap_or(0.0)
}

/// The accent fills let some of through: as it was given, or darkened or
/// lightened towards the text colour where it is too like the window for
/// a little of it to show.
fn tinted(visuals: &egui::Visuals) -> Color32 {
    let scheme = scheme(visuals);
    towards(accent(visuals), scheme.text, &[scheme.window], SHOWS)
}

/// What the scheme's text is drawn on: the window, striped rows, fields
/// and buttons, and the stage pictures are shown on.
fn grounds(scheme: &Scheme) -> [Color32; 5] {
    [scheme.window, scheme.window_alternate, scheme.view, scheme.button, scheme.header]
}

/// The accent at `opacity`, or less, as far as it takes for the text over
/// it to read, over anything the scheme draws text on.
pub(crate) fn fill(visuals: &egui::Visuals, opacity: f32) -> Color32 {
    let (accent, scheme) = (tinted(visuals), scheme(visuals));
    tint(accent, bounded(accent, opacity, &[scheme.text], &grounds(scheme)))
}

/// What a list row lays over the window, `selected` or hovered: little
/// enough of the accent that the row's small coloured text still reads,
/// and the selected one [`ROW_STEP`] apart from the window at the least.
pub(crate) fn row(visuals: &egui::Visuals, selected: bool) -> Color32 {
    let (accent, scheme) = (tinted(visuals), scheme(visuals));
    let texts = small_texts(scheme);
    if selected {
        let amounts = (ROW_SELECTED, MOST_ROW);
        return lit(visuals, scheme.window, ROW_STEP, amounts, &texts, &[scheme.window]);
    }
    tint(accent, bounded(accent, ROW_HOVERED, &texts, &[scheme.window]))
}

/// The scheme's small coloured text, as a list row has: weaker and status
/// text.
fn small_texts(scheme: &Scheme) -> [Color32; 4] {
    [scheme.inactive, scheme.positive, scheme.neutral, scheme.negative]
}

/// How light a colour looks, as CIE L* measures it: 0 for black to 100
/// for white, in steps that look alike.
pub(crate) fn lightness(colour: Color32) -> f32 {
    let y = luminance(colour);
    if y > 216.0 / 24389.0 { 116.0 * y.cbrt() - 16.0 } else { y * 24389.0 / 27.0 }
}

/// The relative luminance of a colour, as WCAG has it: 0 to 1.
fn luminance(colour: Color32) -> f32 {
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
}

/// `tint` laid over `background`, as the window draws it.
fn over(background: Color32, tint: Color32) -> Color32 {
    // premultiplied, as it is blended
    let left = 1.0 - f32::from(tint.a()) / 255.0;
    let channel = |under: u8, over: u8| (f32::from(under) * left + f32::from(over)).round() as u8;
    Color32::from_rgb(
        channel(background.r(), tint.r()),
        channel(background.g(), tint.g()),
        channel(background.b(), tint.b()),
    )
}

/// The tint of the accent that marks something chosen over the window,
/// standing `step` apart in lightness from `resting`, what isn't chosen
/// looks like: `wanted` of the accent, or more, up to `most`, as long as
/// each of `texts` reads over each of `backgrounds`. Where the accent can't
/// stand apart so, as a grey, black or one like the window can't, it is
/// moved towards the text colour, which is away from what rests on either
/// scheme: lighter on Breeze Dark, darker on Breeze Light, its hue kept.
fn lit(
    visuals: &egui::Visuals,
    resting: Color32,
    step: f32,
    (wanted, most): (f32, f32),
    texts: &[Color32],
    backgrounds: &[Color32],
) -> Color32 {
    let (accent, scheme) = (tinted(visuals), scheme(visuals));
    let mut best = (0.0, tint(accent, bounded(accent, wanted, texts, backgrounds)));
    for shade in 0..=20 {
        let colour = mix(accent, scheme.text, shade as f32 / 20.0);
        let top = bounded(colour, most, texts, backgrounds);
        let first = (wanted.min(top) * 100.0).round() as i32;
        for amount in first..=(top * 100.0).round() as i32 {
            let chosen = tint(colour, amount as f32 / 100.0);
            let apart = (lightness(over(scheme.window, chosen)) - lightness(resting)).abs();
            if apart >= step {
                return chosen;
            }
            if apart > best.0 {
                best = (apart, chosen);
            }
        }
    }
    best.1
}

/// The contrast of two colours, as WCAG measures it: 1 to 21.
pub(crate) fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Text saying something went well.
pub(crate) fn good(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).positive
}

/// Text warning of something.
pub(crate) fn warn(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).neutral
}

/// Text saying something went wrong.
pub(crate) fn bad(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).negative
}

/// The outline of what is on: half the accent, as Breeze outlines a
/// dialog's default button, or more, as it takes to be seen on the window.
pub(crate) fn on_outline(visuals: &egui::Visuals) -> Stroke {
    let scheme = scheme(visuals);
    let half = mix(mark(visuals), visuals.widgets.inactive.bg_stroke.color, 0.5);
    Stroke::new(1.0, towards(half, scheme.text, &[scheme.window, scheme.window_alternate], SEEN))
}

/// The background of stages pictures are shown on.
pub(crate) fn stage(dark: bool) -> Color32 {
    if dark { DARK.header } else { LIGHT.header }
}

/// The empty part of a gauge, and its outline: in the dark scheme a faint
/// shade of the window rather than the view's near black, which reads as a
/// slab; in the light one the view's white, which needs no outline.
pub(crate) fn track(visuals: &egui::Visuals) -> (Color32, Stroke) {
    let scheme = scheme(visuals);
    if visuals.dark_mode {
        (mix(scheme.window, scheme.text, 0.05), visuals.widgets.noninteractive.bg_stroke)
    } else {
        (scheme.view, Stroke::NONE)
    }
}

/// `visuals` for a primary button, the one way on in its place: filled
/// with the accent, its words black or white, whichever reads better on
/// it, and the accent made as much darker or lighter as it takes for them
/// to read at [`PRIMARY_WORDS`], and to stand apart from other buttons by
/// [`CHOSEN_STEP`]. Hovered and pressed, the fill moves further from the
/// words. The shortcut beside the words, weaker text, is a shade between
/// them and the fill that still reads on each; it is outlined in the
/// accent, as it is seen on the window.
pub(crate) fn primary(visuals: &mut egui::Visuals) {
    let scheme = scheme(visuals);
    let (fills, words, hint) = primary_colours(accent(visuals), scheme.button);
    let outline = Stroke::new(1.0, mark(visuals));
    let widgets = &mut visuals.widgets;
    for (state, fill) in
        [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active].into_iter().zip(fills)
    {
        state.weak_bg_fill = fill;
        state.bg_stroke = outline;
        state.fg_stroke.color = words;
    }
    visuals.weak_text_color = Some(hint);
}

/// `visuals` for a primary button that can't be pressed: a button as any
/// other, its words and shortcut in the weaker text colour, which reads on
/// it, rather than faded as egui fades what is greyed out.
pub(crate) fn primary_disabled(visuals: &mut egui::Visuals) {
    let weak = scheme(visuals).inactive;
    visuals.widgets.noninteractive.fg_stroke.color = weak;
    visuals.widgets.inactive.fg_stroke.color = weak;
    visuals.weak_text_color = Some(weak);
    visuals.disabled_alpha = 1.0;
}

/// A primary button's fills, resting, hovered and pressed, its words and
/// its shortcut's colour, for `accent` beside buttons of the `resting`
/// fill.
fn primary_colours(accent: Color32, resting: Color32) -> ([Color32; 3], Color32, Color32) {
    let (dark, light) = (LIGHT.text, DARK.text);
    let words = if contrast(dark, accent) >= contrast(light, accent) { dark } else { light };
    // away from the words: lighter under dark ones, darker under light ones
    let away = if words == dark { Color32::WHITE } else { Color32::BLACK };
    let steps =
        |from: Color32, to: Color32| (0..=40).map(move |step| mix(from, to, step as f32 / 40.0));
    let mut fill =
        steps(accent, away).find(|fill| contrast(words, *fill) >= PRIMARY_WORDS).unwrap_or(away);
    // apart from other buttons, the way there is room
    if (lightness(fill) - lightness(resting)).abs() < CHOSEN_STEP {
        let apart = |fill: &Color32| {
            (lightness(*fill) - lightness(resting)).abs() >= CHOSEN_STEP
                && contrast(words, *fill) >= PRIMARY_WORDS
        };
        let darker = steps(fill, Color32::BLACK).find(apart);
        let lighter = steps(fill, Color32::WHITE).find(apart);
        fill = darker.or(lighter).unwrap_or(fill);
    }
    // hovered and pressed, further from the words; where the fill can't
    // go further, or would come nearer other buttons, towards them, as
    // long as they read as well
    let state = |step: f32| {
        [away, words]
            .into_iter()
            .flat_map(|to| (1..=40).map(move |amount| mix(fill, to, amount as f32 / 40.0)))
            .find(|shade| {
                (lightness(*shade) - lightness(fill)).abs() >= step
                    && (lightness(*shade) - lightness(resting)).abs() >= CHOSEN_STEP
                    && contrast(words, *shade) >= PRIMARY_WORDS
            })
            .unwrap_or(fill)
    };
    let fills = [fill, state(PRIMARY_HOVERED), state(PRIMARY_PRESSED)];
    let hint = (0..=(PRIMARY_HINT * 100.0) as i32)
        .rev()
        .map(|step| step as f32 / 100.0)
        .map(|amount| mix(words, fill, amount))
        .find(|hint| fills.iter().all(|fill| contrast(*hint, *fill) >= READABLE))
        .unwrap_or(words);
    (fills, words, hint)
}

/// The colours of a theme, `accent` for selection, focus and hover.
pub(crate) fn visuals(dark: bool, accent: Color32) -> egui::Visuals {
    let scheme = if dark { &DARK } else { &LIGHT };
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    // Breeze draws frames in a mix of the background and the text
    let separator = mix(scheme.window, scheme.text, 0.2);
    let outline = mix(scheme.button, scheme.text, 0.25);
    let radius = CornerRadius::same(RADIUS);
    let state = |fill: Color32, weak_fill: Color32, stroke: Stroke| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: weak_fill,
        bg_stroke: stroke,
        fg_stroke: Stroke::new(1.0, scheme.text),
        corner_radius: radius,
        expansion: 0.0,
    };
    // hovering outlines in the accent, as it can be seen on the window;
    // pressing shades the button
    let hover = Stroke::new(
        1.0,
        towards(accent, scheme.text, &[scheme.window, scheme.window_alternate], SEEN),
    );
    let pressed = mix(scheme.button, scheme.text, PRESSED);
    visuals.widgets = egui::style::Widgets {
        noninteractive: state(scheme.window, scheme.window, Stroke::new(1.0, separator)),
        // fill for checkboxes and the like, weak fill for buttons
        inactive: state(scheme.view, scheme.button, Stroke::new(1.0, outline)),
        hovered: state(scheme.view, scheme.button, hover),
        active: state(scheme.view, pressed, hover),
        // the accent as it was given, where [`accent`] finds it
        open: state(accent, scheme.button, hover),
    };
    // what is chosen lets 30% of the accent through, as Breeze does, or as
    // much more or lighter as it takes to stand apart from a button
    let amounts = (BLEND, MOST_CHOSEN);
    let chosen =
        lit(&visuals, scheme.button, CHOSEN_STEP, amounts, &[scheme.text], &grounds(scheme));
    visuals.selection =
        egui::style::Selection { bg_fill: chosen, stroke: Stroke::new(1.0, scheme.text) };
    visuals.weak_text_color = Some(scheme.inactive);
    visuals.hyperlink_color = scheme.link;
    visuals.faint_bg_color = scheme.window_alternate;
    visuals.extreme_bg_color = scheme.view;
    visuals.text_edit_bg_color = Some(scheme.view);
    visuals.code_bg_color = scheme.view;
    visuals.warn_fg_color = scheme.neutral;
    visuals.error_fg_color = scheme.negative;
    visuals.window_fill = scheme.window;
    visuals.panel_fill = scheme.window;
    visuals.window_stroke = Stroke::new(1.0, separator);
    visuals.window_corner_radius = radius;
    visuals.menu_corner_radius = radius;
    visuals.text_cursor.stroke = Stroke::new(1.5, scheme.text);
    visuals.handle_shape = egui::style::HandleShape::Circle;
    visuals.slider_trailing_fill = true;
    visuals
}

/// Gives both themes Breeze's look, with `accent`, or Breeze's where the
/// system has none.
pub(crate) fn apply(ctx: &egui::Context, accent: Option<Color32>) {
    let accent = accent.unwrap_or(BREEZE_ACCENT);
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals(dark, accent);
            let spacing = &mut style.spacing;
            // Breeze's layout spacing and margins, and its buttons' padding
            spacing.item_spacing = vec2(6.0, 6.0);
            spacing.button_padding = vec2(8.0, 4.0);
            spacing.interact_size.y = CONTROL;
            spacing.window_margin = egui::Margin::same(10);
            spacing.menu_margin = egui::Margin::same(4);
            // checkboxes and radio buttons, and slider grooves
            spacing.icon_width = 16.0;
            spacing.icon_width_inner = 8.0;
            spacing.slider_rail_height = 6.0;
        });
    }
}

/// Rings the widget with the keyboard's focus in the accent, as egui marks
/// it only by drawing it pressed. Text fields show their focus themselves,
/// with a cursor and an outline of their own.
pub(crate) fn focus_ring(ctx: &egui::Context) {
    let Some(id) = ctx.memory(|memory| memory.focused()) else { return };
    if ctx.text_edit_focused() {
        return;
    }
    let Some(response) = ctx.read_response(id) else { return };
    if !response.interact_rect.is_positive() {
        return;
    }
    let visuals = ctx.global_style().visuals.clone();
    let ring = Stroke::new(2.0, mark(&visuals));
    // cut to what is shown of it, as in a list scrolled part of the way
    let painter = ctx.layer_painter(response.layer_id).with_clip_rect(response.interact_rect);
    let radius = visuals.widgets.inactive.corner_radius;
    painter.rect_stroke(response.rect, radius, ring, egui::StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_on_the_accent_like_breeze() {
        let orange = Color32::from_rgb(246, 116, 0);
        for (dark, scheme) in [(false, &LIGHT), (true, &DARK)] {
            let visuals = visuals(dark, orange);
            assert_eq!(accent(&visuals), orange);
            // hovered and pressed buttons only outline in it, so that one
            // with the focus, drawn pressed, doesn't look chosen
            assert_eq!(visuals.widgets.hovered.weak_bg_fill, scheme.button);
            assert_eq!(visuals.widgets.active.bg_stroke.color, mark(&visuals));
            assert_ne!(visuals.widgets.active.weak_bg_fill, mix(scheme.button, orange, BLEND));
        }
        // selections let 30% of it through, over what is under them, where
        // that stands apart from a button, as on Breeze Light
        let light = visuals(false, orange);
        assert_eq!(light.selection.bg_fill, Color32::from_rgba_unmultiplied(246, 116, 0, 77));
        // outlined in it as it is where it is seen, darker where it isn't
        assert_eq!(mark(&visuals(true, orange)), orange);
        let light = mark(&light);
        assert!(contrast(light, LIGHT.window) >= SEEN && contrast(orange, LIGHT.window) < SEEN);
    }

    #[test]
    fn keeps_small_text_readable() {
        for scheme in [&LIGHT, &DARK] {
            // on the window, striped rows, and the stage
            for background in [scheme.window, scheme.window_alternate, scheme.header] {
                for text in small_texts(scheme) {
                    let ratio = contrast(text, background);
                    assert!(ratio >= READABLE, "{text:?} on {background:?}: {ratio}");
                }
            }
            assert!(contrast(scheme.link, scheme.window) >= READABLE);
        }
    }

    #[test]
    fn measures_lightness_as_cie_does() {
        assert!(lightness(Color32::BLACK).abs() < 0.01);
        assert!((lightness(Color32::WHITE) - 100.0).abs() < 0.01);
        // middle grey, 18% of the light, is about half way
        assert!((lightness(Color32::from_gray(119)) - 50.0).abs() < 0.5);
    }

    #[test]
    fn keeps_any_accent_readable_seen_and_apart() {
        // Plasma's, and those no scheme suits: as light or as dark as the
        // window, grey, and a yellow that is light at full strength
        let hostile = [
            Color32::WHITE,
            Color32::BLACK,
            Color32::from_rgb(128, 128, 128),
            Color32::from_rgb(255, 255, 0),
            LIGHT.window,
            DARK.window,
        ];
        let accents = ACCENTS.iter().map(|(_, colour)| *colour).chain(hostile);
        for given in accents {
            for (dark, scheme) in [(false, &LIGHT), (true, &DARK)] {
                let visuals = visuals(dark, given);
                assert_eq!(accent(&visuals), given);
                let reads = |text: Color32, under: Color32, what: &str| {
                    let ratio = contrast(text, under);
                    assert!(ratio >= READABLE, "{what} with {given:?}, dark {dark}: {ratio}");
                };
                let apart = |a: Color32, b: Color32, step: f32, what: &str| {
                    let apart = (lightness(a) - lightness(b)).abs();
                    assert!(apart >= step, "{what} with {given:?}, dark {dark}: {apart}");
                };
                let seen = |colour: Color32, what: &str| {
                    for background in [scheme.window, scheme.window_alternate] {
                        let ratio = contrast(colour, background);
                        assert!(ratio >= SEEN, "{what} of {given:?}, dark {dark}: {ratio}");
                    }
                };
                // a chosen segment, and chosen text, on anything
                for background in grounds(scheme) {
                    reads(scheme.text, over(background, visuals.selection.bg_fill), "selection");
                    reads(scheme.text, over(background, fill(&visuals, 0.5)), "a fill");
                }
                // and what is chosen stands apart from what isn't, and
                // is outlined where it is seen
                let chosen = over(scheme.window, visuals.selection.bg_fill);
                apart(chosen, scheme.button, CHOSEN_STEP, "a chosen segment");
                seen(on_outline(&visuals).color, "the outline of what is chosen");
                // a list row's small coloured text, lit or hovered, and the
                // lit one apart from the window
                for selected in [true, false] {
                    let row = over(scheme.window, row(&visuals, selected));
                    for text in small_texts(scheme) {
                        reads(text, row, "a list row");
                    }
                }
                let lit = over(scheme.window, row(&visuals, true));
                apart(lit, scheme.window, ROW_STEP, "the selected list row");
                // the primary button's words and shortcut, hovered and
                // pressed too, and it apart from other buttons
                let mut primary_visuals = visuals.clone();
                primary(&mut primary_visuals);
                let widgets = &primary_visuals.widgets;
                let hint = primary_visuals.weak_text_color();
                for state in [widgets.inactive, widgets.hovered, widgets.active] {
                    reads(state.fg_stroke.color, state.weak_bg_fill, "the primary button");
                    reads(hint, state.weak_bg_fill, "the primary button's shortcut");
                    apart(state.weak_bg_fill, scheme.button, CHOSEN_STEP, "the primary button");
                }
                // a shortcut weaker than the words, and a press that shows
                assert_ne!(hint, widgets.inactive.fg_stroke.color, "{given:?}, dark {dark}");
                let resting = widgets.inactive.weak_bg_fill;
                apart(widgets.hovered.weak_bg_fill, resting, PRIMARY_HOVERED, "hovering");
                apart(widgets.active.weak_bg_fill, resting, PRIMARY_PRESSED, "pressing");
                seen(widgets.inactive.bg_stroke.color, "the primary button's outline");
                // greyed out, still read, on a button as any other
                let mut disabled = visuals.clone();
                primary_disabled(&mut disabled);
                reads(disabled.weak_text_color(), scheme.button, "a disabled primary button");
                // small text in it, on the window
                reads(accent_text(&visuals), scheme.window, "accent text");
                // outlines and the keyboard's focus, seen on the window
                seen(mark(&visuals), "outline");
            }
        }
    }
}

//! The window's look: KDE's Breeze Light and Breeze Dark colour schemes and
//! Breeze's sizes, as far as egui's style has them, with the system's
//! accent where it has one. Every colour that follows the accent is made
//! from the one given to [`apply`], so changing it changes them all.

use eframe::egui::{self, Color32, CornerRadius, Stroke, vec2};

/// Breeze's accent, for systems without one.
pub(crate) const BREEZE_ACCENT: Color32 = Color32::from_rgb(61, 174, 233);

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

/// Breeze's frame radius, for buttons, fields and dialogs.
const RADIUS: u8 = 5;

/// How much of the accent Breeze lets through where it marks something,
/// as opacity or as a mix (its `Blend_Value`).
const BLEND: f32 = 0.3;

/// How much of the accent a list row lets through, selected and hovered:
/// less than [`BLEND`], so the small coloured text on a row stays
/// readable; the selected row is outlined in the accent as well.
pub(crate) const ROW_SELECTED: f32 = 0.15;
pub(crate) const ROW_HOVERED: f32 = 0.07;

/// How much of the accent a primary button mixes into its fill.
const PRIMARY: f32 = 0.45;

/// How far a pressed button's fill moves towards the text colour: a
/// change of shade, not of colour, so that a button with the keyboard's
/// focus, which egui draws as pressed, doesn't look chosen.
const PRESSED: f32 = 0.08;

/// The contrast small text needs against what is under it.
const READABLE: f32 = 4.5;

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

/// The accent of `visuals`, which hovered widgets are outlined in.
pub(crate) fn accent(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.hovered.bg_stroke.color
}

/// The accent as small text or a glyph on the window: darkened or
/// lightened towards the text colour as far as it takes to read, as a light
/// accent is too faint on Breeze Light.
pub(crate) fn accent_text(visuals: &egui::Visuals) -> Color32 {
    let scheme = scheme(visuals);
    readable(accent(visuals), scheme.text, scheme.window)
}

/// `colour` moved towards `text` until it reads on `background`.
fn readable(colour: Color32, text: Color32, background: Color32) -> Color32 {
    (0..=20)
        .map(|step| mix(colour, text, step as f32 / 20.0))
        .find(|shade| contrast(*shade, background) >= READABLE)
        .unwrap_or(text)
}

/// The contrast of two colours, as WCAG measures it: 1 to 21.
pub(crate) fn contrast(a: Color32, b: Color32) -> f32 {
    let luminance = |colour: Color32| {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
    };
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
/// dialog's default button.
pub(crate) fn on_outline(visuals: &egui::Visuals) -> Stroke {
    Stroke::new(1.0, mix(accent(visuals), visuals.widgets.inactive.bg_stroke.color, 0.5))
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

/// `visuals` for a primary button, the one way on in its place: its fill
/// the button colour mixed with the accent, more when hovered or pressed,
/// and outlined in the accent.
pub(crate) fn primary(visuals: &mut egui::Visuals) {
    let accent = accent(visuals);
    let button = scheme(visuals).button;
    let outline = Stroke::new(1.0, accent);
    let widgets = &mut visuals.widgets;
    for (state, amount) in [
        (&mut widgets.inactive, PRIMARY),
        (&mut widgets.hovered, PRIMARY + 0.1),
        (&mut widgets.active, PRIMARY + 0.2),
    ] {
        state.weak_bg_fill = mix(button, accent, amount);
        state.bg_stroke = outline;
    }
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
    // hovering outlines in the accent; pressing shades the button
    let hover = Stroke::new(1.0, accent);
    let pressed = mix(scheme.button, scheme.text, PRESSED);
    visuals.widgets = egui::style::Widgets {
        noninteractive: state(scheme.window, scheme.window, Stroke::new(1.0, separator)),
        // fill for checkboxes and the like, weak fill for buttons
        inactive: state(scheme.view, scheme.button, Stroke::new(1.0, outline)),
        hovered: state(scheme.view, scheme.button, hover),
        active: state(scheme.view, pressed, hover),
        open: state(scheme.view, scheme.button, hover),
    };
    visuals.selection = egui::style::Selection {
        bg_fill: tint(accent, BLEND),
        stroke: Stroke::new(1.0, scheme.text),
    };
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
            spacing.interact_size.y = 28.0;
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
    let ring = Stroke::new(2.0, accent(&visuals));
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
            // selections let 30% of it through, over what is under them
            assert_eq!(visuals.selection.bg_fill, Color32::from_rgba_unmultiplied(246, 116, 0, 77));
            // hovered and pressed buttons only outline in it, so that one
            // with the focus, drawn pressed, doesn't look chosen
            assert_eq!(visuals.widgets.hovered.weak_bg_fill, scheme.button);
            assert_eq!(visuals.widgets.active.bg_stroke.color, orange);
            assert_ne!(visuals.widgets.active.weak_bg_fill, mix(scheme.button, orange, BLEND));
        }
    }

    #[test]
    fn keeps_small_text_readable() {
        for (dark, scheme) in [(false, &LIGHT), (true, &DARK)] {
            // on the window, striped rows, the stage, and a list row lit as
            // selected, with Breeze's accent
            let row = mix(scheme.window, BREEZE_ACCENT, ROW_SELECTED);
            for background in [scheme.window, scheme.window_alternate, scheme.header, row] {
                for text in [scheme.inactive, scheme.positive, scheme.neutral, scheme.negative] {
                    let ratio = contrast(text, background);
                    assert!(ratio >= READABLE, "{text:?} on {background:?}: {ratio}");
                }
            }
            assert!(contrast(scheme.link, scheme.window) >= READABLE);
            // a light accent darkened till it reads, which a dark one does
            let visuals = visuals(dark, BREEZE_ACCENT);
            assert!(contrast(accent_text(&visuals), scheme.window) >= READABLE);
        }
        let yellow = visuals(false, Color32::from_rgb(253, 188, 75));
        assert!(contrast(accent_text(&yellow), LIGHT.window) >= READABLE);
    }
}

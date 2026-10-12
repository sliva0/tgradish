//! The window's look: KDE's Breeze Light and Breeze Dark colour schemes and
//! Breeze's sizes, as far as egui's style has them, with the system's
//! accent where it has one.

use eframe::egui::{self, Color32, CornerRadius, Stroke, vec2};

/// Breeze's accent, for systems without one.
pub const BREEZE_ACCENT: Color32 = Color32::from_rgb(61, 174, 233);

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
}

const LIGHT: Scheme = Scheme {
    window: Color32::from_rgb(239, 240, 241),
    window_alternate: Color32::from_rgb(227, 229, 231),
    view: Color32::from_rgb(255, 255, 255),
    button: Color32::from_rgb(252, 252, 252),
    header: Color32::from_rgb(222, 224, 226),
    text: Color32::from_rgb(35, 38, 41),
    inactive: Color32::from_rgb(112, 125, 138),
    link: Color32::from_rgb(41, 128, 185),
};

const DARK: Scheme = Scheme {
    window: Color32::from_rgb(32, 35, 38),
    window_alternate: Color32::from_rgb(41, 44, 48),
    view: Color32::from_rgb(20, 22, 24),
    button: Color32::from_rgb(41, 44, 48),
    header: Color32::from_rgb(20, 22, 24),
    text: Color32::from_rgb(252, 252, 252),
    inactive: Color32::from_rgb(161, 169, 177),
    link: Color32::from_rgb(29, 153, 243),
};

/// Text that says how things went, the same in both schemes.
pub const POSITIVE: Color32 = Color32::from_rgb(39, 174, 96);
pub const NEUTRAL: Color32 = Color32::from_rgb(246, 116, 0);
pub const NEGATIVE: Color32 = Color32::from_rgb(218, 68, 83);

/// Breeze's frame radius, for buttons, fields and dialogs.
const RADIUS: u8 = 5;

/// How much of the accent Breeze lets through where it marks something,
/// as opacity or as a mix (its `Blend_Value`).
const BLEND: f32 = 0.3;

/// `from` moved `amount` of the way to `to`.
fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
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
pub fn tint(accent: Color32, opacity: f32) -> Color32 {
    let alpha = (opacity * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha)
}

/// The accent of `visuals`, which hovered widgets are outlined in.
pub fn accent(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.hovered.bg_stroke.color
}

/// The outline of what is on: half the accent, as Breeze outlines a
/// dialog's default button.
pub fn on_outline(visuals: &egui::Visuals) -> Stroke {
    Stroke::new(1.0, mix(accent(visuals), visuals.widgets.inactive.bg_stroke.color, 0.5))
}

/// The background of stages pictures are shown on.
pub fn stage(dark: bool) -> Color32 {
    if dark { DARK.header } else { LIGHT.header }
}

/// The colours of a theme, `accent` for selection, focus and hover.
pub fn visuals(dark: bool, accent: Color32) -> egui::Visuals {
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
    // hovering outlines in the accent; pressing mixes it in
    let hover = Stroke::new(1.0, accent);
    visuals.widgets = egui::style::Widgets {
        noninteractive: state(scheme.window, scheme.window, Stroke::new(1.0, separator)),
        // fill for checkboxes and the like, weak fill for buttons
        inactive: state(scheme.view, scheme.button, Stroke::new(1.0, outline)),
        hovered: state(scheme.view, scheme.button, hover),
        active: state(scheme.view, mix(scheme.button, accent, BLEND), hover),
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
    visuals.warn_fg_color = NEUTRAL;
    visuals.error_fg_color = NEGATIVE;
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
pub fn apply(ctx: &egui::Context, accent: Option<Color32>) {
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
            // pressed buttons mix it in; hovered ones only outline in it
            assert_eq!(visuals.widgets.active.weak_bg_fill, mix(scheme.button, orange, 0.3));
            assert_eq!(visuals.widgets.hovered.weak_bg_fill, scheme.button);
        }
    }
}

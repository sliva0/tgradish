//! Visual theme based on KDE Breeze Light and Breeze Dark palettes and widget
//! dimensions, paired with the system or user-selected accent colour.
//!
//! All accent-derived colours are computed dynamically in [`apply`] so
//! changing the accent updates every control while guaranteeing readable text
//! contrast and visible outlines for any accent lightness or saturation.

use eframe::egui::{self, Color32, CornerRadius, Stroke, vec2};

/// Default Breeze accent colour when the system provides none.
pub(crate) const BREEZE_ACCENT: Color32 = Color32::from_rgb(61, 174, 233);

/// Built-in accent palette matching KDE Plasma's colour settings, in their
/// order, keyed by the names accepted in `[gui] accent` (`blue` is
/// [`BREEZE_ACCENT`]).
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

/// Base colour palette using Breeze role names.
struct Scheme {
    /// Window background for panels and dialogs.
    window: Color32,
    window_alternate: Color32,
    /// View background for text fields, lists, and tracks.
    view: Color32,
    button: Color32,
    /// Header background used for the preview stage.
    header: Color32,
    text: Color32,
    /// Secondary/inactive foreground text.
    inactive: Color32,
    link: Color32,
    /// Semantic status text colours (success, warning, error).
    positive: Color32,
    neutral: Color32,
    negative: Color32,
}

/// Breeze Light palette, with darkened secondary, link, and status text so
/// small text achieves at least 4.5:1 contrast against the window, preview
/// stage, and selected list rows.
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

/// Breeze Dark palette, with slightly lightened status text for 4.5:1 contrast.
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

/// Semantic fill colours for gauges and badges (shared across light and dark
/// themes; for text, use [`good`], [`warn`], and [`bad`]).
pub(crate) const POSITIVE: Color32 = Color32::from_rgb(39, 174, 96);
pub(crate) const NEUTRAL: Color32 = Color32::from_rgb(246, 116, 0);
pub(crate) const NEGATIVE: Color32 = Color32::from_rgb(218, 68, 83);

/// Standard button and input height in points (matching Breeze).
pub(crate) const CONTROL: f32 = 28.0;

/// Breeze corner radius for buttons, inputs, and dialogs.
const RADIUS: u8 = 5;

/// Default Breeze accent blend factor (`Blend_Value`).
const BLEND: f32 = 0.3;

/// Accent opacity for selected and hovered list rows (kept below [`BLEND`] so
/// small status text on the row stays legible; selected rows also get an
/// accent border).
const ROW_SELECTED: f32 = 0.15;
const ROW_HOVERED: f32 = 0.07;

/// Minimum lightness difference (CIE L*, 0..100) between a selected segment
/// and an inactive button (~the step between Breeze's window fill and button
/// border, keeping grey or dark accents clearly distinguishable).
const CHOSEN_STEP: f32 = 10.0;

/// Minimum L* difference between a selected list row and the window background
/// (Breeze Light's small coloured row text leaves room for about 6 before it
/// drops under 4.5:1; the accent outline marks the row too).
const ROW_STEP: f32 = 5.0;

/// Minimum perceptual colour difference ([`difference`], CIE76 ΔE*ab) between
/// selected and unselected states: a selected segment vs an inactive button, a
/// selected list row vs the window, and a primary button vs regular buttons.
/// Hue sets a colourful accent's tint apart as much as lightness does, so
/// Plasma's colours meet this at their L* step alone; a grey, black or
/// window-like accent can only use lightness, which takes about twice the step.
/// 19 is about how far Breeze's blue sets a chosen segment apart, and a primary
/// button over such an accent takes the words and fill that let it differ as
/// much as a coloured one.
const CHOSEN_APART: f32 = 19.0;
const ROW_APART: f32 = 8.0;
const PRIMARY_APART: f32 = 30.0;

/// Maximum accent opacity for a selected segment or list row when needed to
/// meet lightness/colour difference targets.
const MOST_CHOSEN: f32 = 0.5;
const MOST_ROW: f32 = 0.3;

/// Target contrast ratio between a primary button's label and its fill (higher
/// than 4.5:1 so the dimmed keyboard shortcut beside the label also reaches
/// 4.5:1).
const PRIMARY_WORDS: f32 = 7.0;

/// Minimum L* shift for hovered and pressed primary button fills.
const PRIMARY_HOVERED: f32 = 3.0;
const PRIMARY_PRESSED: f32 = 6.0;

/// Maximum blend factor of a primary button's shortcut text into its fill
/// while maintaining [`READABLE`] contrast.
const PRIMARY_HINT: f32 = 0.45;

/// Blend factor towards the text colour for pressed regular buttons (using a
/// neutral shade shift rather than the accent colour so keyboard-focused
/// buttons, which egui renders with active visuals, do not look selected).
const PRESSED: f32 = 0.08;

/// Minimum WCAG contrast ratio for small text.
const READABLE: f32 = 4.5;

/// Minimum WCAG contrast ratio for control borders, marks, and focus rings.
const SEEN: f32 = 3.0;

/// Minimum contrast ratio against the window for an accent used in translucent
/// tinting so the tint remains visible even when the raw accent matches the
/// window colour.
const SHOWS: f32 = 2.0;

/// Linearly interpolates RGB channels from `from` to `to` by `amount`.
pub(crate) fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// Returns `accent` with unmultiplied `opacity` (`0.0..=1.0`).
pub(crate) fn tint(accent: Color32, opacity: f32) -> Color32 {
    let alpha = (opacity * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha)
}

/// Returns the base [`Scheme`] for `visuals`.
fn scheme(visuals: &egui::Visuals) -> &'static Scheme {
    if visuals.dark_mode { &DARK } else { &LIGHT }
}

/// Returns the raw accent colour stored in `visuals` (stashed in
/// `widgets.open.bg_fill`, which checkboxes and sliders never use).
pub(crate) fn accent(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.open.bg_fill
}

/// Returns the accent colour adjusted to at least [`SEEN`] contrast against
/// the window for hover borders, marks, and focus rings.
pub(crate) fn mark(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.hovered.bg_stroke.color
}

/// Returns the accent colour adjusted to at least [`READABLE`] contrast
/// against the window for small text and icons.
pub(crate) fn accent_text(visuals: &egui::Visuals) -> Color32 {
    let scheme = scheme(visuals);
    towards(accent(visuals), scheme.text, &[scheme.window], READABLE)
}

/// Shifts `colour` towards `text` in 5% steps until it reaches `ratio`
/// contrast against all `backgrounds`.
fn towards(colour: Color32, text: Color32, backgrounds: &[Color32], ratio: f32) -> Color32 {
    (0..=20)
        .map(|step| mix(colour, text, step as f32 / 20.0))
        .find(|shade| backgrounds.iter().all(|background| contrast(*shade, *background) >= ratio))
        .unwrap_or(text)
}

/// Returns the largest opacity `<= wanted` at which `accent` composited over
/// each of `backgrounds` preserves [`READABLE`] contrast for all `texts`.
fn bounded(accent: Color32, wanted: f32, texts: &[Color32], backgrounds: &[Color32]) -> f32 {
    let reads = |amount: f32| {
        backgrounds.iter().all(|background| {
            let under = over(*background, tint(accent, amount));
            texts.iter().all(|text| contrast(*text, under) >= READABLE)
        })
    };
    let steps = (wanted * 100.0).round() as i32;
    (0..=steps).rev().map(|step| step as f32 / 100.0).find(|amount| reads(*amount)).unwrap_or(0.0)
}

/// Returns the accent shifted towards the text colour if needed to achieve at
/// least [`SHOWS`] contrast against the window before translucent tinting.
fn tinted(visuals: &egui::Visuals) -> Color32 {
    let scheme = scheme(visuals);
    towards(accent(visuals), scheme.text, &[scheme.window], SHOWS)
}

/// Returns all background surfaces on which regular text is drawn.
fn grounds(scheme: &Scheme) -> [Color32; 5] {
    [scheme.window, scheme.window_alternate, scheme.view, scheme.button, scheme.header]
}

/// Returns a translucent accent fill at `opacity` (reduced or shifted if
/// necessary) that keeps text readable on all surfaces and stands apart from
/// an empty gauge track by [`CHOSEN_STEP`] and [`CHOSEN_APART`].
pub(crate) fn fill(visuals: &egui::Visuals, opacity: f32) -> Color32 {
    let (scheme, (empty, _)) = (scheme(visuals), track(visuals));
    let (apart, amounts) = ((CHOSEN_STEP, CHOSEN_APART), (opacity, opacity));
    lit(visuals, empty, apart, amounts, &[scheme.text], &grounds(scheme))
}

/// Returns the translucent accent fill for a `selected` or hovered list row,
/// preserving [`READABLE`] contrast for all small coloured row labels and
/// separating the selected row from the window by at least [`ROW_STEP`] (and
/// [`ROW_APART`] where contrast permits).
pub(crate) fn row(visuals: &egui::Visuals, selected: bool) -> Color32 {
    let (accent, scheme) = (tinted(visuals), scheme(visuals));
    let texts = small_texts(scheme);
    if selected {
        let (amounts, apart) = ((ROW_SELECTED, MOST_ROW), (ROW_STEP, ROW_APART));
        return lit(visuals, scheme.window, apart, amounts, &texts, &[scheme.window]);
    }
    tint(accent, bounded(accent, ROW_HOVERED, &texts, &[scheme.window]))
}

/// Returns the secondary and semantic text colours used on list rows.
fn small_texts(scheme: &Scheme) -> [Color32; 4] {
    [scheme.inactive, scheme.positive, scheme.neutral, scheme.negative]
}

/// Computes perceptual lightness (CIE L*, `0.0..=100.0`).
pub(crate) fn lightness(colour: Color32) -> f32 {
    let y = luminance(colour);
    if y > 216.0 / 24389.0 { 116.0 * y.cbrt() - 16.0 } else { y * 24389.0 / 27.0 }
}

/// Computes WCAG relative luminance (`0.0..=1.0`).
fn luminance(colour: Color32) -> f32 {
    let [r, g, b] = linear(colour);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Converts sRGB channels to linear light (`0.0..=1.0`).
fn linear(colour: Color32) -> [f32; 3] {
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    [channel(colour.r()), channel(colour.g()), channel(colour.b())]
}

/// Converts an sRGB colour to CIE L*a*b* (D65 illuminant).
fn lab(colour: Color32) -> [f32; 3] {
    let [r, g, b] = linear(colour);
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.950_47;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.088_83;
    let f =
        |t: f32| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
}

/// Computes CIE76 ΔE*ab perceptual colour difference between `a` and `b`.
pub(crate) fn difference(a: Color32, b: Color32) -> f32 {
    let (a, b) = (lab(a), lab(b));
    a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt()
}

/// Alpha-composites premultiplied `tint` over opaque `background`.
fn over(background: Color32, tint: Color32) -> Color32 {
    let left = 1.0 - f32::from(tint.a()) / 255.0;
    let channel = |under: u8, over: u8| (f32::from(under) * left + f32::from(over)).round() as u8;
    Color32::from_rgb(
        channel(background.r(), tint.r()),
        channel(background.g(), tint.g()),
        channel(background.b(), tint.b()),
    )
}

/// Finds a translucent accent tint (with opacity in `wanted..=most`, shifting
/// the accent towards the text colour if needed) that keeps all `texts`
/// readable over all `backgrounds` while separating the composited window fill
/// from `resting` by at least `step` L* and `apart` ΔE*ab. If nothing readable
/// stands that far apart, uses the furthest that does. The accent shifts
/// towards the text colour (lighter on Breeze Dark, darker on Breeze Light),
/// keeping its hue.
fn lit(
    visuals: &egui::Visuals,
    resting: Color32,
    (step, apart): (f32, f32),
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
            let seen = over(scheme.window, chosen);
            let stepped = (lightness(seen) - lightness(resting)).abs() / step;
            let differs = difference(seen, resting) / apart;
            if stepped >= 1.0 && differs >= 1.0 {
                return chosen;
            }
            if stepped.min(differs) > best.0 {
                best = (stepped.min(differs), chosen);
            }
        }
    }
    best.1
}

/// Computes the WCAG contrast ratio (`1.0..=21.0`) between `a` and `b`.
pub(crate) fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Semantic text colour for success.
pub(crate) fn good(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).positive
}

/// Semantic text colour for warnings.
pub(crate) fn warn(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).neutral
}

/// Semantic text colour for errors.
pub(crate) fn bad(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).negative
}

/// Returns the border stroke for active/selected choice buttons (a 50% mix of
/// [`mark`] and the inactive border, like a Breeze dialog's default button
/// outline, shifted towards the text colour if needed to reach [`SEEN`]
/// contrast).
pub(crate) fn on_outline(visuals: &egui::Visuals) -> Stroke {
    let scheme = scheme(visuals);
    let half = mix(mark(visuals), visuals.widgets.inactive.bg_stroke.color, 0.5);
    Stroke::new(1.0, towards(half, scheme.text, &[scheme.window, scheme.window_alternate], SEEN))
}

/// Returns the border stroke for floating overlays such as info notices and
/// timeline readouts, reaching [`SEEN`] contrast against the window and its
/// alternate (striped-row) shade.
pub(crate) fn floating_outline(visuals: &egui::Visuals) -> Stroke {
    let scheme = scheme(visuals);
    let frame = visuals.widgets.noninteractive.bg_stroke.color;
    Stroke::new(1.0, towards(frame, scheme.text, &[scheme.window, scheme.window_alternate], SEEN))
}

/// Drop shadow for floating overlays.
pub(crate) fn floating_shadow(visuals: &egui::Visuals) -> egui::Shadow {
    let alpha = if visuals.dark_mode { 120 } else { 40 };
    egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(alpha) }
}

/// Returns the fill colour for the active span of a timeline bar (at least 50%
/// of [`mark`], increased if necessary so the composited span reaches [`SEEN`]
/// contrast against both the window and the unused track).
pub(crate) fn span(visuals: &egui::Visuals) -> Color32 {
    let (scheme, mark) = (scheme(visuals), mark(visuals));
    let seen = |used: Color32| {
        [scheme.window, visuals.extreme_bg_color].iter().all(|under| contrast(used, *under) >= SEEN)
    };
    // The mark itself is seen on the window, and further from the view.
    (50..=100)
        .map(|amount| tint(mark, amount as f32 / 100.0))
        .find(|span| seen(over(scheme.window, *span)))
        .unwrap_or(mark)
}

/// High-contrast two-tone stroke colours (`core` in the text colour, `casing`
/// in the view background colour) so ticks and outlines remain visible over
/// any accent hue or lightness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cased {
    pub(crate) core: Color32,
    pub(crate) casing: Color32,
}

/// Returns [`Cased`] colours for marks drawn over accent fills.
pub(crate) fn over_fill(visuals: &egui::Visuals) -> Cased {
    let scheme = scheme(visuals);
    Cased { core: scheme.text, casing: scheme.view }
}

/// Returns the outline colour for status badges on file-list rows, ensuring
/// [`SEEN`] contrast over thumbnails and selected/unselected row fills
/// (using secondary grey on Breeze Dark so white text does not outline badges
/// too harshly).
pub(crate) fn badge_edge(visuals: &egui::Visuals) -> Color32 {
    let scheme = scheme(visuals);
    if !visuals.dark_mode {
        return scheme.text;
    }
    let rows = [true, false].map(|selected| over(scheme.window, row(visuals, selected)));
    towards(scheme.inactive, scheme.text, &[rows[0], rows[1], scheme.window], SEEN)
}

/// Maximum blend factor towards the text colour for [`spent`] progress bars.
const SPENT: f32 = 0.15;

/// Returns the neutral fill colour for completed non-selected fitting steps
/// (e.g. oversized encodes), shifting the empty track colour towards the text
/// colour as far as [`SPENT`] while keeping both normal and warning text
/// [`READABLE`].
pub(crate) fn spent(visuals: &egui::Visuals) -> Color32 {
    let (scheme, (track, _)) = (scheme(visuals), track(visuals));
    let reads = |fill: &Color32| {
        [scheme.text, scheme.neutral].iter().all(|text| contrast(*text, *fill) >= READABLE)
    };
    let steps = (SPENT * 100.0).round() as i32;
    let mut shades = (0..=steps).rev().map(|step| mix(track, scheme.text, step as f32 / 100.0));
    shades.find(reads).unwrap_or(track)
}

/// Hatching stripe colour drawn over [`span`] for timeline segments beyond
/// Telegram's duration limit (uses the unused track's view colour so the
/// hatched region visually blends with the unused track regardless of accent).
pub(crate) fn past_limit(visuals: &egui::Visuals) -> Color32 {
    scheme(visuals).view
}

/// Background colour for preview stages.
pub(crate) fn stage(dark: bool) -> Color32 {
    if dark { DARK.header } else { LIGHT.header }
}

/// Returns the background fill and border stroke for an empty gauge or
/// progress bar track (a subtle bordered window tint in dark mode rather than
/// near-black `view`, and an unbordered white `view` in light mode).
pub(crate) fn track(visuals: &egui::Visuals) -> (Color32, Stroke) {
    let scheme = scheme(visuals);
    if visuals.dark_mode {
        (mix(scheme.window, scheme.text, 0.05), visuals.widgets.noninteractive.bg_stroke)
    } else {
        (scheme.view, Stroke::NONE)
    }
}

/// Minimum L* shift for hovered and dragged scroll-bar handles relative to the
/// resting handle.
const HANDLE_HOVERED: f32 = 10.0;
const HANDLE_HELD: f32 = 20.0;

/// Scroll-bar track and handle colours (`[resting, hovered, held]`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScrollBar {
    pub(crate) track: Color32,
    pub(crate) handle: [Color32; 3],
}

/// Computes neutral Breeze scroll-bar colours: a subtle track and a handle with
/// at least [`SEEN`] contrast against both the track and window, brightening or
/// darkening on hover and drag (avoiding egui's default, which paints both
/// track and handle in the same `view` colour).
pub(crate) fn scroll_bar(visuals: &egui::Visuals) -> ScrollBar {
    let scheme = scheme(visuals);
    let track = mix(scheme.window, scheme.text, 0.06);
    let resting = towards(track, scheme.text, &[scheme.window, track], SEEN);
    let further = |step: f32| {
        (1..=40)
            .map(|amount| mix(resting, scheme.text, amount as f32 / 40.0))
            .find(|shade| (lightness(*shade) - lightness(resting)).abs() >= step)
            .unwrap_or(scheme.text)
    };
    ScrollBar { track, handle: [resting, further(HANDLE_HOVERED), further(HANDLE_HELD)] }
}

/// Styles `visuals` for an enabled primary action button: filled with the
/// accent (adjusted so its dark or light label reaches [`PRIMARY_WORDS`]
/// contrast and the fill differs from regular buttons by [`CHOSEN_STEP`] and
/// [`PRIMARY_APART`]) and outlined in the accent. Where no fill under those
/// words stands far enough apart (white on a grey on Breeze Dark), the other
/// label colour is used. Hovered and pressed fills move further from the label;
/// the shortcut is a shade between label and fill that reads on each.
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

/// Styles `visuals` for a disabled primary button: uses a regular button fill
/// with un-faded secondary text (`disabled_alpha = 1.0`) so the label and
/// shortcut maintain [`READABLE`] contrast.
pub(crate) fn primary_disabled(visuals: &mut egui::Visuals) {
    let weak = scheme(visuals).inactive;
    visuals.widgets.noninteractive.fg_stroke.color = weak;
    visuals.widgets.inactive.fg_stroke.color = weak;
    visuals.weak_text_color = Some(weak);
    visuals.disabled_alpha = 1.0;
}

/// Computes `([resting, hovered, pressed], label_colour, shortcut_colour)` for
/// a primary button with `accent` next to regular buttons of `resting` fill.
fn primary_colours(accent: Color32, resting: Color32) -> ([Color32; 3], Color32, Color32) {
    let (dark, light) = (LIGHT.text, DARK.text);
    let better = if contrast(dark, accent) >= contrast(light, accent) { dark } else { light };
    let other = if better == dark { light } else { dark };
    let away = |words: Color32| if words == dark { Color32::WHITE } else { Color32::BLACK };
    let steps =
        |from: Color32, to: Color32| (0..=40).map(move |step| mix(from, to, step as f32 / 40.0));
    let stands = |fill: Color32, words: Color32| {
        (lightness(fill) - lightness(resting)).abs() >= CHOSEN_STEP
            && difference(fill, resting) >= PRIMARY_APART
            && contrast(words, fill) >= PRIMARY_WORDS
    };
    // Shift `accent` until `words` reaches `PRIMARY_WORDS` contrast, then shift
    // darker or lighter if needed to stand apart from `resting`.
    let under = |words: Color32| {
        let reads = |fill: &Color32| contrast(words, *fill) >= PRIMARY_WORDS;
        let fill = steps(accent, away(words)).find(reads).unwrap_or(away(words));
        let apart = |fill: &Color32| stands(*fill, words);
        let darker = || steps(fill, Color32::BLACK).find(apart);
        let lighter = || steps(fill, Color32::WHITE).find(apart);
        (stands(fill, words).then_some(fill).or_else(darker).or_else(lighter), fill)
    };
    let (words, fill) = match (under(better), under(other)) {
        ((Some(fill), _), _) => (better, fill),
        (_, (Some(fill), _)) => (other, fill),
        ((None, fill), _) => (better, fill),
    };
    let away = away(words);
    // Shift hovered and pressed states away from `words` (or towards `words`
    // if already near white/black or moving nearer other buttons) while
    // maintaining `stands`.
    let state = |step: f32| {
        [away, words]
            .into_iter()
            .flat_map(|to| (1..=40).map(move |amount| mix(fill, to, amount as f32 / 40.0)))
            .find(|shade| {
                (lightness(*shade) - lightness(fill)).abs() >= step && stands(*shade, words)
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

/// Builds egui [`Visuals`](egui::Visuals) for the light or `dark` Breeze theme
/// with `accent`.
pub(crate) fn visuals(dark: bool, accent: Color32) -> egui::Visuals {
    let scheme = if dark { &DARK } else { &LIGHT };
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    // Breeze draws frames in a mix of the background and the text.
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
    // Hover outlines in the contrast-adjusted accent; active shades the button neutrally.
    let hover = Stroke::new(
        1.0,
        towards(accent, scheme.text, &[scheme.window, scheme.window_alternate], SEEN),
    );
    let pressed = mix(scheme.button, scheme.text, PRESSED);
    visuals.widgets = egui::style::Widgets {
        noninteractive: state(scheme.window, scheme.window, Stroke::new(1.0, separator)),
        // `bg_fill` is used for checkboxes/sliders; `weak_bg_fill` for buttons.
        inactive: state(scheme.view, scheme.button, Stroke::new(1.0, outline)),
        hovered: state(scheme.view, scheme.button, hover),
        active: state(scheme.view, pressed, hover),
        // Store the raw `accent` in `open.bg_fill` for `accent(visuals)`.
        open: state(accent, scheme.button, hover),
    };
    // What is chosen lets 30% of the accent through, as Breeze does, or as much
    // more or lighter as it takes to stand apart from a button.
    let amounts = (BLEND, MOST_CHOSEN);
    let apart = (CHOSEN_STEP, CHOSEN_APART);
    let chosen = lit(&visuals, scheme.button, apart, amounts, &[scheme.text], &grounds(scheme));
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

/// Applies Breeze visuals and spacing to both Light and Dark egui styles, using
/// `accent` (or [`BREEZE_ACCENT`] when `None`).
pub(crate) fn apply(ctx: &egui::Context, accent: Option<Color32>) {
    let accent = accent.unwrap_or(BREEZE_ACCENT);
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals(dark, accent);
            let spacing = &mut style.spacing;
            // Breeze's layout spacing and margins, and its buttons' padding.
            spacing.item_spacing = vec2(6.0, 6.0);
            spacing.button_padding = vec2(8.0, 4.0);
            spacing.interact_size.y = CONTROL;
            spacing.window_margin = egui::Margin::same(10);
            spacing.menu_margin = egui::Margin::same(4);
            // Checkboxes and radio buttons, and slider grooves.
            spacing.icon_width = 16.0;
            spacing.icon_width_inner = 8.0;
            spacing.slider_rail_height = 6.0;
        });
    }
}

/// Draws a 2 pt accent ring around the keyboard-focused widget, since egui
/// marks focus only by drawing it pressed. Skips text inputs, which draw
/// their own focus border and cursor, and the crop box until arrow keys move
/// it.
pub(crate) fn focus_ring(ctx: &egui::Context) {
    let Some(id) = ctx.memory(|memory| memory.focused()) else { return };
    if ctx.text_edit_focused() {
        return;
    }
    // Only ring the crop box after arrow-key nudging, not on pointer drag.
    if crate::canvas::crop_focused(ctx) && !crate::canvas::crop_ringed(ctx) {
        return;
    }
    let Some(response) = ctx.read_response(id) else { return };
    if !response.interact_rect.is_positive() {
        return;
    }
    let visuals = ctx.global_style().visuals.clone();
    let ring = Stroke::new(2.0, mark(&visuals));
    // Clip to `interact_rect` so partially scrolled list items clip their focus ring.
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
    fn fills_a_spent_step_plainly_and_readably() {
        for dark in [false, true] {
            let visuals = visuals(dark, BREEZE_ACCENT);
            let (scheme, fill, (track, _)) = (scheme(&visuals), spent(&visuals), track(&visuals));
            // its words, and the warning among them
            for text in [scheme.text, scheme.neutral] {
                let ratio = contrast(text, fill);
                assert!(ratio >= READABLE, "{text:?} on {fill:?}, dark {dark}: {ratio}");
            }
            // filled, as a step done with, unlike a bar not yet begun
            let apart = (lightness(fill) - lightness(track)).abs();
            assert!(apart >= 3.0, "{fill:?} on {track:?}, dark {dark}: {apart}");
        }
    }

    #[test]
    fn measures_lightness_as_cie_does() {
        assert!(lightness(Color32::BLACK).abs() < 0.01);
        assert!((lightness(Color32::WHITE) - 100.0).abs() < 0.01);
        // middle grey, 18% of the light, is about half way
        assert!((lightness(Color32::from_gray(119)) - 50.0).abs() < 0.5);
        // greys differ by lightness alone; a colour by its hue as well
        let (grey, red) = (Color32::from_gray(119), Color32::from_rgb(233, 61, 88));
        let lighter = Color32::from_gray(150);
        assert!((difference(grey, lighter) - (lightness(lighter) - 50.0)).abs() < 0.5);
        assert!((difference(Color32::BLACK, Color32::WHITE) - 100.0).abs() < 0.01);
        assert!(difference(grey, red) > 60.0, "{}", difference(grey, red));
    }

    /// Plasma's accents, and those no scheme suits: as light or as dark as
    /// the window, grey, and a yellow that is light at full strength.
    fn every_accent() -> impl Iterator<Item = Color32> {
        let hostile = [
            Color32::WHITE,
            Color32::BLACK,
            Color32::from_rgb(128, 128, 128),
            Color32::from_rgb(255, 255, 0),
            LIGHT.window,
            DARK.window,
        ];
        ACCENTS.iter().map(|(_, colour)| *colour).chain(hostile)
    }

    #[test]
    fn keeps_any_accent_readable_seen_and_apart() {
        for given in every_accent() {
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
                let differs = |a: Color32, b: Color32, least: f32, what: &str| {
                    let apart = difference(a, b);
                    assert!(apart >= least, "{what} with {given:?}, dark {dark}: {apart}");
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
                // a bar's fill apart from its empty part
                let (filled, (empty, _)) =
                    (over(scheme.window, fill(&visuals, 0.5)), track(&visuals));
                apart(filled, empty, CHOSEN_STEP, "a bar's fill");
                differs(filled, empty, CHOSEN_APART, "a bar's fill");
                // and what is chosen stands apart from what isn't, and
                // is outlined where it is seen
                let chosen = over(scheme.window, visuals.selection.bg_fill);
                apart(chosen, scheme.button, CHOSEN_STEP, "a chosen segment");
                // by hue, or where it has none as a grey hasn't, by more
                // lightness, beside a resting button and on the window
                differs(chosen, scheme.button, CHOSEN_APART, "a chosen segment by a button");
                differs(chosen, scheme.window, CHOSEN_STEP, "a chosen segment on the window");
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
                // and by its hue, or more lightness, where its small text
                // leaves room, as Breeze Dark's does
                if dark {
                    differs(lit, scheme.window, ROW_APART, "the selected list row");
                }
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
                    differs(state.weak_bg_fill, scheme.button, PRIMARY_APART, "the primary button");
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

    #[test]
    fn shows_a_scroll_bars_handle_on_its_track_and_the_window() {
        for given in every_accent() {
            for (dark, scheme) in [(false, &LIGHT), (true, &DARK)] {
                let visuals = visuals(dark, given);
                let ScrollBar { track, handle } = scroll_bar(&visuals);
                // resting, hovered and held, each seen on the track under
                // it and on the window at its ends, as an outline is
                for (state, colour) in ["resting", "hovered", "held"].into_iter().zip(handle) {
                    for (under, what) in [(track, "track"), (scheme.window, "window")] {
                        let ratio = contrast(colour, under);
                        assert!(
                            ratio >= SEEN,
                            "{state} handle on the {what} with {given:?}, dark {dark}: {ratio}"
                        );
                    }
                }
                // nearer the text as the pointer is over it, and held
                let from = |shade: Color32| (lightness(shade) - lightness(handle[0])).abs();
                let towards_text = |shade: Color32| {
                    (lightness(scheme.text) - lightness(shade)).abs()
                        < (lightness(scheme.text) - lightness(handle[0])).abs()
                };
                assert!(from(handle[1]) >= HANDLE_HOVERED && towards_text(handle[1]));
                assert!(from(handle[2]) >= HANDLE_HELD && towards_text(handle[2]));
                // the track a faint shade of the window, never as strong as
                // the handle, and unlike egui's, the view's near black
                let faint = (lightness(track) - lightness(scheme.window)).abs();
                assert!((1.5..CHOSEN_STEP).contains(&faint), "track, dark {dark}: {faint}");
                assert_ne!(handle[0], track);
                assert_ne!(track, visuals.extreme_bg_color);
            }
        }
    }

    #[test]
    fn shows_marks_over_any_accent() {
        for given in every_accent() {
            for (dark, scheme) in [(false, &LIGHT), (true, &DARK)] {
                let visuals = visuals(dark, given);
                let seen = |a: Color32, b: Color32, what: &str| {
                    let ratio = contrast(a, b);
                    assert!(ratio >= SEEN, "{what} with {given:?}, dark {dark}: {ratio}");
                };
                // the timeline's used part, over the window, and its
                // unused part, the track
                let used = over(scheme.window, span(&visuals));
                let track = visuals.extreme_bg_color;
                // the used part seen beside the window and the unused part,
                // not only its ends' ticks
                seen(used, scheme.window, "the used part on the window");
                seen(used, track, "the used part beside the unused part");
                let Cased { core, casing } = over_fill(&visuals);
                // a tick's core, as the playhead and the ends of the used
                // part, seen on either, and on the stripes past the limit
                seen(core, used, "a tick on the used part");
                seen(core, track, "a tick on the track");
                seen(core, past_limit(&visuals), "a tick on the part past the limit");
                // the part past the limit is half striped in the track's
                // colour: the stripes twice the step that sets a chosen
                // segment apart, so even blurred to their average, as fine
                // stripes are seen from afar, it stands as far apart from
                // the part within it
                let apart = (lightness(past_limit(&visuals)) - lightness(used)).abs();
                assert!(apart >= 2.0 * CHOSEN_STEP, "stripes with {given:?}, dark {dark}: {apart}");
                // an outline over the accent at its strongest, as the frame
                // a click picks over the frame picked: its casing seen on
                // it, its core on the track
                seen(casing, mark(&visuals), "the casing of a mark on the accent");
                // the outline of a badge on a list row, on the row lit as
                // selected or hovered, or not at all
                for selected in [true, false] {
                    let row = over(scheme.window, row(&visuals, selected));
                    seen(badge_edge(&visuals), row, "a badge's outline on a list row");
                }
                seen(badge_edge(&visuals), scheme.window, "a badge's outline on the list");
                // the limit's label, beside the bar, on the window
                for label in [scheme.inactive, scheme.neutral] {
                    let ratio = contrast(label, scheme.window);
                    assert!(ratio >= READABLE, "the limit's label, dark {dark}: {ratio}");
                }
            }
        }
    }
}

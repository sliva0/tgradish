//! Built-in fonts and fallback-aware text layout. Characters missing from both
//! the bundled fonts and system fallback fonts are rendered as `U+XXXX` code
//! tags. System font discovery lives in [`crate::fallback`].

use std::hash::{Hash, Hasher};

use eframe::egui::{self, Color32};

use crate::app::App;
use crate::fallback;
use crate::l10n::fl;

/// The family of headings and of what stands out: Ubuntu Medium.
pub(crate) fn medium() -> egui::FontFamily {
    egui::FontFamily::Name("medium".into())
}

/// [`medium`] where `ctx` has the GUI's fonts, else the body's: tests
/// draw parts of the GUI without them.
pub(crate) fn medium_in(ctx: &egui::Context) -> egui::FontFamily {
    let medium = medium();
    match ctx.fonts(|fonts| fonts.definitions().families.contains_key(&medium)) {
        true => medium,
        false => egui::FontFamily::Proportional,
    }
}

/// The GUI's fonts: Ubuntu Light for text, Ubuntu Medium for headings,
/// JetBrains Mono NL for commands, and a few symbols; small subsets, made
/// by `assets/fonts/subset.py`. Ubuntu's are named "Ubuntu Light derivative tgradish"
/// and "Ubuntu Medium derivative tgradish" within, as its licence asks of
/// a version changed but not substantially.
pub(crate) fn fonts() -> egui::FontDefinitions {
    use egui::{FontData, FontFamily};
    let mut fonts = egui::FontDefinitions::empty();
    for (name, bytes) in [
        ("ubuntu", &include_bytes!("../assets/fonts/Ubuntu-Light-derivative-tgradish.ttf")[..]),
        ("medium", &include_bytes!("../assets/fonts/Ubuntu-Medium-derivative-tgradish.ttf")[..]),
        ("mono", &include_bytes!("../assets/fonts/JetBrainsMonoNL-Regular-tgradish.ttf")[..]),
        ("icons", &include_bytes!("../assets/fonts/emoji-icon-font-tgradish.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), std::sync::Arc::new(FontData::from_static(bytes)));
    }
    // each falls back on the others for what it lacks
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["ubuntu".into(), "icons".into(), "mono".into()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["mono".into(), "icons".into(), "ubuntu".into()]);
    let medium = ["medium", "icons", "ubuntu", "mono"];
    fonts.families.insert(self::medium(), medium.into_iter().map(Into::into).collect());
    fonts
}

/// The system's fonts for what the GUI's own lack, and what was shown
/// when they were last looked for.
pub(crate) struct Fonts {
    /// The system's fonts for what tgradish's own lack.
    pub(crate) fallbacks: fallback::Fallbacks,
    /// What the texts were when fonts were last looked for them.
    checked: Option<u64>,
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts { fallbacks: fallback::Fallbacks::new(fonts()), checked: None }
    }
}

impl Fonts {
    /// Whether fonts are to be looked for what is shown, known by
    /// `shown`: only as it changes, or while fonts are looked for.
    fn look_again(&mut self, shown: u64) -> bool {
        if self.checked == Some(shown) && !self.fallbacks.looking() {
            return false;
        }
        self.checked = Some(shown);
        true
    }
}

impl App {
    /// Looks for the system's fonts for characters in names the GUI's
    /// own fonts lack.
    pub(crate) fn find_fonts(&mut self, ctx: &egui::Context) {
        // only as what is shown changes, or while fonts are looked for:
        // known by what the texts are made from, without making them
        let mut hasher = std::hash::DefaultHasher::new();
        for item in &self.items {
            (&item.inputs, item.made.as_ref().map(|made| &made.job.output)).hash(&mut hasher);
        }
        let selected = self.selected.and_then(|id| self.item(id));
        selected.map(|item| self.output_for(item)).hash(&mut hasher);
        self.banner.rows.iter().for_each(|said| said.id.hash(&mut hasher));
        self.dialogs.inspection.as_ref().map(|inspection| &inspection.path).hash(&mut hasher);
        self.config.gui.output_dir.hash(&mut hasher);
        let shown = hasher.finish();
        if !self.fonts.look_again(shown) {
            return;
        }
        let mut texts: Vec<String> = Vec::new();
        for item in &self.items {
            texts.push(item.name());
            texts.extend(item.inputs.iter().map(|path| path.display().to_string()));
            if let Some(made) = &item.made {
                texts.push(made.job.output.display().to_string());
            }
        }
        if let Some(item) = self.selected.and_then(|id| self.item(id)) {
            texts.push(self.output_for(item).display().to_string());
        }
        texts.extend(self.banner.rows.iter().flat_map(|said| said.message.texts()));
        texts.extend(
            self.dialogs
                .inspection
                .as_ref()
                .map(|inspection| inspection.path.display().to_string()),
        );
        texts.extend(self.config.gui.output_dir.as_ref().map(|dir| dir.display().to_string()));
        self.fonts.fallbacks.check(ctx, texts.iter().map(String::as_str));
    }
}

/// Body text in `colour`.
pub(crate) fn text_format(ui: &egui::Ui, colour: Color32) -> egui::TextFormat {
    egui::TextFormat::simple(egui::TextStyle::Body.resolve(ui.style()), colour)
}

/// `text` laid out as the GUI can draw it: each character no font has,
/// neither the GUI's nor the system's, as a small tag of its code, and
/// formatting characters, which draw nothing, left out. Also the
/// characters no font has.
pub(crate) fn drawable(
    ui: &egui::Ui,
    fallbacks: &fallback::Fallbacks,
    text: &str,
    format: egui::TextFormat,
) -> (egui::text::LayoutJob, Vec<char>) {
    drawable_in(fallbacks, text, format, ui.visuals().widgets.inactive.bg_fill)
}

pub(crate) fn drawable_in(
    fallbacks: &fallback::Fallbacks,
    text: &str,
    format: egui::TextFormat,
    tag_fill: Color32,
) -> (egui::text::LayoutJob, Vec<char>) {
    let mut job = egui::text::LayoutJob::default();
    let mut missing = Vec::new();
    if text.is_ascii() {
        job.append(text, 0.0, format);
        return (job, missing);
    }
    let tag = egui::TextFormat {
        font_id: egui::FontId::monospace(format.font_id.size * 0.72),
        color: format.color.gamma_multiply(0.7),
        background: tag_fill,
        valign: egui::Align::Center,
        ..Default::default()
    };
    let (mut run, mut gap) = (String::new(), 0.0);
    for c in text.chars() {
        if fallback::formatting(c) {
            continue;
        }
        if !fallbacks.missing(c) {
            run.push(c);
            continue;
        }
        if !run.is_empty() {
            job.append(&run, gap, format.clone());
            run.clear();
        }
        job.append(&format!("U+{:04X}", c as u32), 2.0, tag.clone());
        gap = 2.0;
        missing.push(c);
    }
    if !run.is_empty() {
        job.append(&run, gap, format);
    }
    (job, missing)
}

/// What to say of characters no font has, if there are any.
pub(crate) fn missing_note(missing: &[char]) -> Option<String> {
    // each once, in the order they come
    let mut codes: Vec<String> = Vec::new();
    for code in missing.iter().map(|c| format!("U+{:04X}", *c as u32)) {
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    if codes.is_empty() {
        return None;
    }
    Some(fl!("no-font", count = codes.len(), codes = codes.join(", ")))
}

/// Every character some built-in font draws.
#[cfg(test)]
pub(crate) fn drawn() -> std::collections::BTreeSet<char> {
    let fonts = fonts();
    fonts.font_data.values().flat_map(|data| fallback::characters(&data.font, data.index)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The letters and marks of the languages written in Latin and
    /// Cyrillic letters the GUI is or may be translated into, beyond
    /// English's: Ukrainian, Russian, Portuguese, Spanish, Indonesian,
    /// Uzbek, Turkish, German and French, and the quotes, dashes and
    /// spaces they write with.
    const LETTERS: [&str; 10] = [
        "ґєіїҐЄІЇёЁъЪыЫэЭ",
        "ãõáéíóúâêôàçÃÕÁÉÍÓÚÂÊÔÀÇ",
        "ñ¿¡üÑÜ",
        "ʻʼ",
        "ğşıİçöüĞŞÇÖÜ",
        "äöüßÄÖÜ",
        "éèêëàâçîïôûùüÿœæÉÈÊËÀÂÇÎÏÔÛÙÜŸŒÆ",
        "«»„“”‚‘’‹›",
        "–—…",
        "\u{a0}",
    ];

    /// The text's, the headings' and the commands' fonts draw [`LETTERS`]
    /// themselves, not by falling back on another.
    #[test]
    fn each_font_draws_the_letters_of_translations() {
        let fonts = fonts();
        for name in ["ubuntu", "medium", "mono"] {
            let data = &fonts.font_data[name];
            let drawn = fallback::characters(&data.font, data.index);
            let lacking: String = LETTERS.concat().chars().filter(|c| !drawn.contains(c)).collect();
            assert!(lacking.is_empty(), "{name} lacks {lacking:?}");
        }
        // Ubuntu lacks the capital ß, which JetBrains Mono draws for it
        assert!(drawn().contains(&'ẞ'));
    }

    #[test]
    fn explains_characters_no_font_has() {
        assert_eq!(missing_note(&[]), None);
        let one = missing_note(&['\u{2fe0}']).unwrap();
        assert!(one.starts_with("No font was found for U+2FE0,") && one.contains("its code"));
        let two = missing_note(&['\u{2fe0}', '\u{1faf9}', '\u{1faf9}']).unwrap();
        assert!(two.contains("U+2FE0, U+1FAF9,") && two.contains("their code"), "{two}");
        assert!(fallback::formatting('\u{200d}') && fallback::formatting('\u{fe0f}'));
        assert!(!fallback::formatting('a') && !fallback::formatting('日'));
    }
}

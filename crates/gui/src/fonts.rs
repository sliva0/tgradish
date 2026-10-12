//! The window's own fonts, and text laid out with them so that it always
//! reads as something: a character no font has, the system's included, is
//! shown as its code. `fallback` finds the system's fonts.

use eframe::egui::{self, Color32};

use crate::app::App;
use crate::fallback;

/// The window's fonts: Ubuntu Light for text, JetBrains Mono NL for
/// commands, and a few symbols; small subsets, see `assets/fonts`.
pub(crate) fn fonts() -> egui::FontDefinitions {
    use egui::{FontData, FontFamily};
    let mut fonts = egui::FontDefinitions::empty();
    for (name, bytes) in [
        ("ubuntu", &include_bytes!("../assets/fonts/Ubuntu-Light-tgradish.ttf")[..]),
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
    fonts
}

impl App {
    /// Looks for the system's fonts for characters in names the window's
    /// own fonts lack.
    pub(crate) fn find_fonts(&mut self, ctx: &egui::Context) {
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
        texts.extend(self.message.clone());
        texts.extend(
            self.inspection.as_ref().map(|inspection| inspection.path.display().to_string()),
        );
        texts.extend(self.config.gui.output_dir.as_ref().map(|dir| dir.display().to_string()));
        self.fallbacks.check(ctx, texts.iter().map(String::as_str));
    }
}

/// Body text in `colour`.
pub(crate) fn text_format(ui: &egui::Ui, colour: Color32) -> egui::TextFormat {
    egui::TextFormat::simple(egui::TextStyle::Body.resolve(ui.style()), colour)
}

/// `text` laid out as the window can draw it: each character no font has,
/// neither the window's nor the system's, as a small tag of its code, and
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
    let (them, are, their) = match codes.len() {
        0 => return None,
        1 => ("it", "is", "its"),
        _ => ("them", "are", "their"),
    };
    Some(format!(
        "No font was found for {}, neither among the window's own nor the system's, so {them} {are} \
         shown by {their} code. The file itself is fine.",
        codes.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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

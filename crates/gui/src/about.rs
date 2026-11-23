//! About dialog: tgradish's version and the third-party notices whose
//! licences require them in every copy. [`notices`] is public because
//! `tgradish licenses` prints the same list.

use eframe::egui::{self, RichText, vec2};

use tgradish_core::licenses::{Kind, Notice};

use crate::l10n::fl;
use crate::{icon, widgets};

/// The author's Telegram, for feedback; opens in the Telegram app where
/// installed (see [`crate::system::browse`]).
pub(crate) const FEEDBACK: &str = "https://t.me/s0untitled?direct";

/// Target content width of the About dialog across all languages; longer text
/// wraps.
pub(crate) const WIDTH: f32 = 600.0;

/// Most height of the About dialog's body, which scrolls: as tall as the
/// window lets it be up to this, whichever notices are open.
const MOST_BODY: f32 = 480.0;

/// Renders the About dialog body (version, links, and collapsible licence
/// notices), which scrolls as one.
pub(crate) fn about(ui: &mut egui::Ui) {
    let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    widgets::dialog_body_within(ui, area, MOST_BODY, |ui| {
        ui.horizontal(|ui| {
            icon::show(ui, 96.0);
            ui.vertical(|ui| {
                let version = format!("tgradish {}", env!("CARGO_PKG_VERSION"));
                ui.label(RichText::new(version).heading());
                // Wrap so a long translated summary does not widen the dialog.
                ui.horizontal_wrapped(|ui| {
                    ui.label(fl!("about-summary"));
                    widgets::link(ui, fl!("source-code"), tgradish_core::REPOSITORY);
                });
                widgets::link(ui, fl!("send-feedback"), FEEDBACK);
            });
        });
        ui.add_space(6.0);
        widgets::note(ui, fl!("about-notices"));
        ui.add_space(4.0);
        licences(ui);
    });
}

/// The collapsible licence notices.
fn licences(ui: &mut egui::Ui) {
    // read once, and kept in egui's memory as long as the GUI runs,
    // under a key of About's alone
    let notices = ui.ctx().memory_mut(|memory| {
        memory
            .data
            .get_temp_mut_or_insert_with(
                egui::Id::new("notices"),
                || std::sync::Arc::new(notices()),
            )
            .clone()
    });
    for notice in notices.iter() {
        let heading = wrapped(ui, title(notice.kind));
        egui::CollapsingHeader::new(heading).id_salt(("notice", notice.kind)).show(ui, |ui| {
            if matches!(notice.kind, Kind::Sources { of: "ffmpeg" }) {
                widgets::link(
                    ui,
                    fl!("ffmpeg-sources"),
                    format!(
                        "{}/releases/tag/v{}",
                        tgradish_core::REPOSITORY,
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
            // Split the third-party crate list into one sub-header per licence.
            let rule = format!("\n{}\n", "-".repeat(80));
            let blocks: Vec<&str> = notice.text.split(rule.as_str()).collect();
            if blocks.len() > 1 {
                widgets::note(ui, blocks[0].trim());
                for block in &blocks[1..] {
                    let (head, body) = block.split_once("\n\n").unwrap_or((block, ""));
                    let name = head.lines().next().unwrap_or("").trim_end_matches(", used by:");
                    let title = wrapped(ui, licence_title(name, head));
                    egui::CollapsingHeader::new(title).id_salt(("crates", *block)).show(ui, |ui| {
                        widgets::note(
                            ui,
                            head.lines().skip(1).map(str::trim).collect::<Vec<_>>().join(", "),
                        );
                        text_with_links(ui, body.trim());
                    });
                }
            } else {
                text_with_links(ui, notice.text.trim());
            }
        });
    }
}

/// Wraps a [`egui::CollapsingHeader`] title to the available width beside the
/// fold arrow (egui otherwise keeps header titles on a single line, widening
/// the dialog in narrow windows).
fn wrapped(ui: &egui::Ui, title: String) -> std::sync::Arc<egui::Galley> {
    let room = ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x;
    egui::WidgetText::from(title).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        room.max(0.0),
        egui::TextStyle::Button,
    )
}

/// Returns all licence notices for this build: tgradish's own licence, its
/// bundled GUI font licences, and those from [`tgradish_core::licenses`].
/// [`Notice::title`] returns English titles for the CLI; the GUI localises
/// them via `title`.
pub fn notices() -> Vec<Notice> {
    let fonts = [
        (
            Kind::Font { name: "Ubuntu Light and Medium derivative tgradish", subsets: 2 },
            include_str!("../assets/fonts/Ubuntu-LICENCE.txt"),
        ),
        (
            Kind::Font { name: "JetBrains Mono NL", subsets: 1 },
            include_str!("../assets/fonts/JetBrainsMono-OFL.txt"),
        ),
        (
            Kind::Font { name: "emoji-icon-font", subsets: 1 },
            include_str!("../assets/fonts/emoji-icon-font-LICENSE.txt"),
        ),
    ];
    let mut notices = tgradish_core::licenses::notices();
    // Insert font notices right before the long Rust crate list.
    let at = notices.iter().position(|notice| notice.kind == Kind::Crates).unwrap_or(notices.len());
    for (offset, (kind, text)) in fonts.into_iter().enumerate() {
        notices.insert(at + offset, Notice::new(kind, text));
    }
    notices
}

/// Formats a notice [`Kind`] in the current GUI language, keeping the names in
/// it as they are.
pub(crate) fn title(kind: Kind) -> String {
    match kind {
        Kind::Tgradish => "tgradish".to_owned(),
        Kind::Licence { of } => fl!("notice-licence", name = of),
        Kind::NamedLicence { of, licence } => {
            fl!("notice-named-licence", name = of, licence = licence)
        }
        Kind::LicenceNotes { of } => fl!("notice-licence-notes", name = of),
        Kind::PatentGrant { of } => fl!("notice-patent-grant", name = of),
        Kind::Sources { of } => fl!("notice-sources", name = of),
        Kind::Crates => fl!("notice-crates"),
        Kind::Font { name, subsets } => fl!("notice-font", name = name, subsets = subsets),
        // Fall back to English for any future Kind variant added in core.
        _ => kind.to_string(),
    }
}

/// Formats a crate-licence section header listing up to three crate names from
/// `head` (lines after the first are `"  <crate> <version>"`).
fn licence_title(licence: &str, head: &str) -> String {
    let crates: Vec<&str> =
        head.lines().skip(1).filter_map(|line| line.split_whitespace().next()).collect();
    match crates.as_slice() {
        [] => licence.to_owned(),
        [_] | [_, _] | [_, _, _] => {
            fl!("notice-crates-licence", licence = licence, crates = crates.join(", "))
        }
        [first @ .., _] => {
            let more = crates.len() - 3;
            let crates = first[..3].join(", ");
            fl!("notice-crates-licence", "more", licence = licence, crates = crates, more = more)
        }
    }
}

/// Renders licence `text` in small monospace with clickable HTTP(S) links.
fn text_with_links(ui: &mut egui::Ui, text: &str) {
    let mono = |text: &str| RichText::new(text).monospace().size(11.5);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // Batch consecutive plain-text lines into a single label.
        let mut plain = String::new();
        for line in text.lines() {
            let parts = links(line);
            if parts.iter().all(|(link, _)| !link) {
                plain.push_str(line);
                plain.push('\n');
                continue;
            }
            if !plain.is_empty() {
                ui.label(mono(plain.trim_end_matches('\n')));
                plain.clear();
            }
            ui.horizontal_wrapped(|ui| {
                for (link, part) in parts {
                    if link {
                        widgets::link(ui, mono(part), part);
                    } else {
                        ui.label(mono(part));
                    }
                }
            });
        }
        if !plain.is_empty() {
            ui.label(mono(plain.trim_end_matches('\n')));
        }
    });
}

/// Splits `line` into `(is_url, substring)` segments.
fn links(line: &str) -> Vec<(bool, &str)> {
    let mut parts = Vec::new();
    let mut rest = line;
    while let Some(at) = ["https://", "http://"].iter().filter_map(|scheme| rest.find(scheme)).min()
    {
        let length = rest[at..]
            .find(|c: char| c.is_whitespace() || "<>\"'()[]".contains(c))
            .unwrap_or(rest.len() - at);
        // Strip trailing sentence punctuation from the URL.
        let address = rest[at..at + length].trim_end_matches(['.', ',', ';', ':']);
        if at > 0 {
            parts.push((false, &rest[..at]));
        }
        parts.push((true, address));
        rest = &rest[at + address.len()..];
    }
    if !rest.is_empty() {
        parts.push((false, rest));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The summary, and the link to the source after it, wrap within
    /// About rather than widening it, at the narrowest the GUI goes.
    #[test]
    fn keeps_the_summary_within_abouts_width() {
        use egui_kittest::kittest::Queryable;
        use tgradish_core::presets::Presets;
        let mut harness = egui_kittest::Harness::builder()
            .with_size([crate::MIN_WIDTH, crate::MIN_HEIGHT])
            .build_eframe(|cc| {
                let config = crate::config::Config::default();
                let app = crate::app::App::with(config, None, Presets::builtin());
                // in the GUI's own fonts, whose widths the layout is of
                app.style(&cc.egui_ctx);
                app
            });
        harness.state_mut().dialogs.about = true;
        harness.run_steps(3);
        let rect = |label: String| {
            let nodes: Vec<_> =
                harness.query_all_by_label(&label).map(|node| node.rect()).collect();
            assert!(!nodes.is_empty(), "no {label:?}");
            nodes.into_iter().reduce(|a, b| a.union(b)).unwrap()
        };
        // the title starts the content, and Close ends it
        let left = rect(fl!("about-title")).left();
        let right = rect(fl!("close")).right();
        assert!(right - left <= WIDTH + 0.5, "About is {} wide", right - left);
        for label in [fl!("about-summary"), fl!("source-code")] {
            let shown = rect(label.clone());
            assert!(shown.left() >= left && shown.right() <= right, "{label:?} at {shown:?}");
        }
    }

    #[test]
    fn names_licences_after_their_crates() {
        let head = "MIT License, used by:\n  anyhow 1.0.104\n  arboard 3.6.1";
        assert_eq!(licence_title("MIT License", head), "MIT License: anyhow, arboard");
        let many = "MIT License, used by:\n  a 1\n  b 1\n  c 1\n  d 1\n  e 1";
        assert_eq!(licence_title("MIT License", many), "MIT License: a, b, c and 2 more");
    }

    /// Every notice's title, and each section's of the crates' licences,
    /// is worded by a message, as the GUI's other words are; tgradish's
    /// alone is just its name. In English, each reads as the command line
    /// prints it.
    #[test]
    fn words_every_notices_title_by_a_message() {
        use crate::l10n::record;
        let notices = notices();
        for notice in &notices {
            record::start();
            let title = title(notice.kind);
            assert_eq!(record::take().is_empty(), notice.kind == Kind::Tgradish, "{title}");
            assert_eq!(title, notice.title());
        }
        let crates = notices.iter().find(|notice| notice.kind == Kind::Crates).unwrap();
        let rule = format!("\n{}\n", "-".repeat(80));
        let head = crates.text.split(rule.as_str()).nth(1).unwrap().split("\n\n").next().unwrap();
        let name = head.lines().next().unwrap().trim_end_matches(", used by:");
        record::start();
        licence_title(name, head);
        assert_eq!(record::take().len(), 1);
    }

    #[test]
    fn finds_web_addresses_in_licences() {
        assert_eq!(
            links("See <https://www.apache.org/licenses/LICENSE-2.0>, or http://x.org."),
            [
                (false, "See <"),
                (true, "https://www.apache.org/licenses/LICENSE-2.0"),
                (false, ">, or "),
                (true, "http://x.org"),
                (false, "."),
            ]
        );
        assert_eq!(links("no address here"), [(false, "no address here")]);
    }
}

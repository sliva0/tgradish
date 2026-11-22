//! The About dialog: tgradish's version, and the notices of what it is
//! built from, whose licences ask for them to go with every copy. The CLI
//! prints the same notices, so `notices` is public.

use eframe::egui::{self, RichText, vec2};

use tgradish_core::licenses::{Kind, Notice};

use crate::l10n::fl;
use crate::{icon, widgets};

/// Where feedback goes: the author's Telegram, which opens in its app
/// where it is installed, as [`crate::system::browse`] opens it.
pub(crate) const FEEDBACK: &str = "https://t.me/s0untitled?direct";

/// How wide About's content is, whatever the language: what is longer
/// wraps.
pub(crate) const WIDTH: f32 = 600.0;

/// The height of what is around the notices in About: its title, what
/// is said over them, its button, its margins and the room kept around it
/// in the window; and the least and the most the notices are given.
const CHROME: f32 = 300.0;
const LEAST_ROOM: f32 = 80.0;
const MOST_ROOM: f32 = 360.0;

/// tgradish's version and the licences of what it is made of.
pub(crate) fn about(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        icon::show(ui, 96.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("tgradish {}", env!("CARGO_PKG_VERSION"))).heading());
            // wrapped: on one line, a longer summary would widen About
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
    // as tall as About fits the window, short of the most
    let room = (ui.ctx().content_rect().height() - CHROME).clamp(LEAST_ROOM, MOST_ROOM);
    let area = egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(room);
    widgets::scrolled(ui, area, |ui| {
        let notices = ui.ctx().memory_mut(|memory| {
            memory
                .data
                .get_temp_mut_or_insert_with(egui::Id::new("notices"), || {
                    std::sync::Arc::new(notices())
                })
                .clone()
        });
        for notice in notices.iter() {
            egui::CollapsingHeader::new(title(notice.kind)).id_salt(("notice", notice.kind)).show(
                ui,
                |ui| {
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
                    // the crates' list is long: a section for each licence
                    let rule = format!("\n{}\n", "-".repeat(80));
                    let blocks: Vec<&str> = notice.text.split(rule.as_str()).collect();
                    if blocks.len() > 1 {
                        widgets::note(ui, blocks[0].trim());
                        for block in &blocks[1..] {
                            let (head, body) = block.split_once("\n\n").unwrap_or((block, ""));
                            let name =
                                head.lines().next().unwrap_or("").trim_end_matches(", used by:");
                            let title = licence_title(name, head);
                            egui::CollapsingHeader::new(title).id_salt(("crates", *block)).show(
                                ui,
                                |ui| {
                                    widgets::note(
                                        ui,
                                        head.lines()
                                            .skip(1)
                                            .map(str::trim)
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                    );
                                    text_with_links(ui, body.trim());
                                },
                            );
                        }
                    } else {
                        text_with_links(ui, notice.text.trim());
                    }
                },
            );
        }
    });
}

/// The licence notices of this build: tgradish's, its fonts', and those
/// [`tgradish_core::licenses`] has. Their titles are in English, as the
/// command line prints them; About words them in the GUI's language.
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
    // before the long list of crates
    let at = notices.iter().position(|notice| notice.kind == Kind::Crates).unwrap_or(notices.len());
    for (offset, (kind, text)) in fonts.into_iter().enumerate() {
        notices.insert(at + offset, Notice::new(kind, text));
    }
    notices
}

/// A notice's title in the GUI's language, the names in it as they are.
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
        // a kind core has gained since, in English until the GUI words it
        _ => kind.to_string(),
    }
}

/// A licence's section named after the crates it covers, as many as
/// read at a glance: the crates are the lines after the first of `head`,
/// each a name and a version.
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

/// `text` small and monospaced, as licences are, its web addresses links.
fn text_with_links(ui: &mut egui::Ui, text: &str) {
    let mono = |text: &str| RichText::new(text).monospace().size(11.5);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // lines without addresses together, as one label
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

/// `line` in parts, each a web address or not.
fn links(line: &str) -> Vec<(bool, &str)> {
    let mut parts = Vec::new();
    let mut rest = line;
    while let Some(at) = ["https://", "http://"].iter().filter_map(|scheme| rest.find(scheme)).min()
    {
        let length = rest[at..]
            .find(|c: char| c.is_whitespace() || "<>\"'()[]".contains(c))
            .unwrap_or(rest.len() - at);
        // punctuation after it is the sentence's
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
        harness.state_mut().about = true;
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

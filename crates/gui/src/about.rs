//! The About dialog: tgradish's version, and the notices of what it is
//! built from, whose licences ask for them to go with every copy. The CLI
//! prints the same notices, so `notices` is public.

use eframe::egui::{self, RichText, vec2};

use crate::l10n::fl;
use crate::{icon, widgets};

/// tgradish's version and the licences of what it is made of.
pub(crate) fn about(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        icon::show(ui, 96.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("tgradish {}", env!("CARGO_PKG_VERSION"))).heading());
            ui.horizontal(|ui| {
                ui.label(fl!("about-summary"));
                widgets::link(ui, fl!("source-code"), tgradish_core::REPOSITORY);
            });
        });
    });
    ui.add_space(6.0);
    widgets::note(ui, fl!("about-notices"));
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(360.0).show(ui, |ui| {
        let notices = ui.ctx().memory_mut(|memory| {
            memory
                .data
                .get_temp_mut_or_insert_with(egui::Id::new("notices"), || {
                    std::sync::Arc::new(notices())
                })
                .clone()
        });
        for notice in notices.iter() {
            egui::CollapsingHeader::new(&notice.title).id_salt(("notice", &notice.title)).show(
                ui,
                |ui| {
                    if notice.title.starts_with("ffmpeg: what") {
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
/// [`tgradish_core::licenses`] has.
pub fn notices() -> Vec<tgradish_core::licenses::Notice> {
    use tgradish_core::licenses::Notice;
    let fonts = [
        ("Font: Ubuntu Light, a subset", include_str!("../assets/fonts/Ubuntu-LICENCE.txt")),
        (
            "Font: JetBrains Mono NL, a subset",
            include_str!("../assets/fonts/JetBrainsMono-OFL.txt"),
        ),
        (
            "Font: emoji-icon-font, a subset",
            include_str!("../assets/fonts/emoji-icon-font-LICENSE.txt"),
        ),
    ];
    let mut notices = tgradish_core::licenses::notices();
    // before the long list of crates
    let at =
        notices.iter().position(|notice| notice.title == "Rust crates").unwrap_or(notices.len());
    for (offset, (title, text)) in fonts.into_iter().enumerate() {
        notices.insert(at + offset, Notice { title: title.into(), text: text.into() });
    }
    notices
}

/// A licence's section named after the crates it covers, as many as
/// read at a glance: the crates are the lines after the first of `head`,
/// each a name and a version.
fn licence_title(licence: &str, head: &str) -> String {
    let crates: Vec<&str> =
        head.lines().skip(1).filter_map(|line| line.split_whitespace().next()).collect();
    match crates.as_slice() {
        [] => licence.to_owned(),
        [_] | [_, _] | [_, _, _] => format!("{licence}: {}", crates.join(", ")),
        [first @ .., _] => {
            format!("{licence}: {} and {} more", first[..3].join(", "), crates.len() - 3)
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

    #[test]
    fn names_licences_after_their_crates() {
        let head = "MIT License, used by:\n  anyhow 1.0.104\n  arboard 3.6.1";
        assert_eq!(licence_title("MIT License", head), "MIT License: anyhow, arboard");
        let many = "MIT License, used by:\n  a 1\n  b 1\n  c 1\n  d 1\n  e 1";
        assert_eq!(licence_title("MIT License", many), "MIT License: a, b, c and 2 more");
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

//! Encoder options for WebM, which most people never need, so they sit
//! folded away: libvpx-vp9 options by name, and raw ffmpeg arguments
//! where the system's ffmpeg runs.

use std::collections::BTreeMap;

use eframe::egui::{self, RichText};
use tgradish_core::options::Options;

use super::Context;
use crate::item::Item;
use crate::l10n::fl;
use crate::widgets::{self, note, row};

/// libvpx-vp9 options worth knowing, offered to add with a click.
fn encoder_examples() -> [(&'static str, &'static str, String); 6] {
    [
        ("tune-content", "screen", fl!("example-tune-content")),
        ("aq-mode", "2", fl!("example-aq-mode")),
        ("sharpness", "4", fl!("example-sharpness")),
        ("arnr-strength", "3", fl!("example-arnr-strength")),
        ("g", "60", fl!("example-g")),
        ("qmax", "50", fl!("example-qmax")),
    ]
}

pub(super) fn advanced(ui: &mut egui::Ui, item: &mut Item, context: &Context) {
    ui.add_space(10.0);
    egui::CollapsingHeader::new(RichText::new(fl!("encoder-options")).strong())
        .id_salt("advanced")
        .show(ui, |ui| {
            // what the preset sets, which the item's own go over, whole
            let base = item.choices.webm_base(context.presets, context.config).unwrap_or_default();
            row(ui, "libvpx-vp9", &fl!("vp9-options"), Ok(()), |ui| {
                encoder_rows(ui, item.id, &mut item.choices.webm, &base.encoder_options);
            });

            let system = fl!("ffmpeg-arguments", "system");
            let allowed = if context.extra_args { Ok(()) } else { Err(system.as_str()) };
            row(ui, &fl!("ffmpeg-arguments"), &fl!("ffmpeg-arguments", "hint"), allowed, |ui| {
                arguments(ui, item.id, &mut item.choices.webm, &base.extra_args);
                if !context.extra_args {
                    note(ui, system.as_str());
                }
            });
        });
}

/// The rows of an item's encoder options as they are edited, which its map
/// of them can't hold: a map sorts them by name, and has no place for a row
/// not named yet, nor for two of one name while one of them is typed.
#[derive(Clone)]
struct Rows {
    pairs: Vec<(String, String)>,
    /// The options the item had, its own or its preset's, when the rows
    /// were last in step with them. Once it has others, as after another
    /// preset was taken up, the rows are read from them again.
    written: Option<BTreeMap<String, String>>,
}

impl Rows {
    fn of(options: &Option<BTreeMap<String, String>>) -> Rows {
        let pairs = options.clone().unwrap_or_default().into_iter().collect();
        Rows { pairs, written: options.clone() }
    }

    /// The rows that have a name, as the item keeps them: the later of two
    /// of one name.
    fn named(&self) -> Option<BTreeMap<String, String>> {
        let named: BTreeMap<String, String> = self
            .pairs
            .iter()
            .map(|(name, value)| (name.trim().to_owned(), value.clone()))
            .filter(|(name, _)| !name.is_empty())
            .collect();
        (!named.is_empty()).then_some(named)
    }
}

/// The libvpx-vp9 options of the item `id`, a row each, with examples to
/// add by a click: its own, or else its `preset`'s, which the first change
/// makes its own. The rows stay where they are while they are typed, and
/// the item takes those that have a name.
fn encoder_rows(
    ui: &mut egui::Ui,
    id: u64,
    options: &mut Options,
    preset: &Option<BTreeMap<String, String>>,
) {
    let rows_id = egui::Id::new(("encoder option rows", id));
    let used = options.encoder_options.as_ref().or(preset.as_ref()).cloned();
    let mut rows = ui
        .data(|data| data.get_temp::<Rows>(rows_id))
        .filter(|rows| rows.written == used)
        .unwrap_or_else(|| Rows::of(&used));
    let mut changed = false;
    let mut remove = None;
    for (index, (name, value)) in rows.pairs.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::TextEdit::singleline(name)
                        .hint_text(fl!("vp9-options", "name"))
                        .desired_width(190.0),
                )
                .changed();
            ui.label("=");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(value)
                        .hint_text(fl!("vp9-options", "value"))
                        .desired_width(150.0),
                )
                .changed();
            if ui.small_button("✖").on_hover_text(fl!("remove")).clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        rows.pairs.remove(index);
        changed = true;
    }
    ui.horizontal(|ui| {
        if ui.small_button(fl!("add-option")).clicked() {
            rows.pairs.push((String::new(), String::new()));
        }
        if preset.is_some() && options.encoder_options.is_some() && widgets::reset(ui) {
            options.encoder_options = None;
            rows = Rows::of(preset);
        }
    });
    ui.horizontal_wrapped(|ui| {
        widgets::note(ui, fl!("examples"));
        for (name, value, hint) in encoder_examples() {
            if ui.small_button(format!("{name}={value}")).on_hover_text(hint).clicked() {
                match rows.pairs.iter_mut().find(|(other, _)| other.trim() == name) {
                    Some(pair) => pair.1 = value.to_owned(),
                    None => rows.pairs.push((name.to_owned(), value.to_owned())),
                }
                changed = true;
            }
        }
    });
    if changed {
        // the item's own are all it uses: none at all, where the preset
        // has some, is said by an empty map
        let named = rows.named();
        options.encoder_options = match preset {
            Some(preset) if named.as_ref() == Some(preset) => None,
            Some(_) => Some(named.clone().unwrap_or_default()),
            None => named.clone(),
        };
        rows.written = named.or_else(|| options.encoder_options.clone());
    }
    ui.data_mut(|data| data.insert_temp(rows_id, rows));
}

/// The raw ffmpeg arguments of the item `id`, typed as a shell would take
/// them: its own, or else its `preset`'s, which the first change makes its
/// own. What is typed is kept as it is while the field has the focus, as
/// the words alone can't say it: a space after the last word, a quote not
/// closed yet. The item takes the words whenever the text reads as some,
/// and once the field is left it shows them as the item has them.
fn arguments(ui: &mut egui::Ui, id: u64, options: &mut Options, preset: &Option<Vec<String>>) {
    let typed_id = egui::Id::new(("ffmpeg arguments typed", id));
    let words = options.extra_args.as_ref().or(preset.as_ref());
    let words = words.map(Vec::as_slice).unwrap_or_default();
    let joined = || shlex::try_join(words.iter().map(String::as_str)).unwrap_or_default();
    let mut text = ui.data(|data| data.get_temp::<String>(typed_id)).unwrap_or_else(joined);
    let (edit, reset) = ui
        .horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text(fl!("ffmpeg-arguments", "placeholder"))
                    .desired_width(300.0),
            );
            let own = preset.is_some() && options.extra_args.is_some();
            (edit, own && widgets::reset(ui))
        })
        .inner;
    let read = shlex::split(&text);
    if reset {
        options.extra_args = None;
    } else if edit.changed()
        && let Some(words) = &read
    {
        // the item's own are all it uses: none at all, where the preset
        // has some, is said by an empty list
        options.extra_args = match preset {
            Some(preset) if words == preset => None,
            Some(_) => Some(words.clone()),
            None => Some(words.clone()).filter(|words| !words.is_empty()),
        };
    }
    if edit.has_focus() {
        if read.is_none() {
            note(ui, fl!("ffmpeg-arguments", "unfinished"));
        }
        ui.data_mut(|data| data.insert_temp(typed_id, text));
    } else {
        ui.data_mut(|data| data.remove::<String>(typed_id));
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use tgradish_core::config::Config;
    use tgradish_core::presets::Presets;
    use tgradish_core::telegram::Target;

    use super::*;

    /// The folded options of a new item, open, with the system's ffmpeg.
    fn opened<'a>(presets: &'a Presets, config: &'a Config) -> Harness<'a, Item> {
        opened_with(presets, config, None)
    }

    /// [`opened`], the item made with `preset`.
    fn opened_with<'a>(
        presets: &'a Presets,
        config: &'a Config,
        preset: Option<&str>,
    ) -> Harness<'a, Item> {
        let context = Context {
            presets,
            config,
            ffmpeg: config.ffmpeg.clone(),
            extra_args: true,
            output: None,
            config_path: None,
            presets_dir: None,
            in_frames: false,
        };
        let mut item = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        item.choices.preset = preset.map(str::to_owned);
        let mut harness =
            Harness::new_ui_state(move |ui, item: &mut Item| advanced(ui, item, &context), item);
        widgets::test_fonts(&harness);
        harness.run_steps(2);
        harness.get_by_label(&fl!("encoder-options")).click();
        harness.run_steps(8);
        harness
    }

    /// Types `text` a key at a time, the window drawn after each.
    fn type_keys(harness: &mut Harness<Item>, text: &str) {
        for key in text.chars() {
            harness.event(egui::Event::Text(key.to_string()));
            harness.step();
        }
    }

    /// What the text fields show, in order.
    fn fields(harness: &Harness<Item>) -> Vec<String> {
        harness
            .query_all_by_role(Role::TextInput)
            .map(|field| field.value().unwrap_or_default())
            .collect()
    }

    #[test]
    fn keeps_encoder_option_rows_in_place_while_they_are_typed() {
        let (presets, config) = (Presets::builtin(), Config::default());
        let mut harness = opened(&presets, &config);
        let options = |harness: &Harness<Item>| {
            let options = harness.state().choices.webm.encoder_options.clone();
            options.unwrap_or_default().into_iter().collect::<Vec<_>>()
        };
        let pair = |name: &str, value: &str| (name.to_owned(), value.to_owned());
        harness.get_by_label("qmax=50").click();
        harness.run_steps(2);
        // two new rows, both there, under the one before, and neither the
        // item's until it has a name
        for _ in 0..2 {
            harness.get_by_label(&fl!("add-option")).click();
            harness.run_steps(2);
        }
        assert_eq!(fields(&harness), ["qmax", "50", "", "", "", "", ""]);
        assert_eq!(options(&harness), [pair("qmax", "50")]);
        // a name that sorts before the others, typed through one there is:
        // its row stays where it is, and so does the other of that name
        harness.query_all_by_role(Role::TextInput).nth(2).unwrap().click();
        harness.run_steps(2);
        type_keys(&mut harness, "qmax");
        assert_eq!(fields(&harness), ["qmax", "50", "qmax", "", "", "", ""]);
        type_keys(&mut harness, "-alpha");
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        type_keys(&mut harness, "40");
        assert_eq!(fields(&harness), ["qmax", "50", "qmax-alpha", "40", "", "", ""]);
        assert_eq!(options(&harness), [pair("qmax", "50"), pair("qmax-alpha", "40")]);
        // removed, the rows under it stay
        harness.query_all_by_label("✖").next().unwrap().click();
        harness.run_steps(2);
        assert_eq!(fields(&harness), ["qmax-alpha", "40", "", "", ""]);
        assert_eq!(options(&harness), [pair("qmax-alpha", "40")]);
    }

    #[test]
    fn shows_the_presets_options_and_adds_to_them() {
        let dir = tempfile::tempdir().unwrap();
        let preset = "[webm]\nextra-args = ['-qmin', '4']\n\
                      [webm.encoder-options]\naq-mode = '2'\n";
        std::fs::write(dir.path().join("tuned.toml"), preset).unwrap();
        let (presets, config) = (Presets::load(Some(dir.path())).unwrap(), Config::default());
        let mut harness = opened_with(&presets, &config, Some("tuned"));
        assert_eq!(fields(&harness), ["aq-mode", "2", "-qmin 4"]);
        let own = |harness: &Harness<Item>| harness.state().choices.webm.clone();
        assert_eq!((own(&harness).encoder_options, own(&harness).extra_args), (None, None));
        // one more is the item's with the preset's, not instead of it
        harness.get_by_label("qmax=50").click();
        harness.run_steps(2);
        let both = [("aq-mode", "2"), ("qmax", "50")];
        let both = both.map(|(name, value)| (name.to_owned(), value.to_owned()));
        assert_eq!(own(&harness).encoder_options, Some(both.into_iter().collect()));
        // all removed, the item uses none, over the preset's
        for _ in 0..2 {
            harness.query_all_by_label("✖").next().unwrap().click();
            harness.run_steps(2);
        }
        assert_eq!(own(&harness).encoder_options, Some(BTreeMap::new()));
        assert_eq!(fields(&harness), ["-qmin 4"]);
        // and back to the preset's
        harness.get_by_label("⟲").click();
        harness.run_steps(2);
        assert_eq!(own(&harness).encoder_options, None);
        assert_eq!(fields(&harness), ["aq-mode", "2", "-qmin 4"]);
        // arguments typed after the preset's are the item's, with them
        harness.query_all_by_role(Role::TextInput).last().unwrap().click();
        harness.run_steps(2);
        harness.key_press(egui::Key::End);
        harness.run_steps(2);
        type_keys(&mut harness, " -an");
        let words = ["-qmin", "4", "-an"].map(String::from);
        assert_eq!(own(&harness).extra_args, Some(words.to_vec()));
    }

    #[test]
    fn takes_ffmpeg_arguments_as_they_are_typed() {
        let (presets, config) = (Presets::builtin(), Config::default());
        let mut harness = opened(&presets, &config);
        harness.query_all_by_role(Role::TextInput).last().unwrap().click();
        harness.run_steps(2);
        let words = |harness: &Harness<Item>| harness.state().choices.webm.extra_args.clone();
        // the space between two words stays while the second is typed
        type_keys(&mut harness, "-qmin 4");
        assert_eq!(words(&harness), Some(vec!["-qmin".into(), "4".into()]));
        assert_eq!(fields(&harness), ["-qmin 4"]);
        // a quote left open: what is typed stays, and the words as they
        // last read
        type_keys(&mut harness, " -metadata title=\"my");
        assert_eq!(fields(&harness), ["-qmin 4 -metadata title=\"my"]);
        assert_eq!(words(&harness).map(|words| words.len()), Some(4));
        harness.get_by_label(&fl!("ffmpeg-arguments", "unfinished"));
        type_keys(&mut harness, " sticker\"");
        let read = ["-qmin", "4", "-metadata", "title=my sticker"].map(String::from);
        assert_eq!(words(&harness), Some(read.to_vec()));
        assert!(harness.query_by_label(&fl!("ffmpeg-arguments", "unfinished")).is_none());
        // left, the field shows the words as the item has them
        harness.key_press(egui::Key::Tab);
        harness.run_steps(3);
        assert_eq!(fields(&harness), ["-qmin 4 -metadata 'title=my sticker'"]);
        assert_eq!(words(&harness), Some(read.to_vec()));
    }
}

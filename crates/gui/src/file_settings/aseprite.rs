//! Aseprite input selection across all output formats: frame tag and visible
//! layers.

use std::rc::Rc;

use eframe::egui::{self, RichText};
use tgradish_core::presets::Format;
use tgradish_core::tgradish_tgs::frames::aseprite::{Direction, LayerInfo, Tag};

use crate::cuts::{Cut, Frames};
use crate::item::{Blocker, Choices, Item, quoted};
use crate::l10n::fl;
use crate::widgets::{self, row, section, section_with};

/// Indent of each level of groups in the list of layers.
const LEVEL: f32 = 20.0;
/// Where a level's line runs, from the start of its indent.
const TRUNK: f32 = 9.0;

/// The tag and layers to read, if there is a choice.
pub(super) fn aseprite(ui: &mut egui::Ui, item: &mut Item) {
    // kept, copied only when it changes, to show while it is read again
    let fresh = item.aseprite().map(|(tags, layers, durations)| {
        let seen = item.aseprite_seen.as_deref();
        let same =
            seen.is_some_and(|(t, l, d)| (&t[..], &l[..], &d[..]) == (tags, layers, durations));
        (!same).then(|| Rc::new((tags.to_vec(), layers.to_vec(), durations.to_vec())))
    });
    let read = fresh.is_some();
    if let Some(Some(changed)) = fresh {
        item.aseprite_seen = Some(changed);
    }
    let shown = read || item.input_reading();
    let read = item.aseprite_seen.clone().filter(|_| shown);
    let Some(read) = read else {
        // a tag or layer the file no longer has, as after it was edited:
        // nothing to choose from until it reads again
        let failed = item.input_error().is_some();
        let choices = &mut item.choices;
        let chosen =
            choices.tag.is_some() || choices.show_layers.is_some() || choices.hide_layers.is_some();
        if !(chosen && failed) {
            return;
        }
        section(ui, &fl!("aseprite-file"));
        ui.horizontal(|ui| {
            widgets::note(ui, fl!("unreadable-choices"));
            if ui.button(fl!("read-all")).clicked() {
                (choices.tag, choices.show_layers, choices.hide_layers) = (None, None, None);
            }
        });
        return;
    };
    let (tags, layers, frames) = &*read;
    // a hidden layer, even the only one, is one to show
    let listed = layers.len() >= 2 || layers.iter().any(|layer| !layer.visible);
    // chosen, but not the file's, as after it was edited
    let tag_missing = item.blockers.iter().find_map(|blocker| match blocker {
        Blocker::TagMissing(tag) => Some(tag.clone()),
        _ => None,
    });
    let layers_missing = item.blockers.iter().find_map(|blocker| match blocker {
        Blocker::LayerMissing(layers) => Some(layers.clone()),
        _ => None,
    });
    let tagged = !tags.is_empty() || tag_missing.is_some();
    let layered = listed || layers_missing.is_some();
    if !tagged && !layered {
        return;
    }
    // with a sign that it is being read again
    let reading = item.input_reading();
    section_with(ui, &fl!("aseprite-file"), |ui| {
        if reading {
            ui.add(egui::Spinner::new().size(12.0));
        }
    });
    let format = item.format;
    let choices = &mut item.choices;
    if tagged {
        row(ui, &fl!("tag"), &fl!("tag", "hint"), Ok(()), |ui| {
            tag_row(ui, choices, tags, frames, format);
            if let Some(tag) = &tag_missing {
                let text = fl!("tag-missing", tag = tag.as_str());
                if widgets::attention(ui, &text, &fl!("all-frames"), &fl!("all-frames", "hint")) {
                    choices.tag = None;
                    (choices.start, choices.length) = (None, None);
                }
            }
        });
    }
    if layered {
        row(ui, &fl!("layers"), &fl!("layers", "hint"), Ok(()), |ui| {
            if listed {
                layer_list(ui, choices, layers);
            }
            if let Some(missing) = &layers_missing {
                let (count, names) = (missing.len(), quoted(missing));
                let text = fl!("layers-missing", count = count, layers = names);
                let fix = (fl!("layers-as-file"), fl!("layers-as-file", "hint"));
                if widgets::attention(ui, &text, &fix.0, &fix.1) {
                    (choices.show_layers, choices.hide_layers) = (None, None);
                }
            }
        });
    }
}

/// The frames of an Aseprite file that shows each for `durations`, as the
/// timeline cuts them, whatever they are made into.
fn frames_of(durations: &[f64]) -> Option<Frames> {
    let mut times = vec![0.0];
    for duration in durations {
        times.push(times.last().unwrap() + duration);
    }
    (times.len() >= 2).then_some(Frames { times, cut: Cut::Art })
}

/// How high a layer's row is, less the space between them.
const LAYER: f32 = 22.0;

/// Side of the dot of a tag's colour on its button.
const DOT: f32 = 10.0;

/// The tags to pick, as buttons with dots of their colours. One played forward
/// sets where the input starts and ends, or for a picture, the frame it
/// is, as the timeline writes them; one that plays backwards, or there and
/// back, is read in its own order, which a cut can't do.
fn tag_row(
    ui: &mut egui::Ui,
    choices: &mut Choices,
    tags: &[Tag],
    durations: &[f64],
    format: Format,
) {
    let Some(frames) = frames_of(durations) else { return };
    let total = frames.times[frames.count()];
    let count = frames.count();
    // as boundaries: its first frame and the one after its last
    let span = |tag: &Tag| {
        let from = (tag.from as usize).min(count - 1);
        (from, (tag.to as usize + 1).clamp(from + 1, count))
    };
    let read = choices.tag.clone();
    let picture = format == Format::Webp;
    // the part used, which a tag played forward sets
    let used = frames.part(total, choices.start, choices.length);
    let shown = frames.at(choices.frame.unwrap_or(0.0));
    let by_name = |tag: &Tag| tag.direction != Direction::Forward;
    let whole = read.is_none() && (picture || used == (0, count));
    let picked = |tag: &Tag| match by_name(tag) || read.is_some() {
        false if picture => (span(tag).0..span(tag).1).contains(&shown),
        false => used == span(tag),
        true => read.as_deref() == Some(tag.name.as_str()),
    };
    let picks: Vec<bool> = tags.iter().map(picked).collect();
    ui.horizontal_wrapped(|ui| {
        let all = egui::Button::new(fl!("all-frames"));
        let all = widgets::as_choice(ui, all, whole && !picks.contains(&true));
        let all = ui.add(all).on_hover_text(fl!("all-frames", "hint"));
        if all.clicked() {
            choices.tag = None;
            (choices.start, choices.length) = (None, None);
        }
        for (tag, &picked) in tags.iter().zip(&picks) {
            let (from, after) = span(tag);
            if !tag_button(ui, tag, picked).clicked() {
                continue;
            }
            match by_name(tag) {
                // read in its order, the whole of which is used
                true if picked => choices.tag = None,
                true => {
                    choices.tag = Some(tag.name.clone());
                    (choices.start, choices.length) = (None, None);
                }
                // as where the input starts and ends, or its frame
                false if picked && !picture => (choices.start, choices.length) = (None, None),
                false => {
                    choices.tag = None;
                    match picture {
                        true => choices.frame = frames.start_of(from),
                        false => {
                            frames.set_part((from, after), &mut choices.start, &mut choices.length)
                        }
                    }
                }
            }
        }
    });
}

/// A tag's button, a choice like All frames beside it, lit when `picked`,
/// with a dot of the tag's colour before its name, and its frames on hover.
fn tag_button(ui: &mut egui::Ui, tag: &Tag, picked: bool) -> egui::Response {
    // opaque, as Aseprite shows them
    let [red, green, blue, _] = tag.colour;
    let colour = egui::Color32::from_rgb(red, green, blue);
    let dot = egui::Id::new(("tag colour", &tag.name));
    let atoms = (egui::Atom::custom(dot, egui::vec2(DOT, DOT)), tag.name.as_str());
    let button = widgets::as_choice(ui, egui::Button::new(atoms), picked);
    let drawn = button.atom_ui(ui);
    if let Some(rect) = drawn.rect(dot) {
        // outlined, so a colour near the button's still shows
        let edge = ui.visuals().widgets.inactive.bg_stroke;
        ui.painter().circle(rect.center(), DOT / 2.0, colour, edge);
    }
    let response = drawn.response;
    // as Aseprite counts them, from 1
    let (from, to) = (tag.from + 1, tag.to + 1);
    let frames = if from == to {
        fl!("tag-frame", number = from)
    } else {
        fl!("tag-frames", from = from, to = to)
    };
    response.on_hover_text(match tag.direction {
        Direction::Forward => frames,
        Direction::Reverse => fl!("tag-reverse", frames = frames),
        Direction::PingPong => fl!("tag-ping-pong", frames = frames),
        Direction::PingPongReverse => fl!("tag-pong-ping", frames = frames),
    })
}

/// Whether each layer is the last of its group's, or of the top level's.
fn last_of_group(layers: &[LayerInfo]) -> Vec<bool> {
    (0..layers.len())
        .map(|index| {
            let level = layers[index].level;
            // another at its level, before the group ends
            !layers[index + 1..]
                .iter()
                .take_while(|layer| layer.level >= level)
                .any(|layer| layer.level == level)
        })
        .collect()
}

/// A box for each layer, under its group, ticked where it shows, with
/// lines joining each to its group, as `tree` draws them.
fn layer_list(ui: &mut egui::Ui, choices: &mut Choices, layers: &[LayerInfo]) {
    // a list, closer than rows of settings, its names as Aseprite writes
    // them
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.spacing_mut().interact_size.y = LAYER;
    let last = last_of_group(layers);
    // for each level above the layer's, whether its line goes on below
    let mut going_on: Vec<bool> = Vec::new();
    // the level of the group hiding the layers listed under it
    let mut hidden_from: Option<u16> = None;
    for (index, layer) in layers.iter().enumerate() {
        if hidden_from.is_some_and(|level| layer.level <= level) {
            hidden_from = None;
        }
        let level = usize::from(layer.level);
        going_on.truncate(level);
        let mut on = shows(&named(choices), layer);
        let row = ui.horizontal(|ui| {
            ui.add_space(LEVEL * (level + 1) as f32);
            // as Aseprite writes them, the background underlined
            let name = RichText::new(&layer.name).monospace();
            let name = if layer.group { name.strong() } else { name };
            let name = if layer.background { name.underline() } else { name };
            let response = ui
                .add_enabled(hidden_from.is_none(), egui::Checkbox::new(&mut on, name))
                .on_disabled_hover_text(fl!("layers", "group-hidden"));
            if response.changed() {
                set(choices, layer, on);
            }
            // the first ends with a way back to the file's
            let own = choices.show_layers.is_some() || choices.hide_layers.is_some();
            if index == 0 && widgets::reset_to(ui, own, &fl!("layers", "file")) {
                (choices.show_layers, choices.hide_layers) = (None, None);
            }
        });
        // the lines: of the groups above it that go on, and its own
        let rect = row.response.rect;
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let painter = ui.painter();
        let x = |level: usize| rect.left() + LEVEL * level as f32 + TRUNK;
        let (top, bottom) = (rect.top() - 1.0, rect.bottom() + 1.0);
        for (above, _) in going_on.iter().enumerate().filter(|(_, on)| **on) {
            painter.vline(x(above), top..=bottom, stroke);
        }
        let middle = rect.center().y;
        let end = if last[index] { middle } else { bottom };
        painter.vline(x(level), top..=end, stroke);
        painter.hline(x(level)..=rect.left() + LEVEL * (level + 1) as f32 - 4.0, middle, stroke);
        going_on.push(!last[index]);
        if layer.group && !on && hidden_from.is_none() {
            hidden_from = Some(layer.level);
        }
    }
}

/// The layers the item shows and hides by name.
fn named(choices: &Choices) -> (Vec<String>, Vec<String>) {
    let list = |names: &Option<Vec<String>>| names.clone().unwrap_or_default();
    (list(&choices.show_layers), list(&choices.hide_layers))
}

/// Whether `layer` shows: as the file says, unless it is named.
fn shows((show, hide): &(Vec<String>, Vec<String>), layer: &LayerInfo) -> bool {
    !hide.contains(&layer.name) && (layer.visible || show.contains(&layer.name))
}

/// Shows or hides every layer named as `layer` is, naming it only where
/// that differs from the file.
fn set(choices: &mut Choices, layer: &LayerInfo, on: bool) {
    let (mut show, mut hide) = named(choices);
    show.retain(|name| *name != layer.name);
    hide.retain(|name| *name != layer.name);
    if on != layer.visible {
        if on { &mut show } else { &mut hide }.push(layer.name.clone());
    }
    choices.show_layers = (!show.is_empty()).then_some(show);
    choices.hide_layers = (!hide.is_empty()).then_some(hide);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgradish_core::telegram::Target;

    #[test]
    fn cuts_tags_as_the_timeline_does() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::{NodeT, Queryable};

        let tag = |name: &str, from, to| {
            let mut tag = Tag::default();
            (tag.name, tag.from, tag.to) = (name.into(), from, to);
            tag.colour = [200, 80, 80, 255];
            tag
        };
        let tags = [tag("idle", 0, 1), tag("walk", 2, 5)];
        let durations = [0.1; 8];
        let run = |choices: Choices, format: Format| {
            let tags = tags.to_vec();
            let mut harness = Harness::new_ui_state(
                move |ui, choices: &mut Choices| {
                    tag_row(ui, choices, &tags, &durations, format);
                },
                choices,
            );
            harness.run_steps(2);
            harness
        };
        // egui says a button is picked as toggled
        let picked = |harness: &Harness<Choices>, name: &str| {
            let toggled = harness.get_by_label(name).accesskit_node().toggled();
            toggled == Some(egui::accesskit::Toggled::True)
        };
        for format in [Format::Webm, Format::Tgs] {
            // written as the timeline writes the frames, where they start
            // and end, and still picked once the timeline writes them again
            let mut harness = run(Choices::new(Target::Sticker), format);
            harness.get_by_label("walk").click();
            harness.run_steps(2);
            let frames = frames_of(&durations).unwrap();
            let (mut start, mut length) = (None, None);
            frames.set_part((2, 6), &mut start, &mut length);
            let choices = harness.state();
            assert_eq!((choices.start, choices.length), (start, length), "{format:?}");
            assert_eq!((start, length), (Some(0.2), Some(0.4)));
            assert_eq!(frames.part(0.8, start, length), (2, 6));
            assert!(picked(&harness, "walk") && !picked(&harness, "idle"));
        }
        // a picture's frame starts where the picker puts it
        let mut harness = run(Choices::new(Target::Sticker), Format::Webp);
        harness.get_by_label("walk").click();
        harness.run_steps(2);
        assert_eq!(harness.state().frame, Some(0.2));
        assert!(picked(&harness, "walk"));
    }

    #[test]
    fn a_tag_picked_is_converted_as_its_flags_say() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        use std::time::Duration;
        use tgradish_core::input::Input;
        use tgradish_core::options::flags;
        use tgradish_core::tgradish_tgs::frames::Animation;
        use tgradish_core::webm::{self, read_from};

        // frames of awkward lengths, each its own colour, in an Aseprite
        // file with tags over them
        let delays = [33, 17, 33, 100, 7, 250, 16, 17, 80];
        let colours: Vec<[u8; 4]> = (0..delays.len()).map(|i| [10 + i as u8, 0, 0, 255]).collect();
        let frames = colours
            .iter()
            .map(|&colour| {
                let mut canvas = tgradish_corpus::Canvas::new(4, 4);
                canvas.rect(0, 0, 4, 4, colour);
                canvas
            })
            .collect();
        let sprite = tgradish_corpus::Sprite {
            name: "awkward",
            about: "frames of awkward lengths",
            frames,
            delays: delays.to_vec(),
            scale: 1,
            likely_scale: None,
        };
        let spans = [("a", 0, 0), ("b", 1, 3), ("c", 2, 6), ("d", 4, 4), ("e", 5, 8), ("f", 0, 7)];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("awkward.ase");
        std::fs::write(&path, tgradish_corpus::encode::aseprite(&sprite, &spans)).unwrap();
        let options = webm::Options::default();
        let cancel = tgradish_core::CancelToken::new();
        let mut readable =
            tgradish_core::drawn::readable(&Input::File(path), &options, &cancel).unwrap();
        let (tags, durations) = (readable.tags().to_vec(), readable.durations().to_vec());
        let (animation, _) = readable.take_drawn().unwrap();
        // which frames a .tgs normalised from `start` for `length` keeps
        let normalised = |start: Duration, length: Option<Duration>| {
            let mut normalise = tgradish_core::tgs::Options::default().sticker_options().unwrap();
            (normalise.normalise.start, normalise.normalise.length) = (start, length);
            let normalise = normalise.normalise;
            let (kept, report) =
                tgradish_core::tgradish_tgs::normalise(Animation::clone(&animation), &normalise)
                    .unwrap();
            let frames: Vec<_> = kept
                .frames()
                .iter()
                .map(|frame| (kept.palette()[usize::from(frame.pixels[0])], frame.ticks))
                .collect();
            (frames, report.dropped_frames)
        };
        let ms = |count: u32| f64::from(count) / 1000.0;
        for format in [Format::Tgs, Format::Webm, Format::Webp] {
            for (name, from, to) in spans {
                let mut harness = Harness::new_ui_state(
                    |ui, choices: &mut Choices| tag_row(ui, choices, &tags, &durations, format),
                    Choices::new(Target::Sticker),
                );
                harness.run_steps(2);
                harness.get_by_label(name).click();
                harness.run_steps(2);
                let choices = harness.state().clone();
                let what = format!("{format:?} {name}: {choices:?}");
                let (from, after) = (usize::from(from), usize::from(to) + 1);
                let start_ms: u32 = delays[..from].iter().sum();
                let length_ms: u32 = delays[from..after].iter().sum();
                // the flags of the command line, read back as it reads them
                let shown = flags(&tgradish_core::tgs::Options {
                    start: choices.start.or(choices.frame),
                    length: choices.length,
                    ..Default::default()
                });
                let read = |name: &str| -> Option<f64> {
                    let text = shown.iter().find_map(|flag| flag.strip_prefix(name))?;
                    Some(text.parse().unwrap())
                };
                let (start, length) = (read("--start="), read("--length="));
                // whole milliseconds, as the file times them
                let mut expected = Vec::new();
                if from > 0 {
                    expected.push(format!("--start={}", ms(start_ms)));
                }
                if after < delays.len() && format != Format::Webp {
                    expected.push(format!("--length={}", ms(length_ms)));
                }
                assert_eq!(shown, expected, "{what}");
                let times: Vec<f64> =
                    (0..=delays.len()).map(|i| ms(delays[..i].iter().sum())).collect();
                match format {
                    // the tag's frames, as if cut at them exactly
                    Format::Tgs => {
                        let exactly = |ms: u32| Duration::from_millis(u64::from(ms));
                        let tag = normalised(exactly(start_ms), Some(exactly(length_ms)));
                        let seconds = |value: Option<f64>| value.map(Duration::from_secs_f64);
                        let cut = normalised(seconds(start).unwrap_or_default(), seconds(length));
                        assert_eq!(cut, tag, "{what}");
                        let kept: Vec<[u8; 4]> = cut.0.iter().map(|&(colour, _)| colour).collect();
                        assert!(kept.iter().all(|colour| colours[from..after].contains(colour)));
                    }
                    // read from a hair before the first, for the frames' length
                    Format::Webm => {
                        let cut = readable.cut(&webm::Options { start, length, ..options.clone() });
                        let first = (from > 0).then(|| read_from(times[from], times[from - 1]));
                        assert_eq!(cut.start.is_some(), first.is_some(), "{what}");
                        let near = |a: f64, b: f64| (a - b).abs() < 1e-12;
                        assert!(cut.start.zip(first).is_none_or(|(a, b)| near(a, b)), "{what}");
                        let frames = times[after] - times[from];
                        assert!(cut.length.is_none_or(|cut| near(cut, frames)), "{what}");
                    }
                    // the tag's first frame
                    Format::Webp => {
                        let starts = &times[..delays.len()];
                        let at =
                            tgradish_core::webp::frame_start(start.unwrap_or(0.0), starts, 25.0);
                        let first =
                            if from > 0 { read_from(times[from], times[from - 1]) } else { 0.0 };
                        assert!((at - first).abs() < 1e-12, "{what}");
                    }
                }
            }
        }
    }

    #[test]
    fn names_layers_only_where_they_differ_from_the_file() {
        let layer = |name: &str, visible| {
            let mut layer = LayerInfo::default();
            (layer.name, layer.visible) = (name.into(), visible);
            layer
        };
        let (shadow, sketch) = (layer("Shadow", true), layer("Sketch", false));
        let mut choices = Choices::new(Target::Sticker);
        set(&mut choices, &shadow, false);
        set(&mut choices, &sketch, true);
        assert_eq!(choices.hide_layers, Some(vec!["Shadow".into()]));
        assert_eq!(choices.show_layers, Some(vec!["Sketch".into()]));
        let both = named(&choices);
        assert!(!shows(&both, &shadow) && shows(&both, &sketch));
        // back as the file has them, nothing is named
        set(&mut choices, &shadow, true);
        set(&mut choices, &sketch, false);
        assert_eq!((&choices.show_layers, &choices.hide_layers), (&None, &None));
    }
}

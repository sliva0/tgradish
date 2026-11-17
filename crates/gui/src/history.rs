//! Each file's settings as they were, step by step, for Ctrl+Z to go back
//! through and Ctrl+Shift+Z on again.
//!
//! The settings are changed in place by a great many controls, so rather
//! than each of them saying what it changed, every file's settings are
//! looked at as each frame ends, and a change becomes a step once it comes
//! to rest: once nothing holds the pointer down or types in a field, and a
//! frame has passed without it changing further. A drag, a button held to
//! repeat or a field typed in is then one step, and what follows from a
//! change over the next frames, as a crop kept to an integer scale, goes
//! with it rather than being a step of its own, which undoing would only
//! bring back.
//!
//! Only what the user did is a step: a change the GUI makes by itself, as
//! it settles what a file is once it is read, is taken in as it is.

use eframe::egui;
use tgradish_core::presets::Format;

use crate::item::{Aspect, Choices, Item};
use crate::widgets;

/// How many steps back each file keeps.
const STEPS: usize = 200;
/// How long after a step of the arrow keys the next one joins it, in
/// seconds: a crop nudged pixel by pixel is put back all at once.
const NUDGES_WITHIN: f64 = 1.0;

/// What of a file a step puts back: its choices, the format they are for,
/// and the shape the crop is kept to, which a change of the crop goes by.
/// Not how the preview is looked at or played.
#[derive(Debug, Clone, PartialEq)]
struct Settings {
    format: Format,
    format_guessed: bool,
    choices: Choices,
    aspect: Aspect,
}

impl Settings {
    fn of(item: &Item) -> Settings {
        Settings {
            format: item.format,
            format_guessed: item.format_guessed,
            choices: item.choices.clone(),
            aspect: item.view.aspect,
        }
    }

    /// Whether `item`'s settings are these, without copying them, as this
    /// is asked of every file every frame.
    fn matches(&self, item: &Item) -> bool {
        self.format == item.format
            && self.format_guessed == item.format_guessed
            && self.aspect == item.view.aspect
            && self.choices == item.choices
    }

    fn apply(self, item: &mut Item) {
        item.format = self.format;
        item.format_guessed = self.format_guessed;
        item.choices = self.choices;
        item.view.aspect = self.aspect;
    }
}

/// What a frame was, for the changes made in it.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Frame {
    /// Seconds since the GUI started.
    pub(crate) now: f64,
    /// Something is still being done: the pointer held down, or a field
    /// typed in. What it changes is a step once it is over.
    pub(crate) busy: bool,
    /// The user did something: pressed, typed, turned the wheel, or let go
    /// of what they did in the frame before.
    pub(crate) input: bool,
    /// An arrow key was pressed.
    pub(crate) arrows: bool,
    /// A field typed in was done with, by Enter, Tab or a click elsewhere:
    /// what it took is a step then, apart from what the click goes on to do.
    pub(crate) committed: bool,
}

/// What the frames before were, for what the next one is.
#[derive(Debug, Default)]
pub(crate) struct Frames {
    /// The field typed in as the last frame ended.
    typing: Option<egui::Id>,
    busy: bool,
    selected: Option<u64>,
}

impl Frames {
    /// What the frame ending in `ctx` was, for the file `selected`: the
    /// one the user changes, but in the frame it is picked, as it settles
    /// to be shown then.
    pub(crate) fn end(&mut self, ctx: &egui::Context, selected: Option<u64>) -> Frame {
        let typing = ctx.text_edit_focused() || widgets::typing_number(ctx);
        let typing = typing.then(|| ctx.memory(|memory| memory.focused())).flatten();
        let (now, down, did, arrows) = ctx.input(|input| {
            use egui::{Event, Key};
            let events = &input.raw.events;
            let did = events.iter().any(|event| {
                matches!(
                    event,
                    Event::PointerButton { .. }
                        | Event::Key { pressed: true, .. }
                        | Event::Text(_)
                        | Event::Paste(_)
                        | Event::Cut
                        | Event::MouseWheel { .. }
                        | Event::AccessKitActionRequest(_)
                )
            });
            let arrows = events.iter().any(|event| {
                let arrow = [Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown];
                matches!(event, Event::Key { key, pressed: true, .. } if arrow.contains(key))
            });
            (input.time, input.pointer.any_down(), did, arrows)
        });
        let busy = down || typing.is_some();
        let committed = self.typing.is_some() && self.typing != typing;
        let input = did || busy || self.busy;
        let picked = self.selected != selected;
        (self.typing, self.busy, self.selected) = (typing, busy, selected);
        Frame { now, busy, input: input && !picked, arrows, committed }
    }
}

/// A change since the last step, not at rest yet.
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// Made by the user, rather than by the GUI itself.
    user: bool,
    /// Made by the arrow keys, which join a step of them just before.
    nudge: bool,
}

/// A file's settings as they were before each step, and as they were
/// before each step undone.
#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Settings>,
    redo: Vec<Settings>,
    /// The settings as of the last step, or as they were first seen.
    settled: Option<Settings>,
    /// The settings as the last frame left them.
    seen: Option<Settings>,
    pending: Option<Pending>,
    /// When the last step was made, if by the arrow keys.
    nudged: Option<f64>,
    /// When the last frame ended.
    now: f64,
    /// A step was just undone or made again: what follows from it over the
    /// next frames, as a crop kept to an integer scale, is part of it.
    restoring: bool,
}

impl History {
    /// Whether there is anything to undo, a change not yet at rest too.
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending.is_some_and(|pending| pending.user)
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Takes in `item` as `frame` left it; true where its settings
    /// changed in it, so that another frame follows to see them at rest.
    fn follow(&mut self, item: &Item, frame: &Frame) -> bool {
        self.now = frame.now;
        let Some(seen) = &self.seen else {
            self.settled = Some(Settings::of(item));
            self.seen = self.settled.clone();
            return false;
        };
        let changed = !seen.matches(item);
        if changed {
            self.seen = Some(Settings::of(item));
            let pending = self.pending.get_or_insert(Pending { user: false, nudge: frame.arrows });
            pending.user |= frame.input;
        }
        if self.restoring {
            match changed {
                true => self.take_in(),
                false => self.restoring = false,
            }
            return changed;
        }
        let user = self.pending.is_some_and(|pending| pending.user);
        if (frame.committed && user) || !(changed || frame.busy) {
            self.settle();
        }
        changed
    }

    /// Makes what changed since the last step a step, if the user changed
    /// it and it changed anything, or else takes it in as it is.
    fn settle(&mut self) {
        let Some(pending) = self.pending.take() else { return };
        if !pending.user {
            return self.take_in();
        }
        let before = std::mem::replace(&mut self.settled, self.seen.clone());
        let Some(before) = before.filter(|before| Some(before) != self.seen.as_ref()) else {
            return;
        };
        let nudge = pending.nudge.then_some(self.now);
        let joins = nudge.zip(self.nudged).is_some_and(|(now, last)| now - last <= NUDGES_WITHIN);
        if !(joins && !self.undo.is_empty()) {
            self.undo.push(before);
            if self.undo.len() > STEPS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.nudged = nudge;
    }

    /// Takes the settings as they are now as they were all along.
    fn take_in(&mut self) {
        self.pending = None;
        self.settled = self.seen.clone();
    }

    /// Catches up with `item` as it is now, and makes what is pending a
    /// step: before a step is undone, or something else changes the
    /// settings. A change not seen yet is the GUI's, as what the user does
    /// is seen as the frame they do it in ends.
    fn flush(&mut self, item: &Item) {
        if self.seen.as_ref().is_some_and(|seen| !seen.matches(item)) {
            self.seen = Some(Settings::of(item));
            self.pending.get_or_insert(Pending { user: false, nudge: false });
        }
        if self.restoring {
            self.take_in();
            self.restoring = false;
        }
        self.settle();
        self.nudged = None;
    }

    /// Goes a step back, `back`, or on again, putting `item`'s settings
    /// as they were then; false where there was none to go to.
    fn step(&mut self, item: &mut Item, back: bool) -> bool {
        self.flush(item);
        let (from, to) = match back {
            true => (&mut self.undo, &mut self.redo),
            false => (&mut self.redo, &mut self.undo),
        };
        let Some(settings) = from.pop() else { return false };
        to.push(Settings::of(item));
        settings.clone().apply(item);
        self.settled = Some(settings.clone());
        self.seen = Some(settings);
        self.restoring = true;
        true
    }
}

/// Takes in each of `items` as `frame` left them; true where any changed,
/// so another frame follows to see them at rest.
pub(crate) fn follow<'a>(items: impl Iterator<Item = (&'a mut Item, Frame)>) -> bool {
    let mut changed = false;
    for (item, frame) in items {
        let mut history = std::mem::take(&mut item.history);
        changed |= history.follow(item, &frame);
        item.history = history;
    }
    changed
}

/// Makes what is pending of `item` a step, as before the GUI changes its
/// settings by itself, which [`take_in`] then takes as they are.
pub(crate) fn flush(item: &mut Item) {
    let mut history = std::mem::take(&mut item.history);
    history.flush(item);
    item.history = history;
}

/// Takes `item`'s settings as they are now as they were all along: a
/// change the GUI made, which is no step of the user's.
pub(crate) fn take_in(item: &mut Item) {
    item.history.seen = Some(Settings::of(item));
    item.history.take_in();
}

/// Undoes the last step of `item`, `back`, or makes the last one undone
/// again; false where there was none.
pub(crate) fn step(item: &mut Item, back: bool) -> bool {
    let mut history = std::mem::take(&mut item.history);
    let stepped = history.step(item, back);
    item.history = history;
    stepped
}

impl History {
    /// Every step's settings, and those as of now.
    fn all_mut(&mut self) -> impl Iterator<Item = &mut Settings> {
        let (settled, seen) = (self.settled.iter_mut(), self.seen.iter_mut());
        self.undo.iter_mut().chain(self.redo.iter_mut()).chain(settled).chain(seen)
    }

    /// The preset `from` renamed `to`: each step names it so too, or
    /// going back would name one that is gone.
    pub(crate) fn rename_preset(&mut self, from: &str, to: &str) {
        for settings in self.all_mut() {
            if settings.choices.preset.as_deref() == Some(from) {
                settings.choices.preset = Some(to.to_owned());
            }
        }
    }

    /// The format guessed for the file as it turned out to be once read,
    /// `format`, still a guess where `guessed`: each step that only had
    /// the guess before takes it too, as the user never picked one there.
    pub(crate) fn follow_guess(&mut self, format: Format, guessed: bool) {
        for settings in self.all_mut().filter(|settings| settings.format_guessed) {
            (settings.format, settings.format_guessed) = (format, guessed);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use eframe::egui::{self, Modifiers, PointerButton, Pos2, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use tgradish_core::options::Crop;
    use tgradish_core::telegram::Target;

    use super::*;
    use crate::app::App;
    use crate::config::Config;
    use crate::l10n::fl;
    use crate::testing::{convert, finish, has_ffmpeg};
    use tgradish_core::presets::Presets;

    /// The GUI with a 40 × 24 picture of `name` open and read, which is
    /// made a `.tgs` of, as one colour reads as pixel art.
    fn opened(dir: &Path, names: &[&str]) -> Harness<'static, App> {
        let arts = names.iter().map(|name| {
            let art = dir.join(name);
            let image = image::RgbaImage::from_pixel(40, 24, image::Rgba([200, 40, 90, 255]));
            image.save(&art).unwrap();
            art
        });
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.join("out"));
        // tall enough that the settings don't scroll, which they would as
        // what is said under them grows and shrinks
        let mut harness = Harness::builder()
            .with_size([1280.0, 1400.0])
            .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin()));
        widgets::test_fonts(&harness);
        harness.state_mut().add(arts.collect());
        finish(&mut harness);
        // settled as pixel art by itself: nothing to undo
        let app = harness.state();
        assert!(app.items.iter().all(|item| item.format == Format::Tgs));
        assert!(app.items.iter().all(|item| !item.history.can_undo()));
        harness
    }

    fn selected<'a>(harness: &'a Harness<'_, App>) -> &'a Item {
        let app = harness.state();
        app.selected.and_then(|id| app.item(id)).unwrap()
    }

    /// How many steps back the selected file has.
    fn steps(harness: &Harness<App>) -> usize {
        selected(harness).history.undo.len()
    }

    fn keys(harness: &mut Harness<App>, modifiers: Modifiers, key: egui::Key) {
        harness.key_press_modifiers(modifiers, key);
        harness.run_steps(2);
    }

    /// `key` pressed and let go of, as quickly as a frame each.
    fn nudge(harness: &mut Harness<App>, key: egui::Key) {
        for pressed in [true, false] {
            let modifiers = Modifiers::NONE;
            harness.event(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
        harness.run_steps(1);
    }

    fn undo(harness: &mut Harness<App>) {
        keys(harness, Modifiers::COMMAND, egui::Key::Z);
    }

    fn redo(harness: &mut Harness<App>) {
        keys(harness, Modifiers::COMMAND.plus(Modifiers::SHIFT), egui::Key::Z);
    }

    fn click(harness: &mut Harness<App>, label: &str) {
        harness.get_by_label(label).click();
        harness.run_steps(2);
    }

    /// Where the pixel `at` of the selected file's input is on the screen.
    fn on_picture(harness: &Harness<App>, at: egui::Vec2) -> Pos2 {
        let view = &selected(harness).view;
        let (_, fit_in) = view.input_zoom.anchor.unwrap();
        let (shown, scale) = crate::canvas::layout(fit_in, (40, 24), &view.input_zoom);
        shown.min + at * scale
    }

    /// The pointer pressed at `from`, moved to `to` over a few frames and
    /// let go of there.
    fn drag(harness: &mut Harness<App>, from: Pos2, to: Pos2) {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(from));
        harness.event(press(from, true));
        harness.step();
        for step in 1..=4 {
            harness.event(egui::Event::PointerMoved(from + (to - from) * step as f32 / 4.0));
            harness.step();
            // a step only once it is let go of
            assert!(selected(harness).history.pending.is_some_and(|pending| pending.user));
        }
        harness.event(press(to, false));
        harness.run_steps(3);
    }

    #[test]
    fn undoes_and_redoes_a_crop_dragged_a_drag_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        // a crop drawn, then moved
        let (from, to) =
            (on_picture(&harness, vec2(9.0, 5.0)), on_picture(&harness, vec2(27.0, 17.0)));
        drag(&mut harness, from, to);
        let drawn = selected(&harness).choices.crop;
        assert!(drawn.is_some());
        assert_eq!(steps(&harness), 1);
        let inside = on_picture(&harness, vec2(17.0, 11.0));
        drag(&mut harness, inside, inside + vec2(40.0, 25.0));
        let moved = selected(&harness).choices.crop;
        assert!(moved.is_some() && moved != drawn, "{moved:?}");
        assert_eq!(steps(&harness), 2);
        // back a drag at a time, exactly as each left it
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, drawn);
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, None);
        // nothing further back
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, None);
        // and on again, by either key
        redo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, drawn);
        keys(&mut harness, Modifiers::COMMAND, egui::Key::Y);
        assert_eq!(selected(&harness).choices.crop, moved);
        redo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, moved);
    }

    /// The crop's width field, by what it holds.
    fn width_field(holds: &str) -> egui_kittest::kittest::By<'_> {
        egui_kittest::kittest::by().role(egui::accesskit::Role::SpinButton).value(holds)
    }

    #[test]
    fn undoes_a_number_typed_as_one_step_once_it_is_taken() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        harness.get(width_field("40 px")).focus();
        harness.run_steps(2);
        for key in ["2", "0"] {
            harness.event(egui::Event::Text(key.into()));
            harness.run_steps(2);
        }
        // not taken yet, nor a step
        assert_eq!((selected(&harness).choices.crop, steps(&harness)), (None, 0));
        // Ctrl+Z in the field undoes its text, not the settings
        let typed = |harness: &Harness<App>| {
            harness.query_all(egui_kittest::kittest::by().value("20")).next().is_some()
        };
        assert!(typed(&harness));
        undo(&mut harness);
        assert!(harness.ctx.text_edit_focused() && !typed(&harness));
        assert_eq!((selected(&harness).choices.crop, steps(&harness)), (None, 0));
        // typed again, and taken by Enter: one step
        harness.event(egui::Event::Text("20".into()));
        harness.run_steps(2);
        assert!(typed(&harness));
        keys(&mut harness, Modifiers::NONE, egui::Key::Enter);
        let crop = selected(&harness).choices.crop;
        assert_eq!(crop.map(|crop| crop.width), Some(20));
        assert_eq!(steps(&harness), 1);
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, None);
        redo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, crop);
    }

    #[test]
    fn undoes_a_button_held_down_as_one_step() {
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        // a static sticker scaled by whole multiples, of a scale set
        click(&mut harness, &fl!("format-image"));
        finish(&mut harness);
        click(&mut harness, &fl!("scaling-integer"));
        click(&mut harness, &fl!("scale-fixed"));
        let set = selected(&harness).choices.webm.integer_scale;
        assert!(set.is_some());
        assert_eq!(steps(&harness), 3);
        // the scale's − held for a while, stepping several times: the
        // lower of those so named, as the small picture of the result has
        // one too
        let smaller = fl!("smaller");
        let all = harness.get_all_by_label(&smaller);
        let at = all.map(|node| node.rect()).max_by(|a, b| a.top().total_cmp(&b.top()));
        let at = at.unwrap().center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(at));
        harness.event(press(true));
        let mut scales = vec![set];
        for _ in 0..10 {
            harness.step();
            let scale = selected(&harness).choices.webm.integer_scale;
            if scales.last() != Some(&scale) {
                scales.push(scale);
            }
        }
        assert!(scales.len() > 3, "{scales:?}");
        harness.event(press(false));
        harness.run_steps(2);
        let held = selected(&harness).choices.webm.integer_scale;
        assert_eq!(Some(&held), scales.last());
        assert_eq!(steps(&harness), 4);
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.webm.integer_scale, set);
        redo(&mut harness);
        assert_eq!(selected(&harness).choices.webm.integer_scale, held);
    }

    #[test]
    fn forgets_what_was_undone_once_something_else_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        click(&mut harness, &fl!("target-emoji"));
        click(&mut harness, &fl!("format-video"));
        assert_eq!(steps(&harness), 2);
        undo(&mut harness);
        assert!(selected(&harness).history.can_redo());
        assert_eq!(selected(&harness).format, Format::Tgs);
        // a click that changes nothing leaves it to be redone
        click(&mut harness, &fl!("target-emoji"));
        assert!(selected(&harness).history.can_redo());
        click(&mut harness, &fl!("target-sticker"));
        assert!(!selected(&harness).history.can_redo());
        redo(&mut harness);
        assert_eq!(selected(&harness).choices.target, Target::Sticker);
        assert_eq!(selected(&harness).format, Format::Tgs);
        // back through the sticker and the emoji
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.target, Target::Emoji);
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.target, Target::Sticker);
        assert!(!selected(&harness).history.can_undo());
    }

    #[test]
    fn keeps_a_history_for_each_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["first.png", "second.png"]);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        let targets = |harness: &Harness<App>| -> Vec<Target> {
            harness.state().items.iter().map(|item| item.choices.target).collect()
        };
        harness.state_mut().select(ids[0]);
        harness.run_steps(2);
        click(&mut harness, &fl!("target-emoji"));
        harness.state_mut().select(ids[1]);
        harness.run_steps(2);
        click(&mut harness, &fl!("format-video"));
        assert_eq!(targets(&harness), [Target::Emoji, Target::Sticker]);
        // the first's, picked again
        harness.state_mut().select(ids[0]);
        harness.run_steps(2);
        undo(&mut harness);
        assert_eq!(targets(&harness), [Target::Sticker, Target::Sticker]);
        assert_eq!(harness.state().items[1].format, Format::Webm);
        undo(&mut harness);
        assert_eq!(harness.state().items[1].format, Format::Webm);
        // and the second's
        harness.state_mut().select(ids[1]);
        harness.run_steps(2);
        undo(&mut harness);
        assert_eq!(harness.state().items[1].format, Format::Tgs);
        assert_eq!(targets(&harness), [Target::Sticker, Target::Sticker]);
    }

    #[test]
    fn has_a_result_out_of_date_as_the_change_undone_would() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        click(&mut harness, &fl!("target-emoji"));
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert!(selected(&harness).result_is_current());
        undo(&mut harness);
        assert!(!selected(&harness).result_is_current());
        let outdated = |harness: &Harness<App>| {
            harness.query_by_label(&fl!("show-result", "outdated")).is_some()
        };
        assert!(outdated(&harness));
        redo(&mut harness);
        assert!(selected(&harness).result_is_current() && !outdated(&harness));
    }

    #[test]
    fn undoes_removing_first_and_brings_files_back_with_their_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        click(&mut harness, &fl!("target-emoji"));
        keys(&mut harness, Modifiers::NONE, egui::Key::Delete);
        assert!(harness.state().items.is_empty());
        // the file back, as the notice offers, with what it had
        undo(&mut harness);
        assert_eq!(harness.state().items.len(), 1);
        assert_eq!(selected(&harness).choices.target, Target::Emoji);
        assert!(harness.state().removed.is_none());
        // then its change
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.target, Target::Sticker);
        redo(&mut harness);
        assert_eq!(selected(&harness).choices.target, Target::Emoji);
    }

    #[test]
    fn joins_nudges_of_the_crop_made_close_together() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        let (from, to) =
            (on_picture(&harness, vec2(9.0, 5.0)), on_picture(&harness, vec2(27.0, 17.0)));
        drag(&mut harness, from, to);
        let drawn = selected(&harness).choices.crop.unwrap();
        // clicked inside, so the arrows move it
        let inside = on_picture(&harness, vec2(17.0, 11.0));
        harness.event(egui::Event::PointerMoved(inside));
        for pressed in [true, false] {
            let (button, modifiers) = (PointerButton::Primary, Modifiers::NONE);
            harness.event(egui::Event::PointerButton { pos: inside, button, pressed, modifiers });
            harness.step();
        }
        harness.run_steps(2);
        assert_eq!(steps(&harness), 1);
        // three a moment apart: one step
        for _ in 0..3 {
            nudge(&mut harness, egui::Key::ArrowRight);
        }
        let nudged = selected(&harness).choices.crop.unwrap();
        assert_eq!(nudged.x, drawn.x + 3);
        assert_eq!(steps(&harness), 2);
        // and after a pause, another
        harness.run_steps(6);
        nudge(&mut harness, egui::Key::ArrowDown);
        assert_eq!(steps(&harness), 3);
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, Some(nudged));
        undo(&mut harness);
        assert_eq!(selected(&harness).choices.crop, Some(drawn));
    }

    #[test]
    fn says_in_the_menu_what_can_be_undone_without_moving_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = opened(dir.path(), &["wide.png"]);
        let menu = |harness: &mut Harness<App>| {
            harness.get_by_label(&fl!("main-menu")).click();
            harness.run_steps(2);
            let rows = [fl!("menu-undo"), fl!("menu-redo"), fl!("menu-settings")];
            rows.map(|row| {
                let node = harness.get_by_label_contains(&row);
                (node.rect(), node.accesskit_node().is_disabled())
            })
        };
        let close = |harness: &mut Harness<App>| keys(harness, Modifiers::NONE, egui::Key::Escape);
        let before = menu(&mut harness);
        assert_eq!(before.map(|(_, off)| off), [true, true, false]);
        close(&mut harness);
        click(&mut harness, &fl!("target-emoji"));
        let after = menu(&mut harness);
        assert_eq!(after.map(|(_, off)| off), [false, true, false]);
        assert_eq!(after.map(|(rect, _)| rect), before.map(|(rect, _)| rect));
        // undone from the menu, it can be redone from it
        harness.get_by_label_contains(&fl!("menu-undo")).click();
        harness.run_steps(2);
        assert_eq!(selected(&harness).choices.target, Target::Sticker);
        let undone = menu(&mut harness);
        assert_eq!(undone.map(|(_, off)| off), [true, false, false]);
        harness.get_by_label_contains(&fl!("menu-redo")).click();
        harness.run_steps(2);
        assert_eq!(selected(&harness).choices.target, Target::Emoji);
    }

    /// `history` taking in `item` as a frame a moment after the last
    /// leaves it, the user doing something in it where `input`.
    fn frame(history: &mut History, item: &Item, input: bool) -> bool {
        let now = history.now + 0.25;
        history.follow(item, &Frame { now, input, ..Frame::default() })
    }

    #[test]
    fn takes_in_what_follows_from_a_change_and_what_the_gui_changes_itself() {
        let mut item = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        let mut history = History::default();
        frame(&mut history, &item, false);
        // a change, then what follows from it in the next frame, which
        // has no input: one step, once at rest
        item.choices.target = Target::Emoji;
        assert!(frame(&mut history, &item, true));
        item.choices.crop = Some(Crop { x: 0, y: 0, width: 4, height: 4 });
        assert!(frame(&mut history, &item, false));
        assert!(!frame(&mut history, &item, false));
        assert_eq!(history.undo.len(), 1);
        assert_eq!(history.undo[0].choices.target, Target::Sticker);
        // a change without input, as the GUI makes: no step
        item.choices.crop = None;
        frame(&mut history, &item, false);
        frame(&mut history, &item, false);
        assert_eq!(history.undo.len(), 1);
        // the preset of a step renamed with the one in use
        history.undo[0].choices.preset = Some("mine".into());
        history.rename_preset("mine", "ours");
        assert!(history.step(&mut item, true));
        assert_eq!(item.choices.preset.as_deref(), Some("ours"));
        assert_eq!(item.choices.target, Target::Sticker);
        // and on again, to the change as the GUI left it
        assert!(history.step(&mut item, false));
        assert_eq!((item.choices.target, item.choices.crop), (Target::Emoji, None));
    }

    #[test]
    fn keeps_so_many_steps_back() {
        let mut item = Item::new(1, vec!["pig.mp4".into()], false, Target::Sticker);
        let mut history = History::default();
        frame(&mut history, &item, false);
        for step in 0..STEPS + 10 {
            item.choices.start = Some(step as f64);
            frame(&mut history, &item, true);
            frame(&mut history, &item, false);
        }
        assert_eq!(history.undo.len(), STEPS);
        assert_eq!(history.undo[0].choices.start, Some(9.0));
    }
}

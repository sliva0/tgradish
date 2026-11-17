//! What the keyboard, the mouse wheel, the clipboard and dropped files ask
//! for. Keys are shortcuts only while no text field or dialog has them,
//! so typing is never taken for a command.

use std::path::PathBuf;

use eframe::egui::{self, vec2};
use tgradish_core::presets::Format;

use crate::app::App;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::dnd;
use crate::files::Pick;
use crate::item::{Item, Show};
use crate::l10n::fl;
use crate::widgets;

/// What the keys do. Each is listed in [`SHORTCUTS`], which both
/// the keys' handling and the dialog that lists them read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    AddFiles,
    Paste,
    /// The file above the one picked, or below.
    Select,
    Convert,
    /// What Convert all converts.
    ConvertAll,
    /// Keep best so far, while a WebM is fitted.
    KeepBest,
    /// The result made, copied as a file, as its Copy does: Ctrl+C, which
    /// egui-winit passes on as [`egui::Event::Copy`] instead.
    CopyResult,
    Remove,
    UndoRemoving,
    Play,
    /// A frame back or on, ten with Shift.
    Step,
    /// The first or the last frame used.
    Ends,
    /// The used part to start, or end, at the frame shown.
    CutHere,
    /// The used part's start a frame back or on, ten with Shift.
    MoveStart,
    /// Its end, likewise.
    MoveEnd,
    /// The GUI's scale, which [`crate::scaling::Scaling::keys`] takes
    /// by these keys, in place of egui.
    ScaleUp,
    ScaleDown,
    ScaleReset,
    Settings,
    Inspect,
    /// Escape, which egui's dialogs and menus take.
    Close,
    Shortcuts,
}

/// How the dialog of shortcuts groups them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    Files,
    Playing,
    General,
}

/// A command, and the keys that ask for it: where it goes two ways, back
/// and on, the first goes back.
pub(crate) struct Shortcut {
    pub(crate) command: Command,
    pub(crate) group: Group,
    pub(crate) keys: &'static [egui::KeyboardShortcut],
    /// Whether it is pressed with Shift held too: for ten frames at a
    /// time, or for a key typed with Shift, as `?` is on many keyboards.
    pub(crate) with_shift: bool,
}

const fn key(modifiers: egui::Modifiers, key: egui::Key) -> egui::KeyboardShortcut {
    egui::KeyboardShortcut::new(modifiers, key)
}

const fn shortcut(
    command: Command,
    group: Group,
    keys: &'static [egui::KeyboardShortcut],
    with_shift: bool,
) -> Shortcut {
    Shortcut { command, group, keys, with_shift }
}

/// Every shortcut, in the order the dialog lists them.
pub(crate) const SHORTCUTS: &[Shortcut] = {
    use egui::Key;
    use egui::Modifiers as M;
    use egui::gui_zoom::kb_shortcuts as zoom;
    use {Command as C, Group as G};
    &[
        shortcut(C::AddFiles, G::Files, &[key(M::COMMAND, Key::O)], false),
        shortcut(C::Paste, G::Files, &[key(M::COMMAND, Key::V)], false),
        shortcut(
            C::Select,
            G::Files,
            &[key(M::NONE, Key::ArrowUp), key(M::NONE, Key::ArrowDown)],
            false,
        ),
        shortcut(C::Convert, G::Files, &[key(M::COMMAND, Key::Enter)], false),
        shortcut(C::ConvertAll, G::Files, &[key(M::COMMAND.plus(M::SHIFT), Key::Enter)], false),
        shortcut(C::KeepBest, G::Files, &[key(M::COMMAND, Key::K)], false),
        shortcut(C::CopyResult, G::Files, &[key(M::COMMAND, Key::C)], false),
        shortcut(
            C::Remove,
            G::Files,
            &[key(M::NONE, Key::Delete), key(M::NONE, Key::Backspace)],
            false,
        ),
        shortcut(C::UndoRemoving, G::Files, &[key(M::COMMAND, Key::Z)], false),
        shortcut(C::Play, G::Playing, &[key(M::NONE, Key::Space)], false),
        shortcut(
            C::Step,
            G::Playing,
            &[key(M::NONE, Key::ArrowLeft), key(M::NONE, Key::ArrowRight)],
            true,
        ),
        shortcut(C::Ends, G::Playing, &[key(M::NONE, Key::Home), key(M::NONE, Key::End)], false),
        shortcut(
            C::CutHere,
            G::Playing,
            &[key(M::NONE, Key::OpenBracket), key(M::NONE, Key::CloseBracket)],
            false,
        ),
        shortcut(
            C::MoveStart,
            G::Playing,
            &[key(M::COMMAND, Key::ArrowLeft), key(M::COMMAND, Key::ArrowRight)],
            true,
        ),
        shortcut(
            C::MoveEnd,
            G::Playing,
            &[key(M::ALT, Key::ArrowLeft), key(M::ALT, Key::ArrowRight)],
            true,
        ),
        shortcut(C::Settings, G::General, &[key(M::COMMAND, Key::Comma)], false),
        shortcut(C::Inspect, G::General, &[key(M::COMMAND, Key::I)], false),
        shortcut(C::ScaleUp, G::General, &[zoom::ZOOM_IN, zoom::ZOOM_IN_SECONDARY], false),
        shortcut(C::ScaleDown, G::General, &[zoom::ZOOM_OUT], false),
        shortcut(C::ScaleReset, G::General, &[zoom::ZOOM_RESET], false),
        shortcut(
            C::Shortcuts,
            G::General,
            &[key(M::NONE, Key::Questionmark), key(M::NONE, Key::F1)],
            true,
        ),
        shortcut(C::Close, G::General, &[key(M::NONE, Key::Escape)], false),
    ]
};

/// The shortcut of `command`.
pub(crate) fn shortcut_of(command: Command) -> &'static Shortcut {
    let mut all = SHORTCUTS.iter();
    all.find(|shortcut| shortcut.command == command).expect("every command is listed")
}

/// Which of `command`'s keys was pressed this frame, by its place in its
/// [`Shortcut::keys`], and whether Shift was held.
pub(crate) fn pressed(input: &egui::InputState, command: Command) -> Option<(usize, bool)> {
    let shortcut = shortcut_of(command);
    let held = input.modifiers;
    let at = shortcut.keys.iter().position(|key| {
        let shifted =
            shortcut.with_shift && held.matches_exact(key.modifiers | egui::Modifiers::SHIFT);
        (held.matches_exact(key.modifiers) || shifted) && input.key_pressed(key.logical_key)
    });
    at.map(|at| (at, held.shift))
}

/// `key` as the GUI writes keys, in its language: Ctrl+Enter.
pub(crate) fn key_text(key: &egui::KeyboardShortcut) -> String {
    use egui::Key;
    let mac = cfg!(target_os = "macos");
    let held = key.modifiers;
    let mut parts = Vec::new();
    if held.ctrl && mac {
        parts.push(fl!("key-ctrl"));
    }
    if held.command || held.mac_cmd || (held.ctrl && !mac) {
        parts.push(if mac { fl!("key-cmd") } else { fl!("key-ctrl") });
    }
    if held.alt {
        parts.push(fl!("key-alt"));
    }
    if held.shift {
        parts.push(fl!("key-shift"));
    }
    parts.push(match key.logical_key {
        Key::Enter => fl!("key-enter"),
        Key::Space => fl!("key-space"),
        Key::Delete => fl!("key-delete"),
        Key::Backspace => fl!("key-backspace"),
        Key::Home => fl!("key-home"),
        Key::End => fl!("key-end"),
        Key::Escape => fl!("key-escape"),
        Key::ArrowLeft => fl!("key-left"),
        Key::ArrowRight => fl!("key-right"),
        Key::ArrowUp => fl!("key-up"),
        Key::ArrowDown => fl!("key-down"),
        // a hyphen, as typed, rather than egui's minus sign
        Key::Minus => "-".to_owned(),
        other => other.symbol_or_name().to_owned(),
    });
    parts.join("+")
}

/// The keys of `command`, as the GUI writes them: one, or each of them
/// apart by a slash.
pub(crate) fn keys_text(command: Command) -> String {
    let keys: Vec<String> = shortcut_of(command).keys.iter().map(key_text).collect();
    keys.join(" / ")
}

/// `hint`, said on hover of what `command` does too, with its keys after.
pub(crate) fn with_keys(hint: &str, command: Command) -> String {
    fl!("with-keys", hint = hint, keys = keys_text(command))
}

/// What `command` does, as the dialog of shortcuts says it.
fn what(command: Command) -> String {
    match command {
        Command::AddFiles => fl!("shortcut-add-files"),
        Command::Paste => fl!("shortcut-paste"),
        Command::Select => fl!("shortcut-select"),
        Command::Convert => fl!("shortcut-convert"),
        Command::ConvertAll => fl!("shortcut-convert-all"),
        Command::KeepBest => fl!("shortcut-keep-best"),
        Command::CopyResult => fl!("shortcut-copy-result"),
        Command::Remove => fl!("shortcut-remove"),
        Command::UndoRemoving => fl!("shortcut-undo"),
        Command::Play => fl!("shortcut-play"),
        Command::Step => fl!("shortcut-step"),
        Command::Ends => fl!("shortcut-ends"),
        Command::CutHere => fl!("shortcut-cut-here"),
        Command::MoveStart => fl!("shortcut-move-start"),
        Command::MoveEnd => fl!("shortcut-move-end"),
        Command::Settings => fl!("shortcut-settings"),
        Command::Inspect => fl!("shortcut-inspect"),
        Command::ScaleUp => fl!("shortcut-scale-up"),
        Command::ScaleDown => fl!("shortcut-scale-down"),
        Command::ScaleReset => fl!("shortcut-scale-reset"),
        Command::Close => fl!("shortcut-close"),
        Command::Shortcuts => fl!("shortcut-shortcuts"),
    }
}

impl App {
    /// The dialog that lists every shortcut, while it is open.
    pub(crate) fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.shortcuts {
            return;
        }
        let modal = widgets::dialog(ctx, "shortcuts").show(ctx, |ui| {
            ui.set_width(560.0);
            let crossed = widgets::dialog_title(ui, widgets::title_text(ui, fl!("shortcuts")));
            // the dialog fits the window, the list scrolling within
            let room = (ctx.content_rect().height() - 160.0).max(200.0);
            // that there is more below shows, as in Settings
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                widgets::settings_spacing(ui);
                let groups = [
                    (Group::Files, fl!("shortcuts-files")),
                    (Group::Playing, fl!("shortcuts-playing")),
                    (Group::General, fl!("shortcuts-general")),
                ];
                for (group, title) in groups {
                    let rows = SHORTCUTS.iter().filter(|shortcut| shortcut.group == group);
                    let rows =
                        rows.map(|shortcut| (keys_text(shortcut.command), what(shortcut.command)));
                    shortcut_rows(ui, &title, rows.collect());
                }
                // what the mouse does on the preview and the timeline, which
                // no key does
                let mouse = vec![
                    (fl!("mouse-drag"), fl!("mouse-drag", "does")),
                    (fl!("mouse-cancel"), fl!("mouse-cancel", "does")),
                    (fl!("mouse-double-click"), fl!("mouse-double-click", "does")),
                    (fl!("mouse-wheel"), fl!("mouse-wheel", "does")),
                    (fl!("mouse-pan"), fl!("mouse-pan", "does")),
                    (fl!("mouse-slide"), fl!("mouse-slide", "does")),
                ];
                shortcut_rows(ui, &fl!("shortcuts-mouse"), mouse);
            });
            let closed = widgets::dialog_buttons(ui, |ui| {
                ui.add(widgets::dialog_button(fl!("close"))).clicked()
            });
            crossed || closed
        });
        if modal.inner || modal.should_close() {
            self.shortcuts = false;
        }
    }
}

/// A group of shortcuts under `title`: each one's keys, and what they do.
fn shortcut_rows(ui: &mut egui::Ui, title: &str, rows: Vec<(String, String)>) {
    widgets::section(ui, title);
    // lines of text, closer than rows of controls
    let grid = egui::Grid::new(("shortcuts", title)).num_columns(2).min_row_height(0.0);
    grid.spacing(vec2(16.0, 6.0)).min_col_width(widgets::LABELS).show(ui, |ui| {
        for (keys, does) in rows {
            ui.label(egui::RichText::new(keys).strong());
            ui.label(does);
            ui.end_row();
        }
    });
}

impl App {
    /// Files dropped on the window, pasted, and keys.
    pub(crate) fn take_input(&mut self, ctx: &egui::Context) {
        let paste = ctx.input(|input| self.paste_key.read(&input.events));
        let (dropped, hovered) = ctx.input(|input| {
            let dropped: Vec<PathBuf> = input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect();
            (dropped, !input.raw.hovered_files.is_empty())
        });
        let asked = |command| ctx.input(|input| pressed(input, command));
        let (space, delete, undo, shortcuts) = (
            asked(Command::Play).is_some(),
            // Delete alone, as Backspace is for the row with the focus
            asked(Command::Remove).is_some_and(|(at, _)| at == 0),
            asked(Command::UndoRemoving).is_some(),
            asked(Command::Shortcuts).is_some(),
        );
        let (add, settings, inspect, keep_best) = (
            asked(Command::AddFiles).is_some(),
            asked(Command::Settings).is_some(),
            asked(Command::Inspect).is_some(),
            asked(Command::KeepBest).is_some(),
        );
        let copy = ctx.input(|input| {
            let copied = input.events.iter().any(|event| matches!(event, egui::Event::Copy));
            copied || pressed(input, Command::CopyResult).is_some()
        });
        // text picked in a label is what Ctrl+C copies then
        use egui::text_selection::LabelSelectionState;
        let selecting = ctx.with_plugin::<LabelSelectionState, _>(|labels| labels.has_selection());
        let waiting = self.keyed.waiting;
        self.keyed = Keyed::default();
        self.hovering = hovered || self.hovering;
        if !dropped.is_empty() {
            self.add(dropped);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(drops) = &self.drops {
            for dropped in drops.poll() {
                match dropped {
                    dnd::Dropped::Hovering(over) => self.hovering = over,
                    dnd::Dropped::Files(files) => self.add(files),
                }
            }
        }
        // keys pressed in a dialog are for it, not what is behind
        if self.dialog_open() {
            return;
        }
        // taken, so the button with the focus isn't pressed too, as egui has
        // Enter press it whatever is held with it: Ctrl+Shift+Enter first,
        // which taking Ctrl+Enter would take too
        let take = |command| {
            ctx.input_mut(|input| {
                let (at, _) = pressed(input, command)?;
                input.consume_shortcut(&shortcut_of(command).keys[at]);
                Some(())
            })
            .is_some()
        };
        let mut convert_all = take(Command::ConvertAll);
        // both from a field too, its value typed in; Convert's for the bar
        // under the preview, which has its button
        let mut convert = take(Command::Convert);
        // a number being typed is only taken as it is done with: taken this
        // frame, as its field is drawn, and converted with it the next
        if (convert || convert_all) && widgets::typing_number(ctx) {
            widgets::take_typed_number(ctx);
            self.keyed.waiting = Some((convert, convert_all));
            (convert, convert_all) = (false, false);
        }
        if let Some((one, all)) = waiting {
            (convert, convert_all) = (convert || one, convert_all || all);
        }
        self.keyed.convert = convert;
        if convert_all {
            self.convert_all();
        }
        // typing into a text field is just typing
        if ctx.text_edit_focused() {
            return;
        }
        if shortcuts {
            self.shortcuts = true;
            return;
        }
        if settings {
            self.settings_dialog.open();
            return;
        }
        if inspect {
            self.pick(ctx, Pick::Inspected);
            return;
        }
        if add {
            self.pick(ctx, Pick::Files);
            return;
        }
        // for the bar under the preview, which has the buttons they press
        self.keyed.copy = copy && selecting != Some(true);
        self.keyed.keep_best = keep_best;
        // while a notice offers it, as after a row with the focus was
        // removed by its key
        if undo {
            self.undo_removing();
        }
        // the rest are a focused button's own, as Space and the arrows
        if ctx.memory(|memory| memory.focused().is_some()) {
            return;
        }
        if let Some((at, _)) = asked(Command::Select) {
            self.select_beside(at == 0);
        }
        if space
            && let Some(id) = self.selected
            && let Some(item) = self.item_mut(id)
        {
            item.view.playing = !item.view.playing;
        }
        // back is the first of each pair of keys, on the second; with
        // Shift, ten frames at a time
        let by = |command| {
            asked(command).map_or(0, |(at, shift)| {
                let step = if shift { 10 } else { 1 };
                if at == 0 { -step } else { step }
            })
        };
        let (ends, cut) = (asked(Command::Ends), asked(Command::CutHere));
        let keys = Keys {
            by: by(Command::Step),
            move_start: by(Command::MoveStart),
            move_end: by(Command::MoveEnd),
            first: ends.is_some_and(|(at, _)| at == 0),
            last: ends.is_some_and(|(at, _)| at == 1),
            start_here: cut.is_some_and(|(at, _)| at == 0),
            end_here: cut.is_some_and(|(at, _)| at == 1),
        };
        if keys != Keys::default()
            && let Some(id) = self.selected
            && let Some(item) = self.item_mut(id)
        {
            keys.apply(item);
        }
        if delete && let Some(id) = self.selected {
            self.remove(id);
        }
        if paste {
            self.paste();
        }
    }
}

/// What keys asked of the bar under the preview this frame, which has the
/// buttons that do it too.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Keyed {
    /// Converted, as Convert does.
    pub(crate) convert: bool,
    /// Its result copied, as Copy does.
    pub(crate) copy: bool,
    /// Keep best so far pressed.
    pub(crate) keep_best: bool,
    /// Convert and Convert all asked for as a number was typed, for the
    /// next frame, once its field has taken it.
    waiting: Option<(bool, bool)>,
}

impl App {
    /// Picks the file above the one picked, or below, and has the list show
    /// it: the last or the first where none is picked.
    fn select_beside(&mut self, above: bool) {
        let at = self.selected.and_then(|id| self.items.iter().position(|item| item.id == id));
        let next = match (at, above) {
            (Some(at), true) => at.checked_sub(1),
            (Some(at), false) => Some(at + 1),
            (None, true) => self.items.len().checked_sub(1),
            (None, false) => Some(0),
        };
        if let Some(id) = next.and_then(|next| self.items.get(next)).map(|item| item.id) {
            self.select(id);
            self.reveal_selected = true;
        }
    }
}

/// Ctrl+V, as egui-winit passes it on: it swallows V going down with Ctrl,
/// and says so only where the clipboard holds text, which it pastes as
/// text. Files and images, which it doesn't, are pasted as V comes up
/// instead, once for each time it went down.
#[derive(Debug, Default)]
pub(crate) struct PasteKey {
    /// The clipboard was read for the V held, as it held text.
    read: bool,
    /// V went down without Ctrl, which egui-winit passes on as a key.
    plain: bool,
}

impl PasteKey {
    /// Takes in a frame's `events`, and gives whether they ask for the
    /// clipboard to be pasted. Kept up with while keys go to a text field
    /// or a dialog too, which take a paste themselves, so V going down
    /// there isn't taken for Ctrl+V once it comes up outside.
    fn read(&mut self, events: &[egui::Event]) -> bool {
        let v = shortcut_of(Command::Paste).keys[0].logical_key;
        let mut paste = false;
        for event in events {
            match event {
                egui::Event::Paste(_) => {
                    paste = true;
                    self.read = true;
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } if *key == v => {
                    self.plain = !modifiers.command;
                }
                // went down as Ctrl+V where egui-winit passed nothing on,
                // whether Ctrl is still held or let go first
                egui::Event::Key { key, pressed: false, .. } if *key == v => {
                    paste |= !self.plain && !self.read;
                    *self = PasteKey::default();
                }
                _ => {}
            }
        }
        paste
    }
}

/// What the keys over the preview ask for: a step of frames, the first
/// or last frame used, the used part to start or end at the frame shown,
/// and its start or end to move by frames.
#[derive(Debug, Default, PartialEq, Eq)]
struct Keys {
    by: i64,
    move_start: i64,
    move_end: i64,
    first: bool,
    last: bool,
    start_here: bool,
    end_here: bool,
}

impl Keys {
    /// Moves through what `item` shows, frame by frame, stopping playback.
    fn apply(&self, item: &mut Item) {
        let step = |now: usize, first: usize, last: usize| -> usize {
            if self.first {
                first
            } else if self.last {
                last
            } else {
                (now as i64 + self.by).clamp(first as i64, last as i64) as usize
            }
        };
        let moves = self.by != 0 || self.first || self.last;
        let trims = self.move_start != 0 || self.move_end != 0;
        match item.shown() {
            Show::Result => {
                let Some(clip) = item.result_clip.ready() else { return };
                if moves {
                    let at = step(clip.index_at(item.view.result_time), 0, clip.frames.len() - 1);
                    item.view.result_time = clip.times[at];
                    item.view.playing = false;
                }
            }
            // a picture is made of the frame picked
            Show::Input if item.format == Format::Webp => {
                let Some(frames) = item.input_frames() else { return };
                if moves {
                    let at =
                        step(frames.at(item.choices.frame.unwrap_or(0.0)), 0, frames.count() - 1);
                    item.choices.frame = frames.start_of(at);
                }
            }
            Show::Input => {
                let (Some(frames), Some(length)) = (item.input_frames(), item.input_length())
                else {
                    return;
                };
                let choices = &mut item.choices;
                let now = frames.at(item.view.time);
                let (first, after) = frames.part(length, choices.start, choices.length);
                if self.start_here && now < after {
                    frames.set_part((now, after), &mut choices.start, &mut choices.length);
                }
                if self.end_here && now + 1 > first {
                    frames.set_part((first, now + 1), &mut choices.start, &mut choices.length);
                }
                if moves {
                    let (first, after) = frames.part(length, choices.start, choices.length);
                    item.view.time = frames.times[step(now, first, after - 1)];
                    item.view.playing = false;
                }
                // showing the frame it moves to
                if trims {
                    let (first, after) = frames.part(length, choices.start, choices.length);
                    let (first, after) = if self.move_start != 0 {
                        let moved = (first as i64 + self.move_start).clamp(0, after as i64 - 1);
                        item.view.time = frames.times[moved as usize];
                        (moved as usize, after)
                    } else {
                        let count = frames.count() as i64;
                        let moved = (after as i64 + self.move_end).clamp(first as i64 + 1, count);
                        item.view.time = frames.times[moved as usize - 1];
                        (first, moved as usize)
                    };
                    frames.set_part((first, after), &mut choices.start, &mut choices.length);
                    item.view.playing = false;
                }
            }
        }
    }
}

/// Scrolls by what the wheel turned this frame, instead of egui's easing
/// over the next few frames.
pub(crate) fn direct_scrolling(ctx: &egui::Context) {
    let (line, page) = (
        ctx.options(|options| options.input_options.line_scroll_speed),
        ctx.content_rect().height(),
    );
    ctx.input_mut(|input| {
        // the command key turns the wheel into zooming
        if input.modifiers.command {
            return;
        }
        let mut delta = egui::Vec2::ZERO;
        for event in &input.raw.events {
            if let egui::Event::MouseWheel { unit, delta: turned, .. } = event {
                delta += match unit {
                    egui::MouseWheelUnit::Point => *turned,
                    egui::MouseWheelUnit::Line => *turned * line,
                    egui::MouseWheelUnit::Page => *turned * page,
                };
            }
        }
        if input.modifiers.shift {
            delta = vec2(delta.x + delta.y, 0.0);
        }
        input.smooth_scroll_delta = delta;
    });
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;

    use super::*;
    use crate::jobs::Status;
    use crate::testing::{convert, finish, harness, made_job, square, video_of};

    #[test]
    fn pastes_once_for_each_ctrl_v_whichever_key_comes_up_first() {
        use egui::{Event, Key, Modifiers};
        let v = |pressed, modifiers| Event::Key {
            key: Key::V,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        let (ctrl, none) = (Modifiers::COMMAND, Modifiers::NONE);
        let text = || Event::Paste("/a.png".into());
        // each frame's events, as egui-winit gives them, and what is pasted
        let pastes = |frames: &[Vec<Event>]| -> Vec<bool> {
            let mut key = PasteKey::default();
            frames.iter().map(|events| key.read(events)).collect()
        };
        // files or an image: nothing as V goes down, pasted as it comes up,
        // with Ctrl held or let go first
        assert_eq!(pastes(&[vec![], vec![v(false, ctrl)]]), [false, true]);
        assert_eq!(pastes(&[vec![], vec![v(false, none)]]), [false, true]);
        // text: pasted as V goes down, not again as it comes up
        assert_eq!(pastes(&[vec![text()], vec![v(false, ctrl)]]), [true, false]);
        assert_eq!(pastes(&[vec![text()], vec![v(false, none)]]), [true, false]);
        // and again for the next Ctrl+V
        assert_eq!(
            pastes(&[vec![text()], vec![v(false, ctrl)], vec![v(false, ctrl)]]),
            [true, false, true]
        );
        // V alone pastes nothing, nor V held down before Ctrl
        assert_eq!(pastes(&[vec![v(true, none)], vec![v(false, none)]]), [false, false]);
        assert_eq!(pastes(&[vec![v(true, none)], vec![v(false, ctrl)]]), [false, false]);
        // where a backend passes Ctrl+V on as a key, it is pasted all the same
        assert_eq!(pastes(&[vec![v(true, ctrl)], vec![v(false, none)]]), [false, true]);
    }

    #[test]
    fn lists_every_shortcut_it_takes_once() {
        use crate::l10n::fl;
        use egui_kittest::kittest::Queryable;
        // no keys ask for two things
        let keys: Vec<_> = SHORTCUTS.iter().flat_map(|shortcut| shortcut.keys).collect();
        for (at, key) in keys.iter().enumerate() {
            assert!(!keys[..at].contains(key), "{key:?} twice");
        }
        assert_eq!(keys_text(Command::Convert), "Ctrl+Enter");
        assert_eq!(keys_text(Command::ConvertAll), "Ctrl+Shift+Enter");
        assert_eq!(keys_text(Command::Settings), "Ctrl+,");
        assert_eq!(keys_text(Command::Select), "Up / Down");
        assert_eq!(keys_text(Command::Step), "Left / Right");
        assert_eq!(keys_text(Command::ScaleUp), "Ctrl++ / Ctrl+=");
        assert_eq!(keys_text(Command::ScaleDown), "Ctrl+-");
        // F1 and ?, typed with Shift, open the list, which has them all
        let mut harness = harness(Config::default());
        for (modifiers, key) in [
            (egui::Modifiers::NONE, egui::Key::F1),
            (egui::Modifiers::SHIFT, egui::Key::Questionmark),
        ] {
            harness.key_press_modifiers(modifiers, key);
            harness.run_steps(2);
            assert!(harness.state().shortcuts, "{key:?}");
            for shortcut in SHORTCUTS {
                harness.get_by_label(&keys_text(shortcut.command));
                harness.get_by_label(&what(shortcut.command));
            }
            harness.get_by_label(&fl!("mouse-wheel", "does"));
            harness.key_press(egui::Key::Escape);
            harness.run_steps(2);
            assert!(!harness.state().shortcuts);
        }
        // from the menu too
        harness.get_by_label(&fl!("main-menu")).click();
        harness.run_steps(2);
        harness.get_by_label_contains(&fl!("shortcuts")).click();
        harness.run_steps(2);
        assert!(harness.state().shortcuts);
        // and not while another dialog is open, whose keys they are
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.key_press(egui::Key::F1);
        harness.run_steps(2);
        assert!(!harness.state().shortcuts);
    }

    #[test]
    fn opens_settings_inspecting_and_adding_files_by_their_keys() {
        use crate::l10n::fl;
        use egui_kittest::kittest::Queryable;
        let mut harness = harness(Config::default());
        let command = |harness: &mut egui_kittest::Harness<App>, key| {
            harness.key_press_modifiers(egui::Modifiers::COMMAND, key);
            harness.run_steps(2);
        };
        command(&mut harness, egui::Key::Comma);
        assert!(harness.state().settings_dialog.is_open());
        // none of the others while it is open, whose keys they are
        command(&mut harness, egui::Key::I);
        command(&mut harness, egui::Key::O);
        assert!(harness.state().asked_to_pick.is_empty());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        command(&mut harness, egui::Key::I);
        command(&mut harness, egui::Key::O);
        assert_eq!(harness.state().asked_to_pick, [Pick::Inspected, Pick::Files]);
        // the menu's rows say their keys, as Convert does
        harness.get_by_label(&fl!("main-menu")).click();
        harness.run_steps(2);
        for (command, row) in [
            (Command::Settings, fl!("menu-settings")),
            (Command::Inspect, fl!("inspect")),
            (Command::Shortcuts, fl!("shortcuts")),
        ] {
            harness.get_by_label_contains(&row);
            harness.get_by_label_contains(&keys_text(command));
        }
    }

    #[test]
    fn picks_the_file_above_or_below_by_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let arts: Vec<_> = (0..3)
            .map(|n| {
                let art = dir.path().join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        let mut harness = harness(Config::default());
        harness.state_mut().add(arts);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        harness.state_mut().select(ids[1]);
        harness.run_steps(2);
        let mut picked = Vec::new();
        use egui::Key::{ArrowDown as Down, ArrowUp as Up};
        for key in [Down, Down, Up, Up, Up] {
            harness.key_press(key);
            harness.run_steps(2);
            picked.push(harness.state().selected.unwrap());
        }
        // stopping at the ends
        assert_eq!(picked, [ids[2], ids[2], ids[1], ids[0], ids[0]]);
    }

    #[test]
    fn converts_all_by_its_key_from_a_field_too() {
        use egui_kittest::kittest::Queryable;
        let dir = tempfile::tempdir().unwrap();
        let arts: Vec<_> = (0..2)
            .map(|n| {
                let art = dir.path().join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(arts);
        finish(&mut harness);
        // a value typed in, as Ctrl+Enter converts from a field too
        let field = egui::accesskit::Role::SpinButton;
        harness.query_all_by_role(field).next().expect("a number field").focus();
        harness.run_steps(2);
        assert!(harness.ctx.text_edit_focused());
        let both = egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT);
        harness.key_press_modifiers(both, egui::Key::Enter);
        harness.run_steps(2);
        finish(&mut harness);
        assert!(harness.state().items.iter().all(|item| item.made.is_some()));
    }

    #[test]
    fn converts_a_number_typed_by_its_key_once_taken() {
        use egui_kittest::kittest::Queryable;
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("wide.png");
        image::RgbaImage::from_pixel(40, 24, image::Rgba([200, 40, 90, 255])).save(&art).unwrap();
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        // with fonts, as a field without has no text to put the cursor in
        widgets::test_fonts(&harness);
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        let width = || egui_kittest::kittest::by().role(egui::accesskit::Role::SpinButton);
        let width = move || width().value("40 px");
        let at = harness.get(width()).rect();
        let typed = |harness: &mut egui_kittest::Harness<App>, text: &str| {
            harness.get(width()).focus();
            harness.run_steps(2);
            harness.event(egui::Event::Text(text.into()));
            harness.run_steps(2);
        };
        // typed, then put back: the crop as it was all along, and the
        // field where it was
        typed(&mut harness, "2");
        assert_eq!(harness.state().items[0].choices.crop, None);
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert_eq!(harness.state().items[0].choices.crop, None);
        assert_eq!(harness.get(width()).rect(), at);
        // typed, then converted by its key: with the width typed
        typed(&mut harness, "20");
        assert_eq!(harness.state().items[0].choices.crop, None);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        let made = item.made.as_ref().expect("converted");
        let crop = made.choices.as_ref().and_then(|choices| choices.crop);
        assert_eq!(crop.map(|crop| crop.width), Some(20));
        assert_eq!(item.choices.crop.map(|crop| crop.width), Some(20));
    }

    #[test]
    fn converts_by_its_keys_without_pressing_the_button_with_the_focus() {
        use crate::l10n::fl;
        use egui_kittest::kittest::Queryable;
        let dir = tempfile::tempdir().unwrap();
        let arts: Vec<_> = (0..2)
            .map(|n| {
                let art = dir.path().join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(arts);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        harness.state_mut().select(ids[0]);
        harness.run_steps(2);
        // Enter presses the button with the focus, whatever is held with it
        let (ctrl, both) =
            (egui::Modifiers::COMMAND, egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT));
        for (modifiers, converted) in [(ctrl, vec![true, false]), (both, vec![true, true])] {
            harness.get_by_label(&fl!("remove-all")).focus();
            harness.run_steps(2);
            assert!(harness.ctx.memory(|memory| memory.focused().is_some()));
            harness.key_press_modifiers(modifiers, egui::Key::Enter);
            harness.run_steps(2);
            finish(&mut harness);
            let app = harness.state();
            assert_eq!(app.items.len(), 2, "{modifiers:?} removed them");
            let made: Vec<bool> = app.items.iter().map(|item| item.made.is_some()).collect();
            assert_eq!(made, converted, "{modifiers:?}");
        }
    }

    #[test]
    fn leaves_keys_pressed_in_dialogs_to_them() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.key_press(egui::Key::Delete);
        harness.run_steps(2);
        assert_eq!(harness.state().items.len(), 1);
        // Escape closes the dialog, and the keys are shortcuts again
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!harness.state().settings_dialog.is_open());
        harness.key_press(egui::Key::Delete);
        harness.run_steps(2);
        assert!(harness.state().items.is_empty());
    }

    #[test]
    fn puts_files_removed_back_where_they_were() {
        use crate::l10n::fl;
        use egui_kittest::kittest::Queryable;
        use tgradish_core::options::Crop;
        let dir = tempfile::tempdir().unwrap();
        let arts: Vec<_> = (0..3)
            .map(|n| {
                let art = dir.path().join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        let mut harness = harness(Config::default());
        harness.state_mut().add(arts);
        finish(&mut harness);
        let crop = Some(Crop { x: 1, y: 1, width: 2, height: 2 });
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        harness.state_mut().items[1].choices.crop = crop;
        harness.state_mut().select(ids[1]);
        harness.run_steps(2);
        // the middle one, by its key, said with a way back
        harness.key_press(egui::Key::Delete);
        harness.run_steps(2);
        let order = |harness: &egui_kittest::Harness<App>| -> Vec<u64> {
            harness.state().items.iter().map(|item| item.id).collect()
        };
        assert_eq!(order(&harness), [ids[0], ids[2]]);
        harness.get_by_label(&fl!("files-removed", count = 1));
        harness.get_by_label(&fl!("undo-removing")).click();
        harness.run_steps(2);
        let app = harness.state();
        assert_eq!((order(&harness), app.items[1].choices.crop), (ids.clone(), crop));
        assert_eq!(app.selected, Some(ids[1]));
        assert!(app.banner.rows.is_empty() && app.removed.is_none());
        // one, then the rest: all of them back with Ctrl+Z
        harness.state_mut().remove(ids[0]);
        let rest = order(&harness);
        harness.state_mut().remove_all(&rest);
        harness.run_steps(2);
        assert!(harness.state().items.is_empty());
        harness.get_by_label(&fl!("files-removed", count = 3));
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        harness.run_steps(2);
        assert_eq!(order(&harness), ids);
        assert_eq!(harness.state().items[1].choices.crop, crop);
        // once the offer goes, so do they
        harness.state_mut().remove(ids[2]);
        harness.run_steps(2);
        harness.state_mut().banner.rows.clear();
        harness.run_steps(2);
        assert!(harness.state().removed.is_none());
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        harness.run_steps(2);
        assert_eq!(order(&harness), ids[..2]);
    }

    #[test]
    fn cuts_and_steps_by_frames() {
        let dir = tempfile::tempdir().unwrap();
        // a second at 25 fps
        let Some(video) = video_of(dir.path(), "64x48", 1) else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        {
            let item = &mut harness.state_mut().items[0];
            let frames = item.input_frames().unwrap();
            // as its packets time them
            assert_eq!((frames.count(), item.video.ready().unwrap().starts.len()), (25, 25));
            // frames 5 to 9, cut at the frames shown
            item.view.time = frames.times[5];
            Keys { start_here: true, ..Keys::default() }.apply(item);
            item.view.time = frames.times[9];
            Keys { end_here: true, ..Keys::default() }.apply(item);
            // steps stay within them
            Keys { by: 1, ..Keys::default() }.apply(item);
            assert_eq!(frames.at(item.view.time), 9);
            Keys { first: true, ..Keys::default() }.apply(item);
            assert_eq!(frames.at(item.view.time), 5);
            Keys { by: -10, ..Keys::default() }.apply(item);
            assert_eq!((frames.at(item.view.time), item.view.playing), (5, false));
            // its end and start moved, showing the frame they move to, and
            // back
            let part = |item: &Item| {
                frames.part(item.input_length().unwrap(), item.choices.start, item.choices.length)
            };
            Keys { move_end: 2, ..Keys::default() }.apply(item);
            assert_eq!((part(item), frames.at(item.view.time)), ((5, 12), 11));
            Keys { move_start: -10, ..Keys::default() }.apply(item);
            assert_eq!((part(item), frames.at(item.view.time)), ((0, 12), 0));
            Keys { move_end: -2, ..Keys::default() }.apply(item);
            Keys { move_start: 5, ..Keys::default() }.apply(item);
            assert_eq!(part(item), (5, 10));
            item.choices.preset = Some("fast".into());
            item.choices.webm.fit = Some(tgradish_core::options::Fit::Off);
        }
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        // converting keeps exactly those frames
        let made = made_job(&harness.state().items[0]);
        let Status::Done(done) = &made.status else { panic!("{:?}", made.status) };
        let info = tgradish_core::webm::container::inspect_file(&done.output).unwrap();
        assert_eq!(info.video_frames, 5);
    }
}

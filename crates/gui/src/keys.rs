//! What the keyboard, the mouse wheel, the clipboard and dropped files ask
//! of the window. Keys go to the window only while no text field or dialog
//! has them, so typing is never taken for a command.

use std::path::PathBuf;

use eframe::egui::{self, vec2};
use tgradish_core::presets::Format;

use crate::app::App;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::dnd;
use crate::item::{Item, Show};

impl App {
    /// Files dropped on the window, pasted, and keys.
    pub(crate) fn take_input(&mut self, ctx: &egui::Context) {
        let paste = ctx.input(|input| self.paste_key.read(&input.events));
        let (dropped, hovered, space, delete) = ctx.input(|input| {
            let dropped: Vec<PathBuf> = input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect();
            (
                dropped,
                !input.raw.hovered_files.is_empty(),
                input.key_pressed(egui::Key::Space),
                input.key_pressed(egui::Key::Delete),
            )
        });
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
        // typing into a text field is just typing, and keys pressed in a
        // dialog are for it, not the window behind
        if ctx.memory(|memory| memory.focused().is_some()) || self.dialog_open() {
            return;
        }
        if space
            && let Some(id) = self.selected
            && let Some(item) = self.item_mut(id)
        {
            item.view.playing = !item.view.playing;
        }
        let keys = ctx.input(|input| {
            let pressed = |key| input.key_pressed(key);
            let step = if input.modifiers.shift { 10 } else { 1 };
            let by = if pressed(egui::Key::ArrowLeft) {
                -step
            } else if pressed(egui::Key::ArrowRight) {
                step
            } else {
                0
            };
            // as the keys lie, left to right: with Ctrl the used part's
            // start moves, with Alt its end
            let (start, end) = (input.modifiers.command, input.modifiers.alt);
            Keys {
                by: if end || start { 0 } else { by },
                move_start: if start { by } else { 0 },
                move_end: if end && !start { by } else { 0 },
                first: pressed(egui::Key::Home),
                last: pressed(egui::Key::End),
                start_here: pressed(egui::Key::OpenBracket),
                end_here: pressed(egui::Key::CloseBracket),
            }
        });
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
        let mut paste = false;
        for event in events {
            match event {
                egui::Event::Paste(_) => {
                    paste = true;
                    self.read = true;
                }
                egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. } => {
                    self.plain = !modifiers.command;
                }
                // went down as Ctrl+V where egui-winit passed nothing on,
                // whether Ctrl is still held or let go first
                egui::Event::Key { key: egui::Key::V, pressed: false, .. } => {
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
        match item.view.show {
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
                    item.choices.frame = (at > 0).then_some(frames.times[at]);
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
        // Escape closes the dialog, and the window has its keys again
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!harness.state().settings_dialog.is_open());
        harness.key_press(egui::Key::Delete);
        harness.run_steps(2);
        assert!(harness.state().items.is_empty());
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

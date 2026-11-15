//! The GUI's scale as egui applies it, with the pointer kept where it
//! is on the screen as the scale changes.
//!
//! egui-winit says where the pointer is in points, at the scale of the
//! moment, and says that same place again with each press until the
//! pointer moves; egui keeps it as well. As the scale changes, the pointer
//! hasn't moved, so neither says anything new, and the place both keep is
//! where the pointer would be at the old scale: what is under the pointer
//! doesn't show it, and a button clicked again without moving the pointer
//! isn't where egui looks, as with Settings' − and +, kept under the
//! pointer. So each frame's input is said again at the scale egui reads it
//! at, and where the scale changes, where the pointer is with it.

use eframe::egui;

#[derive(Default)]
pub(crate) struct Scaling {
    /// The scale asked of egui, which it applies as the next frame starts.
    asked: Option<f32>,
    /// Where the pointer last moved, in points as egui-winit said it, and
    /// the scale it said it at.
    said: Option<(egui::Pos2, f32)>,
}

impl Scaling {
    /// Asks egui for `zoom` from the next frame, as
    /// [`egui::Context::set_zoom_factor`], which asks nothing of the scale
    /// it has.
    pub(crate) fn set(&mut self, ctx: &egui::Context, zoom: f32) {
        if zoom != ctx.zoom_factor() {
            self.asked = Some(zoom);
        }
        ctx.set_zoom_factor(zoom);
    }

    /// Takes the scale's keys as the shortcuts list them, Ctrl++
    /// (or Ctrl+=), Ctrl+- and Ctrl+0, as egui would at the end of the
    /// frame, which leaves them to the app, so the scale they ask for is
    /// known.
    pub(crate) fn keys(&mut self, ctx: &egui::Context) {
        use crate::keys::{Command, shortcut_of};
        if ctx.options(|options| options.zoom_with_keyboard) {
            ctx.options_mut(|options| options.zoom_with_keyboard = false);
        }
        let consume = |command| {
            let keys = shortcut_of(command).keys;
            ctx.input_mut(|input| keys.iter().any(|key| input.consume_shortcut(key)))
        };
        let zoom = f64::from(ctx.zoom_factor());
        let to = if consume(Command::ScaleReset) {
            Some(1.0)
        } else {
            let larger = consume(Command::ScaleUp);
            // both from the scale there is, Ctrl+- the later, as egui has it
            match (consume(Command::ScaleDown), larger) {
                (true, _) => Some(crate::config::scale_step(zoom, -1)),
                (false, true) => Some(crate::config::scale_step(zoom, 1)),
                (false, false) => None,
            }
        };
        if let Some(to) = to {
            self.set(ctx, to as f32);
        }
    }

    /// The frame has begun, egui applying the scale asked.
    pub(crate) fn begun(&mut self) {
        self.asked = None;
    }

    /// Says the frame's input, `input`, at the scale egui applies as the
    /// frame begins, from the scale egui-winit said it at, the last one
    /// applied; and where the scale changes, where the pointer is, though
    /// it hasn't moved.
    pub(crate) fn follow(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let said_at = ctx.zoom_factor();
        let zoom = self.asked.unwrap_or(said_at);
        let at = |pos: egui::Pos2, said_at: f32| {
            if said_at == zoom { pos } else { pos * (said_at / zoom) }
        };
        let kept = self.said;
        for event in &mut input.events {
            match event {
                egui::Event::PointerMoved(pos) => {
                    self.said = Some((*pos, said_at));
                    *pos = at(*pos, said_at);
                }
                // said again where the pointer last moved, at the scale
                // then, or somewhere of its own
                egui::Event::PointerButton { pos, .. } => match self.said {
                    Some((said, then)) if said == *pos => *pos = at(said, then),
                    _ => {
                        self.said = Some((*pos, said_at));
                        *pos = at(*pos, said_at);
                    }
                },
                egui::Event::Touch { pos, .. } => *pos = at(*pos, said_at),
                egui::Event::PointerGone => self.said = None,
                _ => {}
            }
        }
        // egui keeps the pointer where it was in points at the old scale
        if zoom != said_at
            && let Some((said, then)) = kept
        {
            input.events.insert(0, egui::Event::PointerMoved(at(said, then)));
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Key, Pos2, pos2};

    use crate::config::Config;
    use crate::testing::Screen;

    #[test]
    fn keeps_the_pointer_where_it_is_as_the_keys_scale_the_gui() {
        let mut screen = Screen::new(Config::default(), [1280.0, 900.0], false);
        let pixels = pos2(700.0, 400.0);
        screen.move_to(pixels);
        let at = |screen: &Screen| -> Option<Pos2> {
            let pointer = screen.pointer()?;
            Some(pointer * screen.harness.ctx.zoom_factor())
        };
        for (key, zoom) in
            [(Key::Plus, 1.1), (Key::Equals, 1.2), (Key::Minus, 1.1), (Key::Num0, 1.0)]
        {
            screen.command(key);
            // the scale applied a frame later, which hovering shows at once
            screen.frame(Vec::new());
            assert_eq!(screen.harness.ctx.zoom_factor(), zoom, "{key:?}");
            let now = at(&screen);
            assert!(now.is_some_and(|at| (at - pixels).length() < 0.01), "{key:?}: {now:?}");
            screen.frames(2);
        }
        // and saved, as the keys' scale is
        assert_eq!(screen.harness.state().config.gui.scale, 1.0);
        screen.command(Key::Plus);
        screen.frames(2);
        assert_eq!(screen.harness.state().config.gui.scale, 1.1);
    }

    #[test]
    fn says_the_pointer_again_at_the_scale_egui_reads_it_at() {
        use eframe::egui::{Context, Event, Modifiers, PointerButton, RawInput};
        let ctx = Context::default();
        let mut scaling = super::Scaling::default();
        let (button, modifiers) = (PointerButton::Primary, Modifiers::NONE);
        let press = |pos| Event::PointerButton { pos, button, pressed: true, modifiers };
        let frame = |scaling: &mut super::Scaling, events: Vec<Event>| {
            let mut input = RawInput { events, ..RawInput::default() };
            scaling.follow(&ctx, &mut input);
            ctx.run_ui(input.clone(), |_| {}).textures_delta.clear();
            scaling.begun();
            input.events
        };
        // as egui-winit says it
        assert_eq!(
            frame(&mut scaling, vec![Event::PointerMoved(pos2(10.0, 10.0))]),
            [Event::PointerMoved(pos2(10.0, 10.0))]
        );
        // twice the size, the pointer still: where it is, at once
        scaling.set(&ctx, 2.0);
        assert_eq!(frame(&mut scaling, Vec::new()), [Event::PointerMoved(pos2(5.0, 5.0))]);
        assert_eq!(ctx.zoom_factor(), 2.0);
        // a press where egui-winit last had it move, at the scale then
        assert_eq!(frame(&mut scaling, vec![press(pos2(10.0, 10.0))]), [press(pos2(5.0, 5.0))]);
        // a press somewhere of its own, as by touch, said at the scale now
        assert_eq!(frame(&mut scaling, vec![press(pos2(30.0, 40.0))]), [press(pos2(30.0, 40.0))]);
        // gone, it stays gone as the scale changes
        frame(&mut scaling, vec![Event::PointerGone]);
        scaling.set(&ctx, 1.0);
        assert_eq!(frame(&mut scaling, Vec::new()), []);
    }
}

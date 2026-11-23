//! Scroll areas in the GUI's style, with a reserved scroll-bar gutter and
//! content that keeps its height while a press inside is held.

use eframe::egui::{self, vec2};

use crate::theme;

/// Right-hand gutter width reserved for the scroll bar whether visible or not
/// so content width stays constant.
pub(crate) const GUTTER: f32 = 12.0;
/// Scroll bar width at the outer edge of [`GUTTER`], flush with adjacent
/// buttons while leaving the inner half of the gutter as a gap.
pub(crate) const SCROLL_BAR: f32 = 6.0;
/// Minimum scroll-bar handle length so long lists remain easy to grab.
const LEAST_HANDLE: f32 = 24.0;
/// Height of the top and bottom scroll-overflow fade gradients.
pub(crate) const SCROLL_FADE: f32 = 24.0;

/// Shows `area` with the standard scroll style: always-visible solid scroll bar
/// when content overflows, a permanently reserved [`GUTTER`], and top/bottom
/// overflow fade gradients.
pub(crate) fn scrolled<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let width = ui.available_width();
    let outer = ui.style().clone();
    ui.scope(|ui| {
        let mut style = egui::style::ScrollStyle::solid();
        style.bar_width = SCROLL_BAR;
        style.bar_inner_margin = GUTTER - SCROLL_BAR;
        style.bar_outer_margin = 0.0;
        style.handle_min_length = LEAST_HANDLE;
        style.fade = egui::style::ScrollFadeStyle { strength: 1.0, size: SCROLL_FADE };
        ui.spacing_mut().scroll = style;
        // egui paints the track in `extreme_bg_color` and the handle in a
        // widget's fill (here a checkbox's, the view colour, the same as the
        // track), so the handle would not show; override both.
        let colours = theme::scroll_bar(ui.visuals());
        let visuals = ui.visuals_mut();
        visuals.extreme_bg_color = colours.track;
        let round = egui::CornerRadius::same((SCROLL_BAR / 2.0) as u8);
        let widgets = &mut visuals.widgets;
        for (state, handle) in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active]
            .into_iter()
            .zip(colours.handle)
        {
            state.bg_fill = handle;
            state.corner_radius = round;
        }
        // Scope the temp key to this specific `Ui` because egui assigns the
        // same default ID to side-by-side scroll areas.
        let tallest = ui.unique_id().with("tallest while held");
        let shown = area.show(ui, |ui| {
            // Restore the outer widget style for content inside the scroll area.
            ui.set_style(outer);
            ui.set_max_width(width - GUTTER);
            let added = add(ui);
            // Prevent content height from shrinking relative to the immediately
            // preceding pass while a pointer press that started inside this
            // scroll area is held down: otherwise, when scrolled to the bottom,
            // holding a repeat button that shortens content below it would shift
            // the button out from under the pointer. Presses that started
            // elsewhere (such as dragging a panel divider) allow normal reflow.
            let pass = ui.ctx().cumulative_pass_nr();
            let before = ui.data(|data| data.get_temp::<(u64, f32)>(tallest));
            let here = tallest.with("pressed here");
            let (pressed, down, origin) = ui.input(|input| {
                let pointer = &input.pointer;
                (pointer.any_pressed(), pointer.any_down(), pointer.press_origin())
            });
            let held_here = if pressed {
                // Check against the viewport rect recorded on the previous frame.
                let seen = ui.data(|data| data.get_temp::<egui::Rect>(tallest.with("seen")));
                let layer = ui.layer_id();
                let on = origin.zip(seen).is_some_and(|(at, seen)| {
                    seen.contains(at) && ui.ctx().layer_id_at(at) == Some(layer)
                });
                ui.data_mut(|data| data.insert_temp(here, on));
                on
            } else {
                ui.data(|data| data.get_temp::<bool>(here)).unwrap_or(false)
            };
            if !down {
                ui.data_mut(|data| data.remove::<bool>(here));
            }
            if let Some((_, before)) = before.filter(|&(at, _)| at + 1 == pass)
                && down
                && held_here
            {
                // Expand from `min_rect().top()` rather than calling
                // `set_min_height`, which would add height after the cursor and
                // grow every frame.
                ui.expand_to_include_y(ui.min_rect().top() + before);
            }
            let height = ui.min_rect().height();
            ui.data_mut(|data| data.insert_temp(tallest, (pass, height)));
            added
        });
        // Record the visible viewport rect including the scroll-bar gutter.
        let seen = egui::Rect::from_min_size(
            shown.inner_rect.min,
            vec2(width.max(shown.inner_rect.width()), shown.inner_rect.height()),
        );
        ui.data_mut(|data| data.insert_temp(tallest.with("seen"), seen));
        shown
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::test_fonts;

    #[test]
    fn keeps_what_scrolls_as_long_only_as_it_was_the_frame_before() {
        // Line count, visibility flag, and resulting scroll content height.
        type State = (usize, bool, f32);
        let show = |ui: &mut egui::Ui, (lines, shown, tall): &mut State| {
            if !*shown {
                return;
            }
            let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            *tall = scrolled(ui, area, |ui| {
                for line in 0..*lines {
                    ui.label(format!("line {line}"));
                }
            })
            .content_size
            .y;
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([300.0, 400.0])
            .build_ui_state(show, (10, true, 0.0));
        test_fonts(&harness);
        harness.run_steps(2);
        let tall = harness.state().2;
        // Hide for several frames, hold the pointer down, then re-show with fewer lines.
        harness.state_mut().1 = false;
        let at = egui::pos2(150.0, 350.0);
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(3);
        *harness.state_mut() = (2, true, 0.0);
        harness.run_steps(3);
        assert!(harness.state().2 < tall / 2.0, "{} of {tall}", harness.state().2);
    }

    #[test]
    fn keeps_what_scrolls_as_long_while_the_pointer_is_held() {
        // A button followed by trailing lines, scrolled to the bottom.
        type State = (usize, egui::Rect);
        let show = |ui: &mut egui::Ui, (lines, at): &mut State| {
            ui.set_height(200.0);
            let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            scrolled(ui, area, |ui| {
                for line in 0..20 {
                    ui.label(format!("line {line}"));
                }
                *at = ui.button("held").rect;
                for line in 0..*lines {
                    ui.label(format!("after {line}"));
                }
            });
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([300.0, 220.0])
            .build_ui_state(show, (3, egui::Rect::NOTHING));
        test_fonts(&harness);
        harness.run();
        harness.event(egui::Event::PointerMoved(egui::pos2(150.0, 100.0)));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -2000.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.run_steps(10);
        let at = harness.state().1.center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        harness.event(egui::Event::PointerMoved(at));
        harness.event(press(true));
        harness.step();
        // Removing trailing lines while the pointer is held keeps the button under the pointer.
        harness.state_mut().0 = 1;
        harness.run_steps(3);
        assert!(harness.state().1.contains(at), "{:?} from {at:?}", harness.state().1);
        // Releasing the pointer lets the scroll area shrink to its new end.
        harness.event(press(false));
        harness.run_steps(3);
        assert!(!harness.state().1.contains(at));
    }

    #[test]
    fn keeps_what_scrolls_as_long_only_for_a_press_that_began_on_it() {
        // Two side-by-side scroll areas with a button in each and trailing
        // lines in the second.
        type State = (usize, [egui::Rect; 2], f32);
        let show = |ui: &mut egui::Ui, (lines, at, tall): &mut State| {
            ui.set_height(200.0);
            ui.columns(2, |columns| {
                let area = egui::ScrollArea::vertical().id_salt("one").auto_shrink([false; 2]);
                scrolled(&mut columns[0], area, |ui| {
                    at[0] = ui.button("elsewhere").rect;
                });
                let area = egui::ScrollArea::vertical().id_salt("two").auto_shrink([false; 2]);
                *tall = scrolled(&mut columns[1], area, |ui| {
                    at[1] = ui.button("here").rect;
                    for line in 0..*lines {
                        ui.label(format!("line {line}"));
                    }
                })
                .content_size
                .y;
            });
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([400.0, 220.0])
            .build_ui_state(show, (30, [egui::Rect::NOTHING; 2], 0.0));
        test_fonts(&harness);
        harness.run();
        let tall = harness.state().2;
        // Measure the second area's content height when shortened while holding
        // button `on`.
        let shortened = |harness: &mut egui_kittest::Harness<'_, State>, on: usize| {
            let at = harness.state().1[on].center();
            let press = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            harness.event(egui::Event::PointerMoved(at));
            harness.event(press(true));
            harness.step();
            harness.state_mut().0 = 5;
            harness.run_steps(3);
            let held = harness.state().2;
            harness.event(press(false));
            harness.state_mut().0 = 30;
            harness.run_steps(3);
            held
        };
        // Holding inside the second area preserves its height; holding in the
        // first area lets the second shrink immediately.
        assert_eq!(shortened(&mut harness, 1), tall);
        let held = shortened(&mut harness, 0);
        assert!(held < tall - 50.0, "{held} of {tall}");
    }

    #[test]
    fn keeps_room_for_the_scroll_bar_whether_it_shows_or_not() {
        // Track available content width and inner rect width as the scroll bar
        // appears and disappears.
        type State = (usize, Option<(f32, f32)>);
        let show = |ui: &mut egui::Ui, (lines, seen): &mut State| {
            let area = egui::ScrollArea::vertical().max_height(200.0).auto_shrink([false, true]);
            let shown = scrolled(ui, area, |ui| {
                for _ in 0..*lines {
                    ui.allocate_exact_size(vec2(40.0, 20.0), egui::Sense::hover());
                }
                ui.available_width()
            });
            *seen = Some((shown.inner, shown.inner_rect.width()));
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size([400.0, 300.0])
            .build_ui_state(show, (2, None));
        harness.run_steps(3);
        let (few, room) = harness.state().1.unwrap();
        harness.state_mut().0 = 40;
        harness.run_steps(30);
        let (many, narrower) = harness.state().1.unwrap();
        // Content width stays identical whether the scroll bar is drawn in the
        // reserved gutter or not.
        assert_eq!(few, many);
        assert!(narrower < room && few <= narrower, "{few} {narrower} {room}");
        assert_eq!(few, harness.ctx.content_rect().width() - 16.0 - GUTTER);
    }
}

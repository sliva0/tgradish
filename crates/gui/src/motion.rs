//! UI animation and reduced-motion detection. Smooth scrolling and transitions
//! follow `config.gui.motion` unless the OS requests reduced motion (via the
//! XDG desktop portal or GNOME's `enable-animations` on Linux, KDE Plasma's
//! `AnimationDurationFactor=0`, or Windows's `SPI_GETCLIENTAREAANIMATION`).
//! Everything that moves asks [`motion`], the one place this is decided.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;

/// How long egui's own easings take with motion, as hover and menus
/// fading in: a little shorter than egui's, as tgradish is a tool used
/// in quick turns.
const ANIMATION: f32 = 0.15;

/// Whether things move, a `bool` in egui's memory: [`set`] by `App` as
/// each frame starts, from the settings and the system, and read by
/// whatever moves with [`motion`], wherever it is drawn.
fn id() -> egui::Id {
    egui::Id::new("motion")
}

/// Whether what changes moves to where it goes, rather than being there
/// at once, as set for this frame; on where nothing set it, as for a part
/// of the GUI drawn alone.
pub(crate) fn motion(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp(id())).unwrap_or(true)
}

/// Sets whether things move from this frame on, egui's own easings and
/// scrolling to a place among them.
pub(crate) fn set(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|data| data.insert_temp(id(), on));
    let time = if on { ANIMATION } else { 0.0 };
    if ctx.global_style().animation_time != time {
        ctx.all_styles_mut(|style| {
            style.animation_time = time;
            style.scroll_animation = if on {
                egui::style::ScrollAnimation::default()
            } else {
                egui::style::ScrollAnimation::none()
            };
        });
    }
}

/// How far a change begun at `since`, taking `duration` seconds, is along
/// its way, from 0 to 1, drawn again until it is done; done at once
/// without motion.
pub(crate) fn along(ctx: &egui::Context, since: f64, duration: f64) -> f32 {
    if !motion(ctx) {
        return 1.0;
    }
    let share = ((ctx.input(|input| input.time) - since) / duration).clamp(0.0, 1.0);
    if share < 1.0 {
        ctx.request_repaint();
    }
    share as f32
}

/// A value eased toward `target` as time passes, kept by `id`, a share `1 - 1/e`
/// of the way in each `time_constant`: what was shown last is kept, so
/// each frame goes on from it. At the target at once without motion, or
/// where `jump` says. Drawn again until it is there.
pub(crate) fn approach(
    ctx: &egui::Context,
    id: egui::Id,
    target: f32,
    time_constant: f32,
    jump: impl FnOnce(f32) -> bool,
) -> f32 {
    let now = ctx.input(|input| input.time);
    let last: Option<(f32, f64)> = ctx.data(|data| data.get_temp(id));
    let shown = match last {
        Some((shown, at)) if motion(ctx) && !jump(shown) => {
            let gone = (now - at).max(0.0) as f32;
            let eased = target + (shown - target) * (-gone / time_constant).exp();
            // the last thousandth is no change to see
            if (eased - target).abs() < 0.001 { target } else { eased }
        }
        _ => target,
    };
    ctx.data_mut(|data| data.insert_temp(id, (shown, now)));
    if shown != target {
        ctx.request_repaint();
    }
    shown
}

/// Whether the system asks for less motion, read on a thread, as the
/// portal may take a while to answer, and followed as it changes.
pub(crate) struct System {
    reduced: Arc<AtomicBool>,
}

impl System {
    /// Starts reading it; `ctx` repaints when it changes.
    pub(crate) fn watch(ctx: &egui::Context) -> System {
        let reduced = Arc::new(AtomicBool::new(false));
        let shared = reduced.clone();
        let ctx = ctx.clone();
        let set = move |less: bool| {
            if shared.swap(less, Ordering::Relaxed) != less {
                ctx.request_repaint();
            }
        };
        // without a thread, things move as the setting says
        let _ = std::thread::Builder::new().name("motion".into()).spawn(move || follow(set));
        System { reduced }
    }

    pub(crate) fn reduced(&self) -> bool {
        self.reduced.load(Ordering::Relaxed)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
use portal::follow;

/// The desktop portal's settings, through libdbus.
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
mod portal {
    use std::ffi::{CStr, c_int};

    use crate::dbus::{Connection, Dbus, Error, Iter, Message, SESSION, STRING, VARIANT};

    const DESTINATION: &CStr = c"org.freedesktop.portal.Desktop";
    const PATH: &CStr = c"/org/freedesktop/portal/desktop";
    const INTERFACE: &CStr = c"org.freedesktop.portal.Settings";
    const BOOLEAN: c_int = b'b' as c_int;
    const UINT32: c_int = b'u' as c_int;

    /// Reads whether less motion is asked for, then follows the portal's
    /// changes, and KDE's to its settings, on a connection of its own,
    /// until the session bus goes away. Without the bus, KDE's settings are
    /// read once.
    pub(super) fn follow(set: impl Fn(bool)) {
        let Some(dbus) = Dbus::load() else { return set(kde()) };
        // SAFETY: the connection, and each message until it is unreferenced,
        // stay valid, and only this thread uses them
        unsafe {
            let mut error = Error([0; 8]);
            (dbus.dbus_error_init)(&mut error);
            let connection = (dbus.dbus_bus_get_private)(SESSION, &mut error);
            (dbus.dbus_error_free)(&mut error);
            if connection.is_null() {
                return set(kde());
            }
            // libdbus ends the process when the bus goes away, unless told
            (dbus.dbus_connection_set_exit_on_disconnect)(connection, 0);
            // listening first, so a change while reading isn't missed
            for rule in [
                c"type='signal',interface='org.freedesktop.portal.Settings',member='SettingChanged'",
                c"type='signal',interface='org.kde.kconfig.notify',member='ConfigChanged'",
            ] {
                (dbus.dbus_bus_add_match)(connection, rule.as_ptr(), &mut error);
                (dbus.dbus_error_free)(&mut error);
            }
            set(reduced(&dbus, connection));
            loop {
                let mut changed = false;
                loop {
                    let message = (dbus.dbus_connection_pop_message)(connection);
                    if message.is_null() {
                        break;
                    }
                    changed |= signal(&dbus, message);
                    (dbus.dbus_message_unref)(message);
                }
                // read again whole, as a change of one may stand for both
                if changed {
                    set(reduced(&dbus, connection));
                }
                if (dbus.dbus_connection_read_write)(connection, -1) == 0 {
                    break;
                }
            }
            (dbus.dbus_connection_close)(connection);
            (dbus.dbus_connection_unref)(connection);
        }
    }

    /// Whether `message` says a setting changed, of the portal's or KDE's.
    unsafe fn signal(dbus: &Dbus, message: *mut Message) -> bool {
        let is = |interface: &CStr, member: &CStr| {
            // SAFETY: `message` is valid while this reads it
            unsafe {
                (dbus.dbus_message_is_signal)(message, interface.as_ptr(), member.as_ptr()) != 0
            }
        };
        is(INTERFACE, c"SettingChanged") || is(c"org.kde.kconfig.notify", c"ConfigChanged")
    }

    /// Whether the portal, or KDE, asks for less motion: the portal's own
    /// setting for it, from 1.20 on, or GNOME's switch of animations, which
    /// its portal passes on, or KDE's speed of them.
    unsafe fn reduced(dbus: &Dbus, connection: *mut Connection) -> bool {
        // SAFETY: as in `follow`
        unsafe {
            let less = read(dbus, connection, c"org.freedesktop.appearance", c"reduced-motion");
            let animations =
                read(dbus, connection, c"org.gnome.desktop.interface", c"enable-animations");
            less == Some(1) || animations == Some(0) || kde()
        }
    }

    /// The portal's setting `key` of `namespace`, a boolean or a number:
    /// with ReadOne, or Read, which portals before 1.15 only have, and
    /// which wraps the value once more.
    unsafe fn read(
        dbus: &Dbus,
        connection: *mut Connection,
        namespace: &CStr,
        key: &CStr,
    ) -> Option<u32> {
        for method in [c"ReadOne", c"Read"] {
            // SAFETY: as in `follow`; the strings appended outlive the call
            unsafe {
                let call = (dbus.dbus_message_new_method_call)(
                    DESTINATION.as_ptr(),
                    PATH.as_ptr(),
                    INTERFACE.as_ptr(),
                    method.as_ptr(),
                );
                if call.is_null() {
                    return None;
                }
                let mut args = Iter([0; 16]);
                (dbus.dbus_message_iter_init_append)(call, &mut args);
                for text in [namespace, key] {
                    let pointer = text.as_ptr();
                    (dbus.dbus_message_iter_append_basic)(
                        &mut args,
                        STRING,
                        (&raw const pointer).cast(),
                    );
                }
                let mut error = Error([0; 8]);
                (dbus.dbus_error_init)(&mut error);
                let reply = (dbus.dbus_connection_send_with_reply_and_block)(
                    connection, call, 5000, &mut error,
                );
                (dbus.dbus_error_free)(&mut error);
                (dbus.dbus_message_unref)(call);
                if reply.is_null() {
                    continue;
                }
                let mut iter = Iter([0; 16]);
                let value = match (dbus.dbus_message_iter_init)(reply, &mut iter) {
                    0 => None,
                    _ => number(dbus, &mut iter),
                };
                (dbus.dbus_message_unref)(reply);
                return value;
            }
        }
        None
    }

    /// The boolean or 32-bit number at `iter`, inside variants.
    pub(super) unsafe fn number(dbus: &Dbus, iter: &mut Iter) -> Option<u32> {
        // SAFETY: `iter` is at an argument of a valid message, and a basic
        // value is at most 8 bytes, which `value` has room for
        unsafe {
            let mut at = *iter;
            loop {
                match (dbus.dbus_message_iter_get_arg_type)(&mut at) {
                    VARIANT => {
                        let mut inside = Iter([0; 16]);
                        (dbus.dbus_message_iter_recurse)(&mut at, &mut inside);
                        at = inside;
                    }
                    BOOLEAN | UINT32 => break,
                    _ => return None,
                }
            }
            let mut value = [0u8; 8];
            (dbus.dbus_message_iter_get_basic)(&mut at, value.as_mut_ptr().cast());
            Some(u32::from_ne_bytes([value[0], value[1], value[2], value[3]]))
        }
    }

    /// Whether KDE's animations are set to instant, in a Plasma session.
    fn kde() -> bool {
        crate::accent::kde::plasma() && super::instant(&crate::accent::kde::files())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_booleans_and_numbers_in_variants() {
            let Some(dbus) = Dbus::load() else {
                eprintln!("skipped: needs libdbus");
                return;
            };
            // a signal's arguments, as a reply's are read
            let read = |kind: c_int, signature: &CStr, value: u32| unsafe {
                let message = (dbus.dbus_message_new_signal)(
                    PATH.as_ptr(),
                    INTERFACE.as_ptr(),
                    c"SettingChanged".as_ptr(),
                );
                let mut args = Iter([0; 16]);
                (dbus.dbus_message_iter_init_append)(message, &mut args);
                let mut inside = Iter([0; 16]);
                (dbus.dbus_message_iter_open_container)(
                    &mut args,
                    VARIANT,
                    signature.as_ptr(),
                    &mut inside,
                );
                (dbus.dbus_message_iter_append_basic)(&mut inside, kind, (&raw const value).cast());
                (dbus.dbus_message_iter_close_container)(&mut args, &mut inside);
                let mut iter = Iter([0; 16]);
                (dbus.dbus_message_iter_init)(message, &mut iter);
                let number = number(&dbus, &mut iter);
                (dbus.dbus_message_unref)(message);
                number
            };
            assert_eq!(read(BOOLEAN, c"b", 0), Some(0));
            assert_eq!(read(BOOLEAN, c"b", 1), Some(1));
            assert_eq!(read(UINT32, c"u", 1), Some(1));
        }
    }
}

/// Whether KDE's settings, `files` the user's first, set its animations to
/// instant.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn instant(files: &[String]) -> bool {
    let factor = files
        .iter()
        .find_map(|text| crate::accent::ini_value(text, "KDE", "AnimationDurationFactor"));
    factor.and_then(|factor| factor.parse::<f64>().ok()).is_some_and(|factor| factor <= 0.0)
}

/// Reads it every few seconds: Windows says when it changes only in
/// window messages, which winit doesn't pass on.
#[cfg(windows)]
fn follow(set: impl Fn(bool)) {
    loop {
        set(!client_animation());
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

/// "Show animations in Windows": on, where it can't be read.
#[cfg(windows)]
#[allow(unsafe_code)]
fn client_animation() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut on: windows_sys::core::BOOL = 1;
    // SAFETY: the action fills in a BOOL, which `on` is
    let read =
        unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut on).cast(), 0) };
    read == 0 || on != 0
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn follow(_set: impl Fn(bool)) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_kdes_instant_animations_as_less_motion() {
        let file = |factor: &str| {
            format!(
                "[General]\nAnimationDurationFactor=1\n[KDE]\nAnimationDurationFactor={factor}\n"
            )
        };
        assert!(instant(&[file("0")]));
        assert!(!instant(&[file("0.5")]));
        // the user's first
        assert!(!instant(&[file("1"), file("0")]));
        assert!(!instant(&["[KDE]\nSingleClick=false\n".to_owned()]));
    }

    #[test]
    fn moves_as_the_setting_says_in_one_place() {
        use crate::config::Config;
        let mut harness = crate::testing::harness(Config::default());
        harness.run_steps(2);
        let style = |harness: &egui_kittest::Harness<crate::app::App>| {
            let style = harness.ctx.global_style();
            (style.animation_time, style.scroll_animation)
        };
        assert!(motion(&harness.ctx));
        assert_eq!(style(&harness), (ANIMATION, egui::style::ScrollAnimation::default()));
        harness.state_mut().config.gui.motion = false;
        harness.step();
        assert!(!motion(&harness.ctx));
        assert_eq!(style(&harness), (0.0, egui::style::ScrollAnimation::none()));
        // a change under way is at its end at once
        let now = harness.ctx.input(|input| input.time);
        assert_eq!(along(&harness.ctx, now, 1.0), 1.0);
        harness.state_mut().config.gui.motion = true;
        harness.step();
        assert_eq!(style(&harness).0, ANIMATION);
        let now = harness.ctx.input(|input| input.time);
        assert_eq!(along(&harness.ctx, now - 0.5, 1.0), 0.5);
    }

    #[test]
    fn puts_the_small_picture_in_place_at_once_without_motion() {
        use crate::config::Corner;
        use egui::{Rect, pos2, vec2};
        let rect = |x: f32| Rect::from_min_size(pos2(x, 0.0), vec2(10.0, 10.0));
        let (before, after) = ((rect(0.0), rect(1.0)), (rect(2.0), rect(3.0)));
        for on in [true, false] {
            let ctx = egui::Context::default();
            set(&ctx, on);
            let mut inset = crate::inset::Inset::default();
            inset.ease(&ctx, 1, (true, 2, Corner::BottomRight), before);
            // another size: from where it was with motion, there at once without
            let (input, room, _) = inset.ease(&ctx, 1, (true, 3, Corner::BottomRight), after);
            assert_eq!((input, room) == after, !on);
        }
    }
}

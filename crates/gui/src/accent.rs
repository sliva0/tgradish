//! The system's accent colour, where it has one: on Linux from the desktop
//! portal, which Plasma 6, GNOME 47 and later and others answer, and in a
//! Plasma session without one there, as Plasma 5's, from KDE's
//! `kdeglobals`; on Windows from the registry; on macOS from AppKit. Read
//! on a thread, as the portal may take a while to start, and followed as it
//! changes.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};

pub(crate) struct Accent {
    colour: Arc<Mutex<Option<Color32>>>,
}

impl Accent {
    /// Starts reading the accent; `ctx` repaints when it arrives or changes.
    pub(crate) fn watch(ctx: &egui::Context) -> Accent {
        let colour = Arc::new(Mutex::new(None));
        let shared = colour.clone();
        let ctx = ctx.clone();
        let set = move |accent: Option<Color32>| {
            let mut colour = shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if *colour != accent {
                *colour = accent;
                ctx.request_repaint();
            }
        };
        // nothing to do without one: the window keeps Breeze's
        let _ = std::thread::Builder::new().name("accent".into()).spawn(move || follow(set));
        Accent { colour }
    }

    pub(crate) fn get(&self) -> Option<Color32> {
        *self.colour.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
use portal::follow;

/// The desktop portal's accent, through libdbus.
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
mod portal {
    use std::ffi::{CStr, c_char};

    use eframe::egui::Color32;

    use crate::dbus::{
        Connection, DOUBLE, Dbus, Error, Iter, Message, SESSION, STRING, STRUCT, VARIANT,
    };

    const DESTINATION: &CStr = c"org.freedesktop.portal.Desktop";
    const PATH: &CStr = c"/org/freedesktop/portal/desktop";
    const INTERFACE: &CStr = c"org.freedesktop.portal.Settings";
    const NAMESPACE: &CStr = c"org.freedesktop.appearance";
    const KEY: &CStr = c"accent-color";

    /// Reads the accent, then follows the portal's changes to it, and
    /// KDE's to its settings, on a connection of its own, until the session
    /// bus goes away. Without the bus, KDE's settings are read once.
    pub(super) fn follow(set: impl Fn(Option<Color32>)) {
        let Some(dbus) = Dbus::load() else { return set(super::kde::accent()) };
        // SAFETY: the connection, and each message until it is unreferenced,
        // stay valid, and only this thread uses them
        unsafe {
            let mut error = Error([0; 8]);
            (dbus.dbus_error_init)(&mut error);
            let connection = (dbus.dbus_bus_get_private)(SESSION, &mut error);
            (dbus.dbus_error_free)(&mut error);
            if connection.is_null() {
                return set(super::kde::accent());
            }
            // libdbus ends the process when the bus goes away, unless told
            (dbus.dbus_connection_set_exit_on_disconnect)(connection, 0);
            // listening first, so a change while reading isn't missed: to
            // the portal, and to KDE saying its settings changed, as
            // Frameworks 5 and 6 do, and Plasma 5's colour settings
            for rule in [
                c"type='signal',interface='org.freedesktop.portal.Settings',member='SettingChanged'",
                c"type='signal',interface='org.kde.kconfig.notify',member='ConfigChanged'",
                c"type='signal',interface='org.kde.KGlobalSettings',member='notifyChange'",
            ] {
                (dbus.dbus_bus_add_match)(connection, rule.as_ptr(), &mut error);
                (dbus.dbus_error_free)(&mut error);
            }
            // the portal's, or else KDE's
            let mut portal = read(&dbus, connection);
            set(portal.or_else(super::kde::accent));
            loop {
                // what came while reading, then while waiting
                loop {
                    let message = (dbus.dbus_connection_pop_message)(connection);
                    if message.is_null() {
                        break;
                    }
                    let is = |interface: &CStr, member: &CStr| {
                        (dbus.dbus_message_is_signal)(message, interface.as_ptr(), member.as_ptr())
                            != 0
                    };
                    if is(INTERFACE, c"SettingChanged")
                        && let Some(accent) = changed(&dbus, message)
                    {
                        portal = accent;
                        set(portal.or_else(super::kde::accent));
                    } else if portal.is_none()
                        && (is(c"org.kde.kconfig.notify", c"ConfigChanged")
                            || is(c"org.kde.KGlobalSettings", c"notifyChange"))
                    {
                        set(super::kde::accent());
                    }
                    (dbus.dbus_message_unref)(message);
                }
                if (dbus.dbus_connection_read_write)(connection, -1) == 0 {
                    break;
                }
            }
            (dbus.dbus_connection_close)(connection);
            (dbus.dbus_connection_unref)(connection);
        }
    }

    /// Asks the portal for the accent: with ReadOne, or Read, which portals
    /// before 1.15 only have, and which wraps the value once more.
    unsafe fn read(dbus: &Dbus, connection: *mut Connection) -> Option<Color32> {
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
                for text in [NAMESPACE, KEY] {
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
                let accent = match (dbus.dbus_message_iter_init)(reply, &mut iter) {
                    0 => None,
                    _ => colour(dbus, &mut iter),
                };
                (dbus.dbus_message_unref)(reply);
                return accent;
            }
        }
        None
    }

    /// The accent a SettingChanged signal brings, if it is about the
    /// accent: the namespace, the key, and the value in a variant.
    unsafe fn changed(dbus: &Dbus, message: *mut Message) -> Option<Option<Color32>> {
        // SAFETY: `message` is valid while this reads it
        unsafe {
            let mut iter = Iter([0; 16]);
            if (dbus.dbus_message_iter_init)(message, &mut iter) == 0 {
                return None;
            }
            for expected in [NAMESPACE, KEY] {
                if (dbus.dbus_message_iter_get_arg_type)(&mut iter) != STRING {
                    return None;
                }
                let mut text: *const c_char = std::ptr::null();
                (dbus.dbus_message_iter_get_basic)(&mut iter, (&raw mut text).cast());
                if CStr::from_ptr(text) != expected || (dbus.dbus_message_iter_next)(&mut iter) == 0
                {
                    return None;
                }
            }
            Some(colour(dbus, &mut iter))
        }
    }

    /// The accent at `iter`: red, green and blue from 0 to 1 in a struct,
    /// inside variants, and out of that range when there is none.
    unsafe fn colour(dbus: &Dbus, iter: &mut Iter) -> Option<Color32> {
        // SAFETY: `iter` is at an argument of a valid message
        unsafe {
            let mut at = *iter;
            loop {
                match (dbus.dbus_message_iter_get_arg_type)(&mut at) {
                    VARIANT => {
                        let mut inside = Iter([0; 16]);
                        (dbus.dbus_message_iter_recurse)(&mut at, &mut inside);
                        at = inside;
                    }
                    STRUCT => break,
                    _ => return None,
                }
            }
            let mut fields = Iter([0; 16]);
            (dbus.dbus_message_iter_recurse)(&mut at, &mut fields);
            let mut rgb = [0u8; 3];
            for (index, channel) in rgb.iter_mut().enumerate() {
                if index > 0 && (dbus.dbus_message_iter_next)(&mut fields) == 0 {
                    return None;
                }
                if (dbus.dbus_message_iter_get_arg_type)(&mut fields) != DOUBLE {
                    return None;
                }
                let mut value = 0f64;
                (dbus.dbus_message_iter_get_basic)(&mut fields, (&raw mut value).cast());
                if !(0.0..=1.0).contains(&value) {
                    return None;
                }
                *channel = (value * 255.0).round() as u8;
            }
            Some(Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A SettingChanged signal as the portal sends it, `rgb` in
        /// `wraps` variants.
        unsafe fn signal(dbus: &Dbus, key: &CStr, rgb: [f64; 3], wraps: usize) -> *mut Message {
            unsafe {
                let signal = (dbus.dbus_message_new_signal)(
                    PATH.as_ptr(),
                    INTERFACE.as_ptr(),
                    c"SettingChanged".as_ptr(),
                );
                let mut args = Iter([0; 16]);
                (dbus.dbus_message_iter_init_append)(signal, &mut args);
                for text in [NAMESPACE, key] {
                    let pointer = text.as_ptr();
                    (dbus.dbus_message_iter_append_basic)(
                        &mut args,
                        STRING,
                        (&raw const pointer).cast(),
                    );
                }
                let mut open = vec![args];
                for wrap in 1..=wraps {
                    // a variant of the struct, or of the variant inside
                    let signature = if wrap == wraps { c"(ddd)" } else { c"v" };
                    let mut inside = Iter([0; 16]);
                    let at = open.last_mut().unwrap();
                    (dbus.dbus_message_iter_open_container)(
                        at,
                        VARIANT,
                        signature.as_ptr(),
                        &mut inside,
                    );
                    open.push(inside);
                }
                let mut fields = Iter([0; 16]);
                let at = open.last_mut().unwrap();
                (dbus.dbus_message_iter_open_container)(at, STRUCT, std::ptr::null(), &mut fields);
                for value in rgb {
                    (dbus.dbus_message_iter_append_basic)(
                        &mut fields,
                        DOUBLE,
                        (&raw const value).cast(),
                    );
                }
                (dbus.dbus_message_iter_close_container)(at, &mut fields);
                while open.len() > 1 {
                    let mut inside = open.pop().unwrap();
                    (dbus.dbus_message_iter_close_container)(open.last_mut().unwrap(), &mut inside);
                }
                signal
            }
        }

        #[test]
        fn reads_the_portal_accent() {
            let Some(dbus) = Dbus::load() else {
                eprintln!("skipped: needs libdbus");
                return;
            };
            let read = |key: &CStr, rgb: [f64; 3], wraps: usize| unsafe {
                let message = signal(&dbus, key, rgb, wraps);
                let accent = changed(&dbus, message);
                (dbus.dbus_message_unref)(message);
                accent
            };
            assert_eq!(read(KEY, [1.0, 0.5, 0.0], 1), Some(Some(Color32::from_rgb(255, 128, 0))));
            // wrapped once more, as Read answers
            assert_eq!(read(KEY, [0.0, 0.0, 1.0], 2), Some(Some(Color32::from_rgb(0, 0, 255))));
            // out of range: no accent set
            assert_eq!(read(KEY, [-1.0, -1.0, -1.0], 1), Some(None));
            // another setting
            assert_eq!(read(c"color-scheme", [1.0, 1.0, 1.0], 1), None);
        }
    }
}

/// KDE's accent, from `kdeglobals`, which the portal answers with in Plasma
/// 6, but not in Plasma 5.
#[cfg(all(unix, not(target_os = "macos")))]
mod kde {
    use std::path::PathBuf;

    use eframe::egui::Color32;

    /// The accent of a Plasma session: the one chosen, or where it follows
    /// the colour scheme, the scheme's selection colour. Other desktops may
    /// have left a `kdeglobals` behind, which isn't theirs to follow.
    pub(super) fn accent() -> Option<Color32> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let plasma = desktop.split(':').any(|name| name.eq_ignore_ascii_case("KDE"))
            || std::env::var_os("KDE_FULL_SESSION").is_some_and(|value| value == "true");
        if !plasma {
            return None;
        }
        let files: Vec<String> =
            paths().iter().filter_map(|path| std::fs::read_to_string(path).ok()).collect();
        of(&files)
    }

    /// The accent `files` say, the user's first, as KDE reads them: each
    /// key from the first of them that has it.
    pub(super) fn of(files: &[String]) -> Option<Color32> {
        let first = |section: &str, key: &str| {
            files.iter().find_map(|text| value(text, section, key)).and_then(colour)
        };
        first("General", "AccentColor").or_else(|| first("Colors:Selection", "BackgroundNormal"))
    }

    /// `kdeglobals` in the user's settings, then in the system's.
    fn paths() -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let user = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| home.map(|home| home.join(".config")));
        let system = std::env::var("XDG_CONFIG_DIRS")
            .ok()
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or_else(|| "/etc/xdg".to_owned());
        let system = system.split(':').filter(|dir| dir.starts_with('/')).map(PathBuf::from);
        user.into_iter().chain(system).map(|dir| dir.join("kdeglobals")).collect()
    }

    /// The value of `key` in `[section]` of an INI file, as KConfig writes
    /// them.
    fn value<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
        let mut inside = false;
        for line in text.lines().map(str::trim) {
            if let Some(name) = line.strip_prefix('[') {
                inside = name.strip_suffix(']') == Some(section);
            } else if inside
                && let Some((name, value)) = line.split_once('=')
                && name.trim() == key
            {
                return Some(value.trim());
            }
        }
        None
    }

    /// A colour as KConfig writes one: `r,g,b`, maybe with alpha, or
    /// `#rrggbb`.
    fn colour(value: &str) -> Option<Color32> {
        if let Some(digits) = value.strip_prefix('#') {
            if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let rgb = u32::from_str_radix(digits, 16).ok()?;
            let [_, red, green, blue] = rgb.to_be_bytes();
            return Some(Color32::from_rgb(red, green, blue));
        }
        let channels: Vec<u8> =
            value.split(',').map(|part| part.trim().parse().ok()).collect::<Option<_>>()?;
        match channels[..] {
            [red, green, blue] | [red, green, blue, _] => Some(Color32::from_rgb(red, green, blue)),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_kdes_accent() {
            let scheme = "[Colors:Selection]\nBackgroundAlternate=30,87,116\n\
                          BackgroundNormal=61,174,233\n[General]\nColorScheme=BreezeDark\n";
            let chosen = "[General]\nAccentColor=233,100,58\n";
            let read =
                |files: &[&str]| of(&files.iter().map(|text| text.to_string()).collect::<Vec<_>>());
            // following the scheme: its selection colour
            assert_eq!(read(&[scheme]), Some(Color32::from_rgb(61, 174, 233)));
            // chosen, in the user's file or the system's
            assert_eq!(read(&[scheme, chosen]), Some(Color32::from_rgb(233, 100, 58)));
            assert_eq!(
                read(&["[General]\nAccentColor = #00d3b8\n"]),
                Some(Color32::from_rgb(0, 211, 184))
            );
            // the user's first
            let other = "[General]\nAccentColor=1,2,3,255\n";
            assert_eq!(read(&[other, chosen]), Some(Color32::from_rgb(1, 2, 3)));
            // none, or none that reads
            assert_eq!(read(&[]), None);
            assert_eq!(read(&["[Other]\nAccentColor=1,2,3\n"]), None);
            assert_eq!(read(&["[General]\nAccentColor=1,2,300\n"]), None);
        }
    }
}

/// Reads the accent every few seconds: Windows says when it changes only
/// in window messages, which winit doesn't pass on.
#[cfg(windows)]
fn follow(set: impl Fn(Option<Color32>)) {
    loop {
        set(registry_accent());
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

/// `AccentColor` of the window manager, a DWORD of 0xAABBGGRR.
#[cfg(windows)]
#[allow(unsafe_code)]
fn registry_accent() -> Option<Color32> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (wide("Software\\Microsoft\\Windows\\DWM"), wide("AccentColor"));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both names end in a NUL, and `data` has room for the DWORD
    // that RRF_RT_REG_DWORD allows
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &mut size,
        )
    };
    let [red, green, blue, _] = data.to_le_bytes();
    (status == 0).then(|| Color32::from_rgb(red, green, blue))
}

/// Reads the accent every few seconds: AppKit says when it changes only
/// on the main thread's run loop, which winit keeps to itself.
#[cfg(target_os = "macos")]
fn follow(set: impl Fn(Option<Color32>)) {
    loop {
        set(objc2::rc::autoreleasepool(|_| control_accent()));
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

/// AppKit's `controlAccentColor`, in sRGB.
#[cfg(target_os = "macos")]
fn control_accent() -> Option<Color32> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    let accent =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Color32::from_rgb(
        channel(accent.redComponent()),
        channel(accent.greenComponent()),
        channel(accent.blueComponent()),
    ))
}

#[cfg(not(any(windows, unix)))]
fn follow(_set: impl Fn(Option<Color32>)) {}

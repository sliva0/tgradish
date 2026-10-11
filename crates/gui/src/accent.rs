//! The system's accent colour, where it has one: from the desktop portal on
//! Linux, which Plasma 6 and GNOME 47 and later answer, and from the
//! registry on Windows. Read on a thread, as the portal may take a while to
//! start, and followed as it changes.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};

pub struct Accent {
    colour: Arc<Mutex<Option<Color32>>>,
}

impl Accent {
    /// Starts reading the accent; `ctx` repaints when it arrives or changes.
    pub fn watch(ctx: &egui::Context) -> Accent {
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

    pub fn get(&self) -> Option<Color32> {
        *self.colour.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
use portal::follow;

/// The desktop portal's accent, through libdbus: the library file dialogs
/// load too, loaded the same way when it is needed, so tgradish starts
/// without it.
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
mod portal {
    use std::ffi::{CStr, c_char, c_int, c_void};

    use eframe::egui::Color32;

    type Connection = c_void;
    type Message = c_void;

    /// libdbus's DBusError and DBusMessageIter, which callers allocate:
    /// opaque here, and larger than libdbus's own on any machine, as
    /// aligned.
    #[repr(C)]
    struct Error([u64; 8]);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Iter([u64; 16]);

    const SESSION: c_int = 0;
    const STRING: c_int = b's' as c_int;
    const VARIANT: c_int = b'v' as c_int;
    const STRUCT: c_int = b'r' as c_int;
    const DOUBLE: c_int = b'd' as c_int;

    const DESTINATION: &CStr = c"org.freedesktop.portal.Desktop";
    const PATH: &CStr = c"/org/freedesktop/portal/desktop";
    const INTERFACE: &CStr = c"org.freedesktop.portal.Settings";
    const NAMESPACE: &CStr = c"org.freedesktop.appearance";
    const KEY: &CStr = c"accent-color";

    macro_rules! functions {
        ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
            /// The functions of libdbus used here, the last three only to
            /// test with.
            #[cfg_attr(not(test), allow(dead_code))]
            struct Dbus {
                $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
                _library: libloading::Library,
            }

            impl Dbus {
                fn load() -> Option<Dbus> {
                    // SAFETY: loading libdbus runs no code of its own, and
                    // each function has the type its header declares
                    unsafe {
                        let library = libloading::Library::new("libdbus-1.so.3").ok()?;
                        $(let $name = *library.get(concat!(stringify!($name), "\0").as_bytes()).ok()?;)*
                        Some(Dbus { $($name,)* _library: library })
                    }
                }
            }
        };
    }

    functions! {
        dbus_error_init: fn(*mut Error);
        dbus_error_free: fn(*mut Error);
        dbus_bus_get_private: fn(c_int, *mut Error) -> *mut Connection;
        dbus_bus_add_match: fn(*mut Connection, *const c_char, *mut Error);
        dbus_connection_send_with_reply_and_block:
            fn(*mut Connection, *mut Message, c_int, *mut Error) -> *mut Message;
        dbus_connection_read_write: fn(*mut Connection, c_int) -> u32;
        dbus_connection_pop_message: fn(*mut Connection) -> *mut Message;
        dbus_connection_close: fn(*mut Connection);
        dbus_connection_unref: fn(*mut Connection);
        dbus_message_new_method_call:
            fn(*const c_char, *const c_char, *const c_char, *const c_char) -> *mut Message;
        dbus_message_unref: fn(*mut Message);
        dbus_message_is_signal: fn(*mut Message, *const c_char, *const c_char) -> u32;
        dbus_message_iter_init: fn(*mut Message, *mut Iter) -> u32;
        dbus_message_iter_init_append: fn(*mut Message, *mut Iter);
        dbus_message_iter_append_basic: fn(*mut Iter, c_int, *const c_void) -> u32;
        dbus_message_iter_get_arg_type: fn(*mut Iter) -> c_int;
        dbus_message_iter_get_basic: fn(*mut Iter, *mut c_void);
        dbus_message_iter_next: fn(*mut Iter) -> u32;
        dbus_message_iter_recurse: fn(*mut Iter, *mut Iter);
        dbus_message_new_signal: fn(*const c_char, *const c_char, *const c_char) -> *mut Message;
        dbus_message_iter_open_container: fn(*mut Iter, c_int, *const c_char, *mut Iter) -> u32;
        dbus_message_iter_close_container: fn(*mut Iter, *mut Iter) -> u32;
    }

    /// Reads the accent, then follows the portal's changes to it, on a
    /// connection of its own, until the session bus goes away.
    pub fn follow(set: impl Fn(Option<Color32>)) {
        let Some(dbus) = Dbus::load() else { return };
        // SAFETY: the connection, and each message until it is unreferenced,
        // stay valid, and only this thread uses them
        unsafe {
            let mut error = Error([0; 8]);
            (dbus.dbus_error_init)(&mut error);
            let connection = (dbus.dbus_bus_get_private)(SESSION, &mut error);
            (dbus.dbus_error_free)(&mut error);
            if connection.is_null() {
                return;
            }
            // listening first, so a change while reading isn't missed
            let rule = c"type='signal',interface='org.freedesktop.portal.Settings',member='SettingChanged'";
            (dbus.dbus_bus_add_match)(connection, rule.as_ptr(), &mut error);
            (dbus.dbus_error_free)(&mut error);
            set(read(&dbus, connection));
            while (dbus.dbus_connection_read_write)(connection, -1) != 0 {
                loop {
                    let message = (dbus.dbus_connection_pop_message)(connection);
                    if message.is_null() {
                        break;
                    }
                    let signal = c"SettingChanged".as_ptr();
                    if (dbus.dbus_message_is_signal)(message, INTERFACE.as_ptr(), signal) != 0
                        && let Some(accent) = changed(&dbus, message)
                    {
                        set(accent);
                    }
                    (dbus.dbus_message_unref)(message);
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

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn follow(_set: impl Fn(Option<Color32>)) {}

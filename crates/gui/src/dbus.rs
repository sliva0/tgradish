//! libdbus, loaded when it is needed, as the file dialogs load it too, so
//! tgradish starts without it: the session bus, for the desktop portal's
//! accent colour and for the file manager.

use std::ffi::{CStr, c_char, c_int, c_void};

pub type Connection = c_void;
pub type Message = c_void;

/// libdbus's DBusError and DBusMessageIter, which callers allocate: opaque
/// here, and larger than libdbus's own on any machine, as aligned.
#[repr(C)]
pub struct Error(pub [u64; 8]);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Iter(pub [u64; 16]);

pub const SESSION: c_int = 0;
pub const STRING: c_int = b's' as c_int;
pub const ARRAY: c_int = b'a' as c_int;
pub const VARIANT: c_int = b'v' as c_int;
pub const STRUCT: c_int = b'r' as c_int;
pub const DOUBLE: c_int = b'd' as c_int;

macro_rules! functions {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// The functions of libdbus used here, some only to test with.
        #[cfg_attr(not(test), allow(dead_code))]
        pub struct Dbus {
            $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
            _library: libloading::Library,
        }

        impl Dbus {
            pub fn load() -> Option<Dbus> {
                // SAFETY: loading libdbus runs no code of its own, and each
                // function has the type its header declares
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
    dbus_connection_set_exit_on_disconnect: fn(*mut Connection, u32);
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
    dbus_message_iter_abandon_container: fn(*mut Iter, *mut Iter);
}

/// Asks the file manager to show the file at `uri` selected in its folder,
/// through org.freedesktop.FileManager1, which Dolphin, Nautilus, Nemo,
/// Caja and Thunar answer. False where none does. Waits for its answer.
pub fn show_items(uri: &CStr) -> bool {
    let Some(dbus) = Dbus::load() else { return false };
    // SAFETY: the connection and the messages stay valid until they are
    // unreferenced, and the strings appended outlive the call
    unsafe {
        let mut error = Error([0; 8]);
        (dbus.dbus_error_init)(&mut error);
        let connection = (dbus.dbus_bus_get_private)(SESSION, &mut error);
        (dbus.dbus_error_free)(&mut error);
        if connection.is_null() {
            return false;
        }
        // libdbus ends the process when the bus goes away, unless told
        (dbus.dbus_connection_set_exit_on_disconnect)(connection, 0);
        let manager = c"org.freedesktop.FileManager1";
        let call = (dbus.dbus_message_new_method_call)(
            manager.as_ptr(),
            c"/org/freedesktop/FileManager1".as_ptr(),
            manager.as_ptr(),
            c"ShowItems".as_ptr(),
        );
        let mut shown = false;
        if !call.is_null() && items(&dbus, call, uri) {
            // a file manager may have to start first
            let reply = (dbus.dbus_connection_send_with_reply_and_block)(
                connection, call, 10_000, &mut error,
            );
            (dbus.dbus_error_free)(&mut error);
            shown = !reply.is_null();
            if shown {
                (dbus.dbus_message_unref)(reply);
            }
        }
        if !call.is_null() {
            (dbus.dbus_message_unref)(call);
        }
        (dbus.dbus_connection_close)(connection);
        (dbus.dbus_connection_unref)(connection);
        shown
    }
}

/// Appends ShowItems' arguments to `call`: the file at `uri`, then a
/// startup id, of which there is none. False where libdbus ran out of
/// memory, which leaves the message not to be sent.
unsafe fn items(dbus: &Dbus, call: *mut Message, uri: &CStr) -> bool {
    // SAFETY: `call` is a valid message, and the strings outlive it
    unsafe {
        let mut args = Iter([0; 16]);
        (dbus.dbus_message_iter_init_append)(call, &mut args);
        let mut items = Iter([0; 16]);
        if (dbus.dbus_message_iter_open_container)(&mut args, ARRAY, c"s".as_ptr(), &mut items) == 0
        {
            return false;
        }
        let item = uri.as_ptr();
        if (dbus.dbus_message_iter_append_basic)(&mut items, STRING, (&raw const item).cast()) == 0
        {
            (dbus.dbus_message_iter_abandon_container)(&mut args, &mut items);
            return false;
        }
        if (dbus.dbus_message_iter_close_container)(&mut args, &mut items) == 0 {
            return false;
        }
        let startup = c"".as_ptr();
        (dbus.dbus_message_iter_append_basic)(&mut args, STRING, (&raw const startup).cast()) != 0
    }
}

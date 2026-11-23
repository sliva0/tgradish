//! Sets the Wayland window icon via `xdg-toplevel-icon-v1`.
//!
//! Wayland compositors normally take a window's icon from the `.desktop` entry
//! matching its `app_id`, which does not exist when running an unpacked binary
//! directly, and winit does not implement `xdg-toplevel-icon-v1`. This module
//! sends the icon over winit's Wayland connection when the compositor supports
//! the protocol (e.g. KWin).

use std::io::Write;
use std::os::fd::AsFd;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_v1::XdgToplevelIconV1;
use winit::platform::wayland::WindowExtWayland;

/// Wayland icon resources and backing shared-memory buffers, kept alive for the
/// lifetime of the window.
pub(crate) struct ToplevelIcon {
    _icon: XdgToplevelIconV1,
    _buffers: Vec<WlBuffer>,
    _pool: WlShmPool,
    _file: std::fs::File,
    /// Holds any events sent for these objects without dispatching them.
    _queue: EventQueue<State>,
}

/// Attaches the application icon to `window` when running on Wayland with
/// `xdg_toplevel_icon_manager_v1`.
pub(crate) fn set(window: &winit::window::Window) -> Option<ToplevelIcon> {
    let toplevel = window.xdg_toplevel()?;
    let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: winit keeps both the Wayland display and the `xdg_toplevel`
    // valid for as long as `window` is alive.
    #[allow(unsafe_code)]
    let (backend, toplevel) = unsafe {
        let backend = Backend::from_foreign_display(display.display.as_ptr().cast());
        (backend, ObjectId::from_ptr(XdgToplevel::interface(), toplevel.as_ptr().cast()).ok()?)
    };
    let connection = Connection::from_backend(backend);
    let toplevel = XdgToplevel::from_id(&connection, toplevel).ok()?;
    let (globals, queue) = registry_queue_init::<State>(&connection).ok()?;
    let handle = queue.handle();
    let manager: XdgToplevelIconManagerV1 = globals.bind(&handle, 1..=1, ()).ok()?;
    let shm: WlShm = globals.bind(&handle, 1..=1, ()).ok()?;

    // Pack 1x, 2x, and 4x integer-scaled icons into one shared-memory file.
    let images: Vec<(i32, Vec<u8>)> = [1, 2, 4]
        .into_iter()
        .map(|scale| {
            let (side, rgba) = crate::icon::scaled(scale);
            (side as i32, argb(&rgba))
        })
        .collect();
    let mut file = tempfile::tempfile().ok()?;
    for (_, pixels) in &images {
        file.write_all(pixels).ok()?;
    }
    let bytes = images.iter().map(|(_, pixels)| pixels.len()).sum::<usize>();
    let pool = shm.create_pool(file.as_fd(), i32::try_from(bytes).ok()?, &handle, ());
    let icon = manager.create_icon(&handle, ());
    // Allow installed icon themes to override the buffer icon.
    icon.set_name("tgradish".into());
    let mut offset = 0;
    let buffers = images
        .iter()
        .map(|(side, pixels)| {
            let format = wl_shm::Format::Argb8888;
            let buffer = pool.create_buffer(offset, *side, *side, side * 4, format, &handle, ());
            icon.add_buffer(&buffer, 1);
            offset += pixels.len() as i32;
            buffer
        })
        .collect();
    manager.set_icon(&toplevel, Some(&icon));
    connection.flush().ok()?;
    Some(ToplevelIcon { _icon: icon, _buffers: buffers, _pool: pool, _file: file, _queue: queue })
}

/// Converts straight RGBA into little-endian `wl_shm::Format::Argb8888`
/// (premultiplied BGRA bytes in memory).
fn argb(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| {
            let times = |channel: u8| ((u16::from(channel) * u16::from(a) + 127) / 255) as u8;
            [times(b), times(g), times(r), a]
        })
        .collect()
}

struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

delegate_noop!(State: ignore WlShm);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore XdgToplevelIconManagerV1);
delegate_noop!(State: XdgToplevelIconV1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplies_blue_first() {
        let rgba = [255, 128, 0, 255, 200, 100, 50, 0, 255, 255, 255, 128];
        assert_eq!(argb(&rgba), [0, 128, 255, 255, 0, 0, 0, 0, 128, 128, 128, 128]);
    }
}

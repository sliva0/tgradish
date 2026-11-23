//! Wayland window activation via `xdg_activation_v1` when a second GUI instance
//! hands over files.
//!
//! Wayland compositors only raise a window when presented with an activation
//! token, and winit only consumes the startup token when creating a new
//! window. This module binds `xdg_activation_v1` directly on winit's Wayland
//! connection and activates the surface using either the token forwarded by
//! the second instance or a self-minted token (which compositors treat as an
//! attention request). It runs on the instance listener thread because a
//! minimised window renders no egui frames until restored.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::{
    self, XdgActivationTokenV1,
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

/// Wayland `xdg_activation_v1` client handle. Must be dropped before the
/// window is destroyed.
pub(crate) struct Raiser {
    connection: Connection,
    activation: XdgActivationV1,
    surface: WlSurface,
    queue: EventQueue<State>,
}

impl Raiser {
    /// Creates a [`Raiser`] for `window` if running on Wayland and the
    /// compositor advertises `xdg_activation_v1`.
    pub(crate) fn new(window: &winit::window::Window) -> Option<Raiser> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Wayland(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: winit keeps the display and surface alive for the lifetime
        // of the window, and Raiser is dropped before the window closes.
        #[allow(unsafe_code)]
        let (backend, surface) = unsafe {
            let backend = Backend::from_foreign_display(display.display.as_ptr().cast());
            (
                backend,
                ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()).ok()?,
            )
        };
        let connection = Connection::from_backend(backend);
        let surface = WlSurface::from_id(&connection, surface).ok()?;
        let (globals, queue) = registry_queue_init::<State>(&connection).ok()?;
        let activation: XdgActivationV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        Some(Raiser { connection, activation, surface, queue })
    }

    /// Activates the window using `token` (or a self-minted token if `None`).
    /// Returns true if the request was flushed to the compositor.
    pub(crate) fn raise(&mut self, token: Option<&str>) -> bool {
        let token = match token {
            Some(token) => token.to_owned(),
            None => match self.own_token() {
                Some(token) => token,
                None => return false,
            },
        };
        self.activation.activate(token, &self.surface);
        self.connection.flush().is_ok()
    }

    /// Requests a fresh activation token from the compositor for our surface
    /// (one no user input asked for, so compositors treat it as an attention
    /// request).
    fn own_token(&mut self) -> Option<String> {
        let handle = self.queue.handle();
        let request = self.activation.get_activation_token(&handle, ());
        request.set_surface(&self.surface);
        request.set_app_id("tgradish".into());
        request.commit();
        let mut state = State { token: None };
        // the compositor says the token before it answers the round trip
        let made = self.queue.roundtrip(&mut state);
        request.destroy();
        made.ok()?;
        state.token
    }
}

impl Drop for Raiser {
    fn drop(&mut self) {
        self.activation.destroy();
        let _ = self.connection.flush();
    }
}

struct State {
    token: Option<String>,
}

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

impl Dispatch<XdgActivationTokenV1, ()> for State {
    fn event(
        state: &mut State,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.token = Some(token);
        }
    }
}

delegate_noop!(State: ignore XdgActivationV1);

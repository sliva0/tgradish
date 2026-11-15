//! Bringing the window forward on Wayland, for files another GUI handed
//! over.
//!
//! A Wayland window comes forward only with a token from the compositor,
//! which winit spends on a window as it opens and not on one already open.
//! This asks with the xdg-activation protocol itself, on winit's
//! connection: with the token the launcher gave the GUI started second,
//! or without one, a token of the window's own, which compositors take as
//! the window asking for attention. It works from the listener's thread,
//! as a minimised window draws no frames until it is shown again.

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

/// Brings the window forward. It must be let go of before the window is.
pub(crate) struct Raiser {
    connection: Connection,
    activation: XdgActivationV1,
    surface: WlSurface,
    queue: EventQueue<State>,
}

impl Raiser {
    /// For `window`, where it is on Wayland and the compositor has the
    /// protocol.
    pub(crate) fn new(window: &winit::window::Window) -> Option<Raiser> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Wayland(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: winit keeps its display open as long as the window is,
        // and the window's surface with it, which the raiser is let go of
        // before
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

    /// Brings the window forward with `token`, or without one, asks for
    /// attention with a token of its own: whether it could ask.
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

    /// A token for the window, which isn't one the user's input asked for.
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

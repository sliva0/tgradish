//! Files dragged onto the window on Wayland.
//!
//! winit reports dropped files on X11, Windows and macOS, but not on
//! Wayland. There, this listens for drags itself: on winit's connection,
//! with a data device of its own read on a thread of its own, like the
//! clipboard's. Compositors send a drag to every data device of the window's
//! client, so this one takes nothing from the clipboard's.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};

const URI_LIST: &str = "text/uri-list";

pub enum Dropped {
    /// Files are dragged over the window, or not any more.
    Hovering(bool),
    Files(Vec<PathBuf>),
}

/// Receives drags on Wayland; `None` elsewhere.
pub struct Drops {
    receiver: Receiver<Dropped>,
}

impl Drops {
    /// Starts listening when the window is on Wayland.
    pub fn start(window: &impl HasDisplayHandle, ctx: &egui::Context) -> Option<Drops> {
        let display = match window.display_handle().ok()?.as_raw() {
            RawDisplayHandle::Wayland(handle) => handle.display,
            _ => return None,
        };
        // SAFETY: winit keeps its display open as long as the window is,
        // which is as long as the program runs
        #[allow(unsafe_code)]
        let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<State>(&connection).ok()?;
        let handle = queue.handle();
        let manager: WlDataDeviceManager = globals.bind(&handle, 1..=3, ()).ok()?;
        let seat: WlSeat = globals.bind(&handle, 1..=1, ()).ok()?;
        let device = manager.get_data_device(&seat, &handle, ());
        let (sender, receiver) = channel();
        let mut state = State {
            connection,
            sender,
            ctx: ctx.clone(),
            types: HashMap::new(),
            over: None,
            _device: device,
        };
        std::thread::Builder::new()
            .name("drag and drop".into())
            .spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {})
            .ok()?;
        Some(Drops { receiver })
    }

    pub fn poll(&self) -> Vec<Dropped> {
        self.receiver.try_iter().collect()
    }
}

struct State {
    connection: Connection,
    sender: Sender<Dropped>,
    ctx: egui::Context,
    /// The types each offer has.
    types: HashMap<ObjectId, Vec<String>>,
    /// The offer of the drag over the window, if it has files.
    over: Option<WlDataOffer>,
    _device: WlDataDevice,
}

impl State {
    fn send(&self, dropped: Dropped) {
        let _ = self.sender.send(dropped);
        self.ctx.request_repaint();
    }

    fn forget(&mut self, offer: &WlDataOffer) {
        self.types.remove(&offer.id());
        offer.destroy();
    }

    /// Reads the dropped files' URIs; the drag's source writes them.
    fn receive(&self, offer: &WlDataOffer) -> Option<Vec<PathBuf>> {
        let (mut reader, writer) = std::io::pipe().ok()?;
        offer.receive(URI_LIST.into(), writer.as_fd());
        drop(writer);
        self.connection.flush().ok()?;
        let mut text = String::new();
        reader.read_to_string(&mut text).ok()?;
        Some(tgradish_core::clipboard::paths_in_text(&text, false))
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut State,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.types.insert(id.id(), Vec::new());
            }
            wl_data_device::Event::Enter { serial, id: Some(offer), .. } => {
                let files = state
                    .types
                    .get(&offer.id())
                    .is_some_and(|types| types.iter().any(|t| t == URI_LIST));
                if files {
                    offer.accept(serial, Some(URI_LIST.into()));
                    if offer.version() >= 3 {
                        offer.set_actions(DndAction::Copy, DndAction::Copy);
                    }
                    state.over = Some(offer);
                    state.send(Dropped::Hovering(true));
                } else {
                    offer.accept(serial, None);
                    state.forget(&offer);
                }
            }
            wl_data_device::Event::Leave => {
                if let Some(offer) = state.over.take() {
                    state.forget(&offer);
                    state.send(Dropped::Hovering(false));
                }
            }
            wl_data_device::Event::Drop => {
                if let Some(offer) = state.over.take() {
                    let files = state.receive(&offer);
                    if offer.version() >= 3 {
                        offer.finish();
                    }
                    state.forget(&offer);
                    state.send(Dropped::Hovering(false));
                    if let Some(files) = files.filter(|files| !files.is_empty()) {
                        state.send(Dropped::Files(files));
                    }
                }
            }
            // copying and pasting is the clipboard's business
            wl_data_device::Event::Selection { id: Some(offer) } => state.forget(&offer),
            _ => {}
        }
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for State {
    fn event(
        state: &mut State,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            state.types.entry(offer.id()).or_default().push(mime_type);
        }
    }
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

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut State,
        _: &WlSeat,
        _: <WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut State,
        _: &WlDataDeviceManager,
        _: <WlDataDeviceManager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

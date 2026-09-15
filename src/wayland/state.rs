use crate::wayland::resources::{KeyboardEvent, KeyboardEventKind, PointerEvent, PointerEventKind};
use log::debug;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const DEFAULT_WIDTH: i32 = 400;
const DEFAULT_HEIGHT: i32 = 400;

pub struct AppStateInternal {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub seat: Option<wl_seat::WlSeat>,
    pub running: bool,
    pub pending_sizes: std::collections::HashMap<usize, (i32, i32)>, // latest size seen per window
    pub pending_configures: Vec<(usize, i32, i32)>,                  // (window_id, width, height)
    pub pending_closes: Vec<usize>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub surface_window_map: Vec<(wl_surface::WlSurface, usize)>,
    pub pointer_focus: Option<usize>,
    pub keyboard_focus: Option<usize>,
    pub pending_pointer_events: Vec<PointerEvent>,
    pub pending_keyboard_events: Vec<KeyboardEvent>,
}

impl AppStateInternal {
    pub fn register_surface(&mut self, surface: &wl_surface::WlSurface, window_id: usize) {
        self.surface_window_map.push((surface.clone(), window_id));
    }

    pub fn unregister_surface(&mut self, window_id: usize) {
        self.surface_window_map.retain(|(_, id)| *id != window_id);
        if self.pointer_focus == Some(window_id) {
            self.pointer_focus = None;
        }
        if self.keyboard_focus == Some(window_id) {
            self.keyboard_focus = None;
        }
    }

    fn lookup_window(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surface_window_map
            .iter()
            .find(|(s, _)| s == surface)
            .map(|(_, id)| *id)
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for AppStateInternal {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => state.compositor = Some(registry.bind(name, 4, qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => state.seat = Some(registry.bind(name, 5, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for AppStateInternal {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm::WlShm, ()> for AppStateInternal {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppStateInternal {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_buffer::WlBuffer, ()> for AppStateInternal {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for AppStateInternal {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppStateInternal {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, usize> for AppStateInternal {
    fn event(
        state: &mut Self,
        xdg_surf: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        window_id: &usize,
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surf.ack_configure(serial);
            let (w, h) = state
                .pending_sizes
                .get(window_id)
                .copied()
                .unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
            state.pending_configures.push((*window_id, w, h));
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, usize> for AppStateInternal {
    fn event(
        state: &mut Self,
        _t: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        window_id: &usize,
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 && height > 0 {
                    state.pending_sizes.insert(*window_id, (width, height));
                }
            }
            // handle close request from compositor
            xdg_toplevel::Event::Close => state.pending_closes.push(*window_id),
            _ => {}
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for AppStateInternal {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let WEnum::Value(caps) = capabilities {
                if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                    debug!("wl_seat: acquired wl_pointer");
                }
                if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                    debug!("wl_seat: acquired wl_keyboard");
                }
            }
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for AppStateInternal {
    fn event(
        state: &mut Self,
        _pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                if let Some(window_id) = state.lookup_window(&surface) {
                    state.pointer_focus = Some(window_id);
                    state.pending_pointer_events.push(PointerEvent {
                        window_id,
                        kind: PointerEventKind::Enter {
                            x: surface_x,
                            y: surface_y,
                        },
                    });
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if let Some(window_id) = state.lookup_window(&surface) {
                    state.pointer_focus = None;
                    state.pending_pointer_events.push(PointerEvent {
                        window_id,
                        kind: PointerEventKind::Leave,
                    });
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                if let Some(window_id) = state.pointer_focus {
                    state.pending_pointer_events.push(PointerEvent {
                        window_id,
                        kind: PointerEventKind::Motion {
                            x: surface_x,
                            y: surface_y,
                        },
                    });
                }
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                if let Some(window_id) = state.pointer_focus {
                    let pressed =
                        matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                    state.pending_pointer_events.push(PointerEvent {
                        window_id,
                        kind: PointerEventKind::Button { button, pressed },
                    });
                }
            }
            wl_pointer::Event::Axis { axis, value, .. } => {
                if let Some(window_id) = state.pointer_focus {
                    if let WEnum::Value(axis_val) = axis {
                        state.pending_pointer_events.push(PointerEvent {
                            window_id,
                            kind: PointerEventKind::Axis {
                                axis: axis_val as u32,
                                value,
                            },
                        });
                    }
                }
            }
            _ => {} // AxisSource, AxisStop, AxisDiscrete, Frame, etc.
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppStateInternal {
    fn event(
        state: &mut Self,
        _keyboard: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { surface, .. } => {
                if let Some(window_id) = state.lookup_window(&surface) {
                    state.keyboard_focus = Some(window_id);
                    state.pending_keyboard_events.push(KeyboardEvent {
                        window_id,
                        kind: KeyboardEventKind::Enter,
                    });
                }
            }
            wl_keyboard::Event::Leave { surface, .. } => {
                if let Some(window_id) = state.lookup_window(&surface) {
                    state.keyboard_focus = None;
                    state.pending_keyboard_events.push(KeyboardEvent {
                        window_id,
                        kind: KeyboardEventKind::Leave,
                    });
                }
            }
            wl_keyboard::Event::Key {
                key,
                state: key_state,
                ..
            } => {
                if let Some(window_id) = state.keyboard_focus {
                    let pressed =
                        matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed));
                    state.pending_keyboard_events.push(KeyboardEvent {
                        window_id,
                        kind: KeyboardEventKind::Key { key, pressed },
                    });
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(window_id) = state.keyboard_focus {
                    state.pending_keyboard_events.push(KeyboardEvent {
                        window_id,
                        kind: KeyboardEventKind::Modifiers {
                            depressed: mods_depressed,
                            latched: mods_latched,
                            locked: mods_locked,
                            group,
                        },
                    });
                }
            }
            wl_keyboard::Event::Keymap { .. } => {
                // TODO: xkbcommon integration for proper key symbol resolution
            }
            _ => {}
        }
    }
}

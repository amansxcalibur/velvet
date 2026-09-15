use bevy_ecs::prelude::*;
use wayland_client::protocol::{wl_compositor, wl_shm};
use wayland_protocols::xdg::shell::client::xdg_wm_base;

#[derive(Message)]
pub struct WindowCloseRequested(pub usize);

#[derive(Resource)]
pub struct WaylandGlobals {
    pub compositor: wl_compositor::WlCompositor,
    pub shm: wl_shm::WlShm,
    pub wm_base: xdg_wm_base::XdgWmBase,
}

#[derive(Debug, Clone)]
pub enum PointerEventKind {
    Enter { x: f64, y: f64 },
    Leave,
    Motion { x: f64, y: f64 },
    Button { button: u32, pressed: bool },
    Axis { axis: u32, value: f64 },
}

/// A single pointer (mouse) event tagged with the owning window.
#[derive(Message, Debug, Clone)]
pub struct PointerEvent {
    pub window_id: usize,
    pub kind: PointerEventKind,
}

#[derive(Debug, Clone)]
pub enum KeyboardEventKind {
    Enter,
    Leave,
    Key { key: u32, pressed: bool },
    Modifiers {
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    },
}

/// A single keyboard event tagged with the owning window.
#[derive(Message, Debug, Clone)]
pub struct KeyboardEvent {
    pub window_id: usize,
    pub kind: KeyboardEventKind,
}

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

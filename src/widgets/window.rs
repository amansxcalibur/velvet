use bevy_ecs::{prelude::*, query, world};
use smithay_client_toolkit::compositor::{self, Surface};
use smithay_client_toolkit::shell::xdg::window;
use std::{collections::HashMap};
use std::os::unix::io::AsFd;
use wayland_client::{
    QueueHandle,
    protocol::{wl_compositor, wl_shm, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::state::AppStateInternal;

#[derive(Component, Clone, Copy)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

#[derive(Component, Clone, Copy)]
pub struct  BackgroundColor { // Following Argb8888 format
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8, // alpha
}

impl Default for Size {
    fn default() -> Self {
        Self { width: 400, height: 300 }
    }
}

impl Default for BackgroundColor {
    fn default() -> Self {
        Self { b: 0xAA, g: 0x88, r: 0x44, a: 0xFF }
    }
}

#[derive(Component)]
pub struct VelvetWindow {
    pub surface: wl_surface::WlSurface,
    pub xdg_surface: xdg_surface::XdgSurface,
    pub toplevel: xdg_toplevel::XdgToplevel,
    pub configured: bool,
}

impl VelvetWindow {
    pub fn new(
        compositor: &wl_compositor::WlCompositor,
        wm_base: &xdg_wm_base::XdgWmBase,
        qh: &QueueHandle<crate::state::AppStateInternal>,
        desc: WindowDescriptor,
        window_id: usize,
    ) -> Self {
        let surface = compositor.create_surface(qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, qh, window_id);
        let toplevel= xdg_surface.get_toplevel(qh, ());

        toplevel.set_title(desc.title);
        surface.commit();

        Self {
            surface,
            xdg_surface,
            toplevel,
            configured: false,
        }
    }
    pub fn draw_solid_teal(
        &self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<crate::state::AppStateInternal>,
        size: &Size,
        color: &BackgroundColor,
    ) {
        let height = size.height;
        let width = size.width;
        let stride = width * 4;
        let buf_size =(stride*height) as usize;

        let file = tempfile::tempfile().unwrap();
        file.set_len(buf_size as u64).unwrap();
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file).unwrap() };

        for px in mmap.chunks_exact_mut(4) {
            px[0] = color.b;
            px[1] = color.g;
            px[2] = color.r;
            px[3] = color.a;
        }

        let pool = shm.create_pool(file.as_fd(), buf_size as i32, qh, ());
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());

        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.damage_buffer(0, 0, width, height);
        self.surface.commit();
    }
}
pub fn render_windows_system(
    world: &mut World,
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<AppStateInternal>,
 ) {
    let mut query = world.query::<(&VelvetWindow, &Size, &BackgroundColor)>();
    for (window, size, color ) in query.iter(world) {
        if window.configured {
            window.draw_solid_teal(shm, qh, size,color);
        }
    }
}

pub enum WindowRole {
    Toplevel,
    Dialog,
    Popup,
}

pub struct WindowDescriptor {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub role: WindowRole,
    pub decorated: bool,
}

impl Default for WindowDescriptor {
    fn default() -> Self {
        Self {
            title: "Velvet Application".into(),
            width: 800,
            height: 600,
            role: WindowRole::Toplevel,
            decorated: true,
        }
    }
}
#[derive(Resource, Default)]
pub struct WindowRegistry {
    entities: HashMap<usize, Entity>,
    next_id: usize,
}

impl WindowRegistry {
    pub fn next_id(&self) -> usize {
        self.next_id
    }
    pub fn register(&mut self, window_id: usize, entity: Entity) {
        self.entities.insert(window_id, entity);
        self.next_id = self.next_id.max(window_id+1);
    }
    pub fn get(&self, window_id: usize) -> Option<Entity> {
        self.entities.get(&window_id).copied()
    }
}
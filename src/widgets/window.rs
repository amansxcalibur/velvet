use bevy_ecs::prelude::*;
use std::collections::HashMap;
use std::os::unix::io::AsFd;
use wayland_client::{
    QueueHandle,
    protocol::{wl_compositor, wl_shm, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::state::AppStateInternal;

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
        let toplevel = xdg_surface.get_toplevel(qh, ());

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
    ) {
        const WIDTH: i32 = 400;
        const HEIGHT: i32 = 300;
        let stride = WIDTH * 4;
        let size = (stride * HEIGHT) as usize;

        let file = tempfile::tempfile().unwrap();
        file.set_len(size as u64).unwrap();
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file).unwrap() };

        for px in mmap.chunks_exact_mut(4) {
            px[0] = 0xAA; // B
            px[1] = 0x88; // G
            px[2] = 0x44; // R
            px[3] = 0xFF; // A
        }

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(0, WIDTH, HEIGHT, stride, wl_shm::Format::Argb8888, qh, ());

        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.damage_buffer(0, 0, WIDTH, HEIGHT);
        self.surface.commit();
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
pub struct WindowManager {
    windows: HashMap<usize, VelvetWindow>,
    next_id: usize,
}

impl WindowManager {
    pub fn render_all(&mut self, shm: &wl_shm::WlShm, qh: &QueueHandle<AppStateInternal>) {
        for window in self.windows.values_mut() {
            if window.configured {
                window.draw_solid_teal(shm, qh);
            }
        }
    }
    pub fn next_id(&self) -> usize {
        self.next_id
    }

    pub fn add_window(&mut self, window: VelvetWindow) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.windows.insert(id, window);
        id
    }

    pub fn mark_configured(&mut self, id: usize) {
        if let Some(w) = self.windows.get_mut(&id) {
            w.configured = true;
        }
    }
}

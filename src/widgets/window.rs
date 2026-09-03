use super::gpu::{GpuContext, GpuSurface};
use crate::wayland::AppStateInternal;
use crate::widgets::descriptor::WindowDescriptor;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use std::os::unix::io::AsFd;
use wayland_client::Proxy;
use wayland_client::{
    QueueHandle,
    protocol::{wl_compositor, wl_shm, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

#[derive(Clone, Copy)]
struct DisplayPtr(*mut std::ffi::c_void);
unsafe impl Send for DisplayPtr {}
unsafe impl Sync for DisplayPtr {}

pub struct VelvetWindow {
    pub gpu: Option<GpuSurface>,
    pub surface: wl_surface::WlSurface,
    pub xdg_surface: xdg_surface::XdgSurface,
    pub toplevel: xdg_toplevel::XdgToplevel,
    pub configured: bool,
    display_ptr: DisplayPtr,
}

impl VelvetWindow {
    pub fn new(
        compositor: &wl_compositor::WlCompositor,
        wm_base: &xdg_wm_base::XdgWmBase,
        qh: &QueueHandle<AppStateInternal>,
        desc: WindowDescriptor,
        window_id: usize,
        display_ptr: *mut std::ffi::c_void,
    ) -> Self {
        let surface = compositor.create_surface(qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, qh, window_id);
        let toplevel = xdg_surface.get_toplevel(qh, window_id);

        toplevel.set_title(desc.title);
        surface.commit();

        Self {
            surface,
            xdg_surface,
            toplevel,
            configured: false,
            gpu: None,
            display_ptr: DisplayPtr(display_ptr),
        }
    }

    pub fn draw_solid_teal(&self, shm: &wl_shm::WlShm, qh: &QueueHandle<AppStateInternal>) {
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

    pub fn render(&self, ctx: &GpuContext) {
        if let Some(gpu) = &self.gpu {
            gpu.render(
                ctx,
                wgpu::Color {
                    r: 0.267,
                    g: 0.529,
                    b: 0.667,
                    a: 1.0,
                },
            );
        }
    }
}

impl HasDisplayHandle for VelvetWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = WaylandDisplayHandle::new(std::ptr::NonNull::new(self.display_ptr.0).unwrap());
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Wayland(handle)) })
    }
}

impl HasWindowHandle for VelvetWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let ptr = self.surface.id().as_ptr();
        let handle = WaylandWindowHandle::new(std::ptr::NonNull::new(ptr as *mut _).unwrap());
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Wayland(handle)) })
    }
}

impl Drop for VelvetWindow {
    fn drop(&mut self) {
        self.toplevel.destroy();
        self.xdg_surface.destroy();
        self.surface.destroy();
    }
}

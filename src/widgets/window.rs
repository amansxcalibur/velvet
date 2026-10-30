use crate::render::GpuSurface;
use crate::wayland::AppStateInternal;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use wayland_client::Proxy;
use wayland_client::{
    protocol::{wl_compositor, wl_surface},
    QueueHandle,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

#[derive(Clone, Copy)]
struct DisplayPtr(*mut std::ffi::c_void);
unsafe impl Send for DisplayPtr {}
unsafe impl Sync for DisplayPtr {}

#[derive(Debug)]
pub enum WindowRole {
    Toplevel,
    Dialog,
    Popup,
}

#[derive(Debug)]
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

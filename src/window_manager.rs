use crate::render::{GpuContext, GpuSurface};
use crate::wayland::WindowCloseRequested;
use crate::widgets::window::VelvetWindow;
use bevy_ecs::prelude::*;
use log::debug;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct WindowManager {
    pub(crate) windows: HashMap<usize, VelvetWindow>,
    next_id: usize,
}

impl WindowManager {
    pub fn add_window(&mut self, window: VelvetWindow) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.windows.insert(id, window);
        id
    }

    pub fn next_id(&self) -> usize {
        self.next_id
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn mark_configured(&mut self, id: usize) {
        if let Some(w) = self.windows.get_mut(&id) {
            w.configured = true;
        }
    }

    pub fn destroy_window_by_id(&mut self, id: usize) {
        if let Some(_window) = self.windows.remove(&id) {
            // _window goes out of scope right here & hence "drop"-ed
        }
    }

    pub fn ensure_gpu_surface(&mut self, id: usize, ctx: &GpuContext, w: u32, h: u32) {
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };

        if let Some(gpu) = &mut window.gpu {
            gpu.resize(ctx, w, h);
        } else {
            let gpu = GpuSurface::new(ctx, window, w, h);
            window.gpu = Some(gpu);
        }
    }

    pub fn render_all_gpu(&self, ctx: &GpuContext) {
        for window in self.windows.values() {
            window.render(ctx);
        }
    }

    pub fn render_window(&self, id: usize, ctx: &GpuContext) {
        if let Some(window) = self.windows.get(&id) {
            window.render(ctx);
        }
    }
}

pub fn process_window_closes(
    mut reader: MessageReader<WindowCloseRequested>,
    mut wm: ResMut<WindowManager>,
) {
    for msg in reader.read() {
        let id = msg.0;

        wm.destroy_window_by_id(id);
        debug!("Destroyed VelvetWindow (id: {id:#04x})");
    }
}

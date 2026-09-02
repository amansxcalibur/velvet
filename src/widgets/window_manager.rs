use crate::wayland::AppStateInternal;
use crate::widgets::window::VelvetWindow;
use bevy_ecs::prelude::*;
use std::collections::HashMap;
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_shm;

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

    pub fn destroy_window_by_id(&mut self, id: usize) {
        if let Some(_window) = self.windows.remove(&id) {
            // _window goes out of scope right here, hence "drop"-ed
        }
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

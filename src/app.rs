use crate::render::*;
use crate::wayland::{AppStateInternal, WaylandGlobals, WindowCloseRequested};
use crate::widgets::window::{VelvetWindow, WindowDescriptor};
use crate::window_manager::{WindowManager, process_window_closes};
use bevy_ecs::message::{MessageRegistry, message_update_system};
use bevy_ecs::prelude::*;
use log::*;
use std::collections::HashMap;
use wayland_client::{Connection, QueueHandle};

pub struct Application {
    world: World,
    schedule: Schedule,
    conn: Connection,
    event_queue: wayland_client::EventQueue<AppStateInternal>,
    qh: QueueHandle<AppStateInternal>,
    app_state: AppStateInternal, // reuse, don't recreate
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    pub fn new() -> Self {
        let _ = env_logger::Builder::new()
            .filter_level(log::LevelFilter::Warn)
            .try_init();

        let conn = Connection::connect_to_env().expect("no wayland compositor");
        let display = conn.display();
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();
        let _wl_registry = display.get_registry(&qh, ());

        let mut app_state = AppStateInternal {
            compositor: None,
            shm: None,
            wm_base: None,
            running: true,
            pending_configures: Vec::new(),
            pending_closes: Vec::new(),
            pending_sizes: HashMap::new(),
        };

        // fetch globals
        event_queue.roundtrip(&mut app_state).unwrap();

        let globals = WaylandGlobals {
            compositor: app_state.compositor.take().expect("Missing wl_compositor"),
            shm: app_state.shm.take().expect("Missing wl_shm"),
            wm_base: app_state.wm_base.take().expect("Missing xdg_wm_base"),
        };

        let mut world = World::new();
        MessageRegistry::register_message::<WindowCloseRequested>(&mut world);
        world.init_resource::<WindowManager>();
        world.insert_resource(globals);
        world.insert_resource(GpuContext::new());

        let mut schedule = Schedule::default();
        schedule.add_systems((message_update_system, process_window_closes));

        Self {
            world,
            schedule,
            conn,
            event_queue,
            qh,
            app_state,
        }
    }

    pub fn create_window(&mut self, desc: WindowDescriptor) -> usize {
        let globals = self.world.resource::<WaylandGlobals>();

        // peeky boo
        let window_id = {
            let window_manager = self.world.resource::<WindowManager>();
            window_manager.next_id()
        };
        let display_ptr = self.conn.backend().display_ptr() as *mut std::ffi::c_void;
        let window = VelvetWindow::new(
            &globals.compositor,
            &globals.wm_base,
            &self.qh,
            desc,
            window_id,
            display_ptr,
        );
        self.add_window(window)
    }

    pub fn add_window(&mut self, window: VelvetWindow) -> usize {
        let mut window_manager = self.world.resource_mut::<WindowManager>();
        window_manager.add_window(window)
    }

    pub fn set_window_quads(
        &mut self,
        window_id: usize,
        quads: Vec<crate::render::quad::QuadInstance>,
    ) {
        let mut window_manager = self.world.resource_mut::<WindowManager>();
        if let Some(window) = window_manager.windows.get_mut(&window_id) {
            window.quads = quads;
        }
    }

    pub fn run(mut self) {
        while self.app_state.running {
            self.event_queue
                .dispatch_pending(&mut self.app_state)
                .unwrap();

            if !self.app_state.pending_configures.is_empty() {
                self.world.resource_scope(|world, ctx: Mut<GpuContext>| {
                    let mut wm = world.resource_mut::<WindowManager>();
                    for (id, w, h) in self.app_state.pending_configures.drain(..) {
                        wm.ensure_gpu_surface(id, &ctx, w as u32, h as u32);
                        wm.mark_configured(id);
                    }
                });
            }

            // close events
            if !self.app_state.pending_closes.is_empty() {
                let mut ecs_messages = self.world.resource_mut::<Messages<WindowCloseRequested>>();
                for id in self.app_state.pending_closes.drain(..) {
                    ecs_messages.write(WindowCloseRequested(id));
                }
            }

            self.schedule.run(&mut self.world);

            let wm = self.world.resource::<WindowManager>();
            if wm.is_empty() {
                break; // no windows remain
            }

            self.world.resource_scope(|world, ctx: Mut<GpuContext>| {
                let wm = world.resource_mut::<WindowManager>();
                wm.render_all_gpu(&ctx);
            });

            let _ = self.conn.flush();
            self.event_queue
                .blocking_dispatch(&mut self.app_state)
                .unwrap();
        }
        debug!("Closing Velvet Application")
    }
}

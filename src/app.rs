use crate::wayland::{AppStateInternal, WaylandGlobals, WindowCloseRequested};
use crate::widgets::descriptor::WindowDescriptor;
use crate::widgets::window::VelvetWindow;
use crate::widgets::window_manager::WindowManager;
use bevy_ecs::prelude::*;
use wayland_client::{Connection, QueueHandle};

use log::*;

pub struct Application {
    world: World,
    conn: Connection,
    event_queue: wayland_client::EventQueue<AppStateInternal>,
    qh: QueueHandle<AppStateInternal>,
    app_state: AppStateInternal, // reuse, don't recreate
}

impl Application {
    pub fn new() -> Self {
        let _ = env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
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
        };

        // fetch globals
        event_queue.roundtrip(&mut app_state).unwrap();

        let globals = WaylandGlobals {
            compositor: app_state.compositor.take().expect("Missing wl_compositor"),
            shm: app_state.shm.take().expect("Missing wl_shm"),
            wm_base: app_state.wm_base.take().expect("Missing xdg_wm_base"),
        };

        let mut world = World::new();
        world.init_resource::<Messages<WindowCloseRequested>>();
        world.init_resource::<WindowManager>();
        world.insert_resource(globals);

        Self {
            world,
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
        let window = VelvetWindow::new(
            &globals.compositor,
            &globals.wm_base,
            &self.qh,
            desc,
            window_id,
        );
        self.add_window(window)
    }

    pub fn add_window(&mut self, window: VelvetWindow) -> usize {
        let mut window_manager = self.world.resource_mut::<WindowManager>();
        window_manager.add_window(window)
    }

    pub fn run(mut self) {
        while self.app_state.running {
            self.event_queue
                .dispatch_pending(&mut self.app_state)
                .unwrap();

            if !self.app_state.pending_configures.is_empty() {
                let mut wm = self.world.resource_mut::<WindowManager>();
                for id in self.app_state.pending_configures.drain(..) {
                    wm.mark_configured(id);
                }
            }

            if !self.app_state.pending_closes.is_empty() {
                let mut wm = self.world.resource_mut::<WindowManager>();
                for id in self.app_state.pending_closes.drain(..) {
                    wm.destroy_window_by_id(id);
                    debug!("Destroyed VelvetWindow (id: {id:#04x})")
                }
            }

            let wm = self.world.resource::<WindowManager>();
            if wm.is_empty() {
                break; // Exit main loop only when no windows remain
            }

            let shm = self.world.resource::<WaylandGlobals>().shm.clone();
            self.world
                .resource_mut::<WindowManager>()
                .render_all(&shm, &self.qh);
            self.world
                .resource_mut::<Messages<WindowCloseRequested>>()
                .update();

            let _ = self.conn.flush();
            self.event_queue
                .blocking_dispatch(&mut self.app_state)
                .unwrap();
        }
        debug!("Closing Application")
    }
}

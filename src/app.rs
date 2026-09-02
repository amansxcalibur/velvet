use crate::state::{AppStateInternal, WaylandGlobals, WindowCloseRequested};
use crate::widgets::window::{VelvetWindow, WindowDescriptor, WindowRegistry, render_windows_system};
use bevy_ecs::{entity, prelude::*};
use wayland_client::{Connection, QueueHandle};

pub struct Application {
    world: World,
    conn: Connection,
    event_queue: wayland_client::EventQueue<AppStateInternal>,
    qh: QueueHandle<AppStateInternal>,
    app_state: AppStateInternal, // reuse, don't recreate
}

impl Application {
    pub fn new() -> Self {
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
        world.init_resource::<WindowRegistry>();
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
            let window_manager = self.world.resource::<WindowRegistry>();
            window_manager.next_id()
        };
        let window = VelvetWindow::new(
            &globals.compositor,
            &globals.wm_base,
            &self.qh,
            desc,
            window_id,
        );
        let entity = self.world.spawn((window, crate::widgets::window::Size::default(), crate::widgets::window::BackgroundColor::default())).id();
        self.world.resource_mut::<WindowRegistry>().register(window_id,entity);

        window_id
    }

    // pub fn add_window(&mut self, window: VelvetWindow) -> usize {
    //     let mut window_manager = self.world.resource_mut::<WindowRegistry>();
    //     window_manager.register(window_id, entity)
    // }

    pub fn run(mut self) {
        while self.app_state.running {
            self.event_queue
                .dispatch_pending(&mut self.app_state)
                .unwrap();

            if !self.app_state.pending_configures.is_empty() {
                let ids: Vec<usize> = self.app_state.pending_configures.drain(..).collect();
                for id in ids {
                    if let Some(entity) = self.world.resource::<WindowRegistry>().get(id) {
                        if let Some(mut window) = self.world.get_mut::<VelvetWindow>(entity) {
                            window.configured = true;
                        }
                    }
                }
            }

            let close_messages = self.world.resource::<Messages<WindowCloseRequested>>();
            if !close_messages.is_empty() {
                break;
            }

            let shm = self.world.resource::<WaylandGlobals>().shm.clone();
            render_windows_system(&mut self.world, &shm, &self.qh);
            self.world
                .resource_mut::<Messages<WindowCloseRequested>>()
                .update();

            let _ = self.conn.flush();
            self.event_queue
                .blocking_dispatch(&mut self.app_state)
                .unwrap();
        }
    }
}

use crate::render::*;
use crate::time::TimeContext;
use crate::wayland::{AppStateInternal, WaylandGlobals, WindowCloseRequested};
use crate::widgets::window::{VelvetWindow, WindowDescriptor};
use crate::widgets::{
    build_tree, flatten_system, layout_system, sync_styles_to_taffy, Node, QuadBuffers,
    WindowLayouts,
};
use crate::window_manager::{process_window_closes, WindowManager};
use bevy_ecs::message::{message_update_system, MessageRegistry};
use bevy_ecs::prelude::*;
use log::*;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::thread;
use std::time::Duration;
use wayland_backend::client::WaylandError;
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
        world.insert_resource(WindowLayouts {
            windows: HashMap::new(),
        });
        world.init_resource::<QuadBuffers>();
        world.init_resource::<TimeContext>();
        world.insert_resource(globals);
        world.insert_resource(GpuContext::new());

        let mut schedule = Schedule::default();
        schedule.add_systems((
            message_update_system,
            process_window_closes,
            sync_styles_to_taffy.before(layout_system),
            layout_system.after(process_window_closes),
            flatten_system.after(layout_system),
        ));

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

    /// Builds the widget tree for a window and returns the root widget entity,
    /// which callers can use with [`Self::child`] and `set_widget_size` to
    /// mutate the UI after the fact.
    pub fn set_layout(&mut self, window_id: usize, width: f32, height: f32, node: Node) -> Entity {
        let layout = build_tree(&mut self.world, window_id, width, height, node);
        let root = *layout.tree.0.get_node_context(layout.root).unwrap();
        let mut layouts = self.world.resource_mut::<WindowLayouts>();
        layouts.windows.insert(window_id, layout);
        root
    }

    /// Returns the `index`-th child widget of `parent`, or `None`.
    pub fn child(&mut self, parent: Entity, index: usize) -> Option<Entity> {
        if let Some(children) = self.world.get::<Children>(parent) {
            let mut ids: Vec<Entity> = vec![];
            for child in children {
                ids.push(*child);
            }
            ids.get(index).copied()
        } else {
            None
        }
    }

    /// The ECS world backing this app.
    ///
    /// This is the GLib-idiomatic way to reach source registration: any code
    /// with the world can call [`TimeContext`](crate::time::TimeContext) fns
    /// directly (`TimeContext::add_timeout(world, …)`), with no reference to
    /// `Application`. Systems and callbacks already receive the world, so they
    /// never need this accessor — it exists for bootstrapping in `main`.
    pub fn world(&mut self) -> &mut World {
        &mut self.world
    }

    /// Runs the event loop as a GLib-style main context.
    ///
    /// Each iteration: dispatch pending Wayland events → pump due time sources
    /// ([`TimeContext::dispatch_pending`](crate::time::TimeContext::dispatch_pending))
    /// → run the ECS schedule (style sync, layout, flatten) → render. Before
    /// waiting for more events, the loop asks
    /// [`TimeContext::next_timeout`](crate::time::TimeContext::next_timeout)
    /// how long it may sleep: if nothing is scheduled it blocks indefinitely on
    /// the compositor (zero CPU while idle), otherwise it wakes at the earliest
    /// timer/tick deadline.
    ///
    /// Time sources live entirely in [`TimeContext`](crate::time::TimeContext);
    /// the only code that knows about this loop is the pump below.
    pub fn run(mut self) {
        while self.app_state.running {
            let was_idle = self
                .event_queue
                .dispatch_pending(&mut self.app_state)
                .unwrap()
                == 0;

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

            // Pump due timers/tick/idle sources *before* the ECS schedule so any
            // widget mutations they make are laid out on the same frame.
            TimeContext::dispatch_pending(&mut self.world, was_idle);

            self.schedule.run(&mut self.world);

            let wm = self.world.resource::<WindowManager>();
            if wm.is_empty() {
                break; // no windows remain
            }

            self.world.resource_scope(|world, ctx: Mut<GpuContext>| {
                let wm = world.resource::<WindowManager>();
                let buffers = world.resource::<QuadBuffers>();
                wm.render_all_gpu(&ctx, buffers);
            });

            let _ = self.conn.flush();
            match TimeContext::next_timeout(&mut self.world) {
                // Something is scheduled: wait (at most) that long for events.
                Some(wait) => {
                    self.timed_dispatch(wait);
                }
                // Nothing scheduled: block until the compositor says something.
                None => {
                    self.event_queue
                        .blocking_dispatch(&mut self.app_state)
                        .unwrap();
                }
            }
        }
        debug!("Closing Velvet Application")
    }

    /// Like [`EventQueue::blocking_dispatch`](wayland_client::EventQueue::blocking_dispatch)
    /// but waits at most `wait` for incoming events instead of blocking forever,
    /// so the loop can honour timer/tick deadlines.
    fn timed_dispatch(&mut self, wait: Duration) {
        let _ = self.event_queue.dispatch_pending(&mut self.app_state);
        let _ = self.conn.flush();

        if let Some(guard) = self.conn.prepare_read() {
            thread::sleep(wait);
            // Opportunistic non-blocking read; the poll-and-read dance is the
            // same as `wayland_client::conn::blocking_read`, but bounded.
            match guard.read() {
                Ok(_) => (),
                Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => (),
                Err(e) => panic!("wayland read failed: {:?}", e),
            }
        }

        let _ = self.event_queue.dispatch_pending(&mut self.app_state);
    }
}

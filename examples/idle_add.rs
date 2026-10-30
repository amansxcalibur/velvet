use std::cell::Cell;
use std::time::Duration;
use velvet::widgets::builder::{block, column};
use velvet::widgets::window::WindowDescriptor;
use velvet::Application;
use velvet::TimeContext;

fn main() {
    let mut app = Application::new();
    let win = app.create_window(WindowDescriptor::default());
    app.set_layout(
        win,
        400.0,
        300.0,
        column(
            vec![block([0.0, 1.0, 0.0, 1.0], Some([300.0, 80.0]))],
            0.0,
            0.0,
        ),
    );

    // g_timeout_add: fires every second and keeps going while it returns `true`.
    let count = Cell::new(0u64);
    TimeContext::add_timeout(app.world(), Duration::from_secs(1), move |_| {
        count.set(count.get() + 1);
        println!("timeout: {} seconds elapsed", count.get());
        true
    });

    // g_timeout_add_once: fires exactly once.
    TimeContext::add_timeout_once(app.world(), Duration::from_secs(2), |_| {
        println!("timeout once: fired after 2 seconds");
    });

    // gtk_widget_add_tick_callback: runs every frame with the frame delta.
    let frames = Cell::new(0u64);
    TimeContext::add_tick_callback(app.world(), move |_, delta| {
        frames.set(frames.get() + 1);
        if frames.get() % 30 == 0 {
            println!("tick: frame delta {:.3} sec", delta.as_secs_f32());
        }
        true
    });

    // g_idle_add: runs while the loop has nothing else to do. It *must* return
    // `false` eventually, or the loop never sleeps (an active idle source keeps
    // the loop waking up — exactly like GLib's low-priority idle sources).
    let idle_runs = Cell::new(0u64);
    TimeContext::add_idle(app.world(), move |_| {
        idle_runs.set(idle_runs.get() + 1);
        println!("idle: ran {} times while idle", idle_runs.get());
        idle_runs.get() < 5
    });

    app.run();
}

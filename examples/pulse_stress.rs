//! A pulse stress test with a fast update interval and many rendered boxes.
//!
//! It prints the render-loop frame rate once per second. Increase
//! `EXTRA_BOXES` or decrease `PULSE_INTERVAL_MS` to test a heavier workload.
//!
//! Run under a Wayland compositor:
//!
//!     cargo run --example pulse_stress

use std::cell::Cell;
use std::time::Duration;
use velvet::widgets::builder::{block, column};
use velvet::widgets::layout::set_widget_size;
use velvet::widgets::window::WindowDescriptor;
use velvet::{Application, TimeContext};

const SMALL: [f32; 2] = [120.0, 80.0];
const BIG: [f32; 2] = [320.0, 200.0];
const PULSE_INTERVAL_MS: u64 = 100;
const EXTRA_BOXES: usize = 10_000;

fn main() {
    let mut app = Application::new();
    let win = app.create_window(WindowDescriptor::default());

    let mut boxes = Vec::with_capacity(EXTRA_BOXES + 1);
    boxes.push(block([1.0, 0.0, 0.0, 1.0], Some(SMALL)));
    for index in 0..EXTRA_BOXES {
        let color = if index % 2 == 0 {
            [0.0, 1.0, 0.0, 1.0]
        } else {
            [0.0, 0.5, 1.0, 1.0]
        };
        boxes.push(block(color, Some([300.0, 40.0])));
    }

    let root = app.set_layout(win, 800.0, 600.0, column(boxes, 24.0, 16.0));
    let red = app.child(root, 0).expect("the pulse box exists");

    let big = Cell::new(false);
    TimeContext::add_timeout(
        app.world(),
        Duration::from_millis(PULSE_INTERVAL_MS),
        move |world| {
            let size = if big.replace(!big.get()) { SMALL } else { BIG };
            set_widget_size(world, red, size[0], size[1]);
            true
        },
    );

    let frames = Cell::new(0u64);
    let elapsed = Cell::new(Duration::ZERO);
    TimeContext::add_tick_callback(app.world(), move |_, delta| {
        frames.set(frames.get() + 1);
        let window = elapsed.get() + delta;

        if window >= Duration::from_secs(1) {
            let fps = frames.get() as f64 / window.as_secs_f64();
            println!("pulse stress frame rate: {fps:.1} FPS");
            frames.set(0);
            elapsed.set(Duration::ZERO);
        } else {
            elapsed.set(window);
        }

        true
    });

    app.run();
}

//! Pulses a box's size in a loop using a GLib-style repeating timeout.
//!
//! Every 750ms the red block flips between two sizes and the column re-resolves
//! around it via the standard dirtying pipeline: `set_widget_size` mutates the
//! `WidgetStyle` component → `sync_styles_to_taffy` dirties the mirror →
//! `layout_system` recomputes → `flatten_system` re-emits quads.
//! The example also prints the render-loop frame rate once per second.
//!
//! The timer is a plain [`TimeContext`](velvet::TimeContext) source, registered
//! GLib-style via the world (no `Application` method) — exactly as a system or
//! widget module would from inside a callback.
//!
//! Run under a Wayland compositor:
//!
//!     cargo run --example pulse

use std::cell::Cell;
use std::time::Duration;
use velvet::widgets::builder::{block, column};
use velvet::widgets::layout::set_widget_size;
use velvet::widgets::window::WindowDescriptor;
use velvet::Application;
use velvet::TimeContext;

const SMALL: [f32; 2] = [120.0, 80.0];
const BIG: [f32; 2] = [320.0, 200.0];

fn main() {
    let mut app = Application::new();
    let win = app.create_window(WindowDescriptor::default());

    // A column: a pulsing red block above a fixed green bar. The bar doesn't
    // change size, but it visibly re-positions as the column resolves.
    let root = app.set_layout(
        win,
        800.0,
        600.0,
        column(
            vec![
                block([1.0, 0.0, 0.0, 1.0], Some(SMALL)),
                block([0.0, 1.0, 0.0, 1.0], Some([300.0, 40.0])),
            ],
            24.0,
            16.0,
        ),
    );
    let red = app.child(root, 0).unwrap();

    // GLib's `g_timeout_add`: repeating until the callback returns `false`.
    let big = Cell::new(false);
    TimeContext::add_timeout(app.world(), Duration::from_millis(750), move |world| {
        if big.get() {
            set_widget_size(world, red, SMALL[0], SMALL[1]);
            big.set(false);
        } else {
            set_widget_size(world, red, BIG[0], BIG[1]);
            big.set(true);
        }
        true // keep repeating
    });

    // A tick callback runs once per render-loop iteration. Count those frames
    // over the measured elapsed time so terminal output stays useful even when
    // individual frame times vary.
    let frames = Cell::new(0u64);
    let elapsed = Cell::new(Duration::ZERO);
    TimeContext::add_tick_callback(app.world(), move |_, delta| {
        frames.set(frames.get() + 1);
        let window = elapsed.get() + delta;

        if window >= Duration::from_secs(1) {
            let fps = frames.get() as f64 / window.as_secs_f64();
            println!("pulse frame rate: {fps:.1} FPS");
            frames.set(0);
            elapsed.set(Duration::ZERO);
        } else {
            elapsed.set(window);
        }

        true // keep measuring
    });

    app.run();
}

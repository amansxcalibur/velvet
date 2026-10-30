use velvet::widgets::builder::{block, column, row};
use velvet::widgets::window::WindowDescriptor;
use velvet::Application;

fn main() {
    let mut app = Application::new();

    let win1 = app.create_window(WindowDescriptor::default());
    let win2 = app.create_window(WindowDescriptor::default());

    // Window 1: a column with a red block and a row of translucent blue / green blocks.
    let ui1 = column(
        vec![
            block([1.0, 0.0, 0.0, 1.0], Some([100.0, 100.0])), // Red
            row(
                vec![
                    block([0.0, 0.0, 1.0, 0.8], Some([120.0, 60.0])), // Translucent Blue
                    block([0.0, 1.0, 0.0, 1.0], Some([120.0, 60.0])), // Green
                ],
                8.0,
                4.0,
            ),
        ],
        16.0,
        20.0,
    );
    app.set_layout(win1, 800.0, 600.0, ui1);

    // Window 2: a wide green bar.
    let ui2 = column(
        vec![block([0.0, 1.0, 0.0, 1.0], Some([300.0, 50.0]))],
        0.0,
        0.0,
    );
    app.set_layout(win2, 800.0, 600.0, ui2);

    app.run();
}

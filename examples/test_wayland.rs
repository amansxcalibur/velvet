use velvet::Application;
use velvet::widgets::window::WindowDescriptor;
use velvet::render::quad::QuadInstance;

fn main() {
    let mut app = Application::new();

    let win1 = app.create_window(WindowDescriptor::default());
    let win2 = app.create_window(WindowDescriptor::default());

    app.set_window_quads(
        win1,
        vec![
            QuadInstance {
                position: [50.0, 50.0],
                size: [100.0, 100.0],
                color: [1.0, 0.0, 0.0, 1.0], // Red
            },
            QuadInstance {
                position: [100.0, 100.0],
                size: [100.0, 100.0],
                color: [0.0, 0.0, 1.0, 0.8], // Translucent Blue
            },
        ],
    );

    // Define the UI for Window 2 (A wide Green bar)
    app.set_window_quads(
        win2,
        vec![QuadInstance {
            position: [20.0, 20.0],
            size: [300.0, 50.0],
            color: [0.0, 1.0, 0.0, 1.0], // Green
        }],
    );

    app.run();
}

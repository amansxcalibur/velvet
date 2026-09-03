use velvet::Application;
use velvet::widgets::window::WindowDescriptor;

fn main() {
    let mut app = Application::new();

    app.create_window(WindowDescriptor::default());
    app.create_window(WindowDescriptor::default());

    app.run();
}

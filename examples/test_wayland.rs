use velvet::widgets::descriptor::WindowDescriptor;
use velvet::Application;

fn main() {
    let mut app = Application::new();

    app.create_window(WindowDescriptor::default());
    app.create_window(WindowDescriptor::default());

    app.run();
}

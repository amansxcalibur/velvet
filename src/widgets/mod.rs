pub mod descriptor;
pub mod window;
pub mod window_manager;

pub use descriptor::{WindowDescriptor, WindowRole};
pub use window::VelvetWindow;
pub use window_manager::WindowManager;

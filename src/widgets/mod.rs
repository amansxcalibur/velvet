pub mod builder;
pub mod flatten;
pub mod layout;
pub mod quad_buffers;
pub mod style;
pub mod window;

#[cfg(test)]
mod tests;

pub use builder::{Node, block, build_tree, column, row};
pub use flatten::flatten_system;
pub use layout::{
    WindowLayout, WindowLayouts, layout_system, set_widget_size, sync_styles_to_taffy,
};
pub use quad_buffers::QuadBuffers;
pub use style::{ComputedRect, WidgetStyle, WindowRoot, WindowSize};
pub use window::{VelvetWindow, WindowDescriptor, WindowRole};

//! `QuadBuffers` holds the flattened quad instances per window.
//!
//! It is the seam between the ECS widget layer and the renderer:
//! [`flatten_system`](super::flatten::flatten_system) rebuilds it every frame
//! and `render_all_gpu` reads from it, so the widget tree never has to mutate
//! `VelvetWindow` directly.

use crate::render::quad::QuadInstance;
use bevy_ecs::prelude::*;

/// Flattened quads per window, keyed by window id.
#[derive(Resource, Default)]
pub struct QuadBuffers {
    pub windows: std::collections::HashMap<usize, Vec<QuadInstance>>,
}

//! Renders the laid-out widget tree down to the flat list of quad instances
//! understood by the GPU pipeline.
//!
//! `QuadBuffers` holds the flattened [`QuadInstance`]s for every window, keyed
//! by window id. It is the seam between the ECS widget layer and the renderer:
//! `render_all_gpu` reads from here every frame, so the widget tree never has
//! to mutate `VelvetWindow`.

use super::quad_buffers::QuadBuffers;
use super::style::{ComputedRect, WidgetStyle, WindowRoot};
use crate::render::quad::QuadInstance;
use bevy_ecs::prelude::*;

/// Walks every window's widget tree, accumulating relative `ComputedRect`
/// values into absolute pixel positions, and emits one quad per widget with a
/// background color.
pub fn flatten_system(world: &mut World) {
    // Snapshot the roots first: the query borrows `world`, but the walk below
    // needs it mutable.
    let mut roots: Vec<(usize, Entity)> = vec![];
    for (entity, root) in world.query::<(Entity, &WindowRoot)>().iter(world) {
        roots.push((root.0, entity));
    }

    let mut quads: Vec<(usize, QuadInstance)> = vec![];
    for (window_id, entity) in roots {
        for quad in collect_quads(world, entity, [0.0, 0.0]) {
            quads.push((window_id, quad));
        }
    }

    // Move them into the QuadBuffers resource.
    let mut buffers = world.resource_mut::<QuadBuffers>();
    buffers.windows.clear();
    for (window_id, quad) in quads {
        if let Some(list) = buffers.windows.get_mut(&window_id) {
            list.push(quad);
        } else {
            buffers.windows.insert(window_id, vec![quad]);
        }
    }
}

fn collect_quads(
    world: &mut World,
    entity: Entity,
    parent_pos: [f32; 2],
) -> Vec<QuadInstance> {
    let mut out: Vec<QuadInstance> = vec![];

    if let Some(rect) = world.get::<ComputedRect>(entity) {
        let position = [
            parent_pos[0] + rect.position[0],
            parent_pos[1] + rect.position[1],
        ];

        if let Some(style) = world.get::<WidgetStyle>(entity)
            && let Some(background) = style.background
        {
            out.push(QuadInstance {
                position,
                size: rect.size,
                color: background,
            });
        }

        if let Some(children) = world.get::<Children>(entity) {
            // Copy the child ids out: we can't hold the immutable borrow while
            // recursing (which needs `&mut world`).
            let mut child_ids: Vec<Entity> = vec![];
            for child in children {
                child_ids.push(*child);
            }
            for child in child_ids {
                for quad in collect_quads(world, child, position) {
                    out.push(quad);
                }
            }
        }
    }

    out
}

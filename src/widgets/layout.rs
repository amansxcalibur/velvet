//! Per-window layout state and systems.
//!
//! [`WindowLayout`] holds the persistent taffy mirror for a window. It lives in
//! the [`WindowLayouts`] resource so that taffy's dirty-tracking / caching can
//! persist across frames: a static UI is laid out once and then only recomputed
//! when something is explicitly marked dirty.

use std::collections::HashMap;

use crate::widgets::WidgetStyle;

use super::style::{ComputedRect, WindowRoot, WindowSize};
use bevy_ecs::prelude::*;
use log::debug;
use taffy::prelude::{AvailableSpace, NodeId, Size};
use taffy::TaffyTree;

/// Newtype wrapper around a `TaffyTree` so the persistent mirror can live in a
/// bevy resource.
///
/// SAFETY: `TaffyTree` is explicitly single-threaded (`!Send`/`!Sync`). Bevy
/// systems that touch this resource run on the main thread and never move the
/// tree across threads, so opting in is sound. Same pattern as `DisplayPtr`.
pub struct TaffyMirror(pub TaffyTree<Entity>);
unsafe impl Send for TaffyMirror {}
unsafe impl Sync for TaffyMirror {}

/// The persistent taffy mirror for a single window.
pub struct WindowLayout {
    pub tree: TaffyMirror,
    pub root: NodeId,
    /// The window size used for the last layout computation, if any.
    pub size: Option<[f32; 2]>,
    pub entity_to_node: HashMap<Entity, NodeId>,
}

/// All window layouts in the world, keyed by window id.
///
/// Inserted explicitly via `world.insert_resource` (no `Default` impl since
/// [`WindowLayout`] builds its `TaffyTree` incrementally).
#[derive(Resource)]
pub struct WindowLayouts {
    pub windows: std::collections::HashMap<usize, WindowLayout>,
}

/// Recomputes the layout of every window that needs it and writes the result
/// back into each widget's [`ComputedRect`] component.
///
/// The layout pass is skipped for windows whose taffy mirror is already clean
/// (nothing was marked dirty and the window size didn't change).
pub fn layout_system(world: &mut World) {
    // Snapshot the viewport sizes first: the root query borrows `world`, but
    // we can't hold that borrow while mutating the layouts resource.
    let mut roots: Vec<(usize, f32, f32)> = vec![];
    for (_, root, size) in world
        .query::<(Entity, &WindowRoot, &WindowSize)>()
        .iter(world)
    {
        roots.push((root.0, size.width, size.height));
    }

    for (window_id, width, height) in roots {
        let mut layouts = world.resource_mut::<WindowLayouts>();
        if let Some(layout) = layouts.windows.get_mut(&window_id) {
            let resized = match layout.size {
                Some([w, h]) => w != width || h != height,
                None => true,
            };
            if resized || layout.tree.0.dirty(layout.root).unwrap() {
                _ = layout.tree.0.compute_layout(
                    layout.root,
                    Size {
                        width: AvailableSpace::Definite(width),
                        height: AvailableSpace::Definite(height),
                    },
                );
                layout.size = Some([width, height]);
            }
        } else {
            debug!("layout_system: no WindowLayout for window {window_id:?}");
        }
    }

    write_back_computed_rects(world);
}

/// Copies the taffy layouts (which are relative to the parent, same as
/// `ComputedRect`) onto their linked ECS entities.
///
/// Runs every frame; it is a cheap read of the cached taffy layouts.
fn write_back_computed_rects(world: &mut World) {
    // Phase 1: snapshot rects while holding a read-only borrow of the layouts.
    let mut rects: Vec<(Entity, [f32; 2], [f32; 2])> = vec![];
    {
        let layouts = world.resource::<WindowLayouts>();
        for layout in layouts.windows.values() {
            for rect in collect_rects(&layout.tree.0, layout.root) {
                rects.push(rect);
            }
        }
    }

    // Phase 2: write the snapshots into the ECS world.
    for (entity, position, size) in rects {
        if let Some(mut rect) = world.get_mut::<ComputedRect>(entity) {
            rect.position = position;
            rect.size = size;
        }
    }
}

/// Recursively reads `Layout` data + the entity id stored in each node's
/// context, returning one `(entity, position, size)` per node.
fn collect_rects(tree: &TaffyTree<Entity>, node: NodeId) -> Vec<(Entity, [f32; 2], [f32; 2])> {
    let mut out: Vec<(Entity, [f32; 2], [f32; 2])> = vec![];

    if let (Some(entity), Ok(layout)) = (tree.get_node_context(node), tree.layout(node)) {
        out.push((
            *entity,
            [layout.location.x, layout.location.y],
            [layout.size.width, layout.size.height],
        ));
    }
    for child in tree.children(node).unwrap() {
        for rect in collect_rects(tree, child) {
            out.push(rect);
        }
    }

    out
}

/// Pushes widget style changes from the ECS world into the persistent taffy
/// mirrors, so the next [`layout_system`] pass sees them as dirty.
///
/// Snapshot-then-write, same pattern as [`layout_system`]: the `Changed` query
/// borrows `world` immutably, and we can't hold that while mutating resources.
///
/// Note: `set_style` calls `mark_dirty` internally, so this system alone turns
/// a component mutation into a taffy relayout on the next pass.
pub fn sync_styles_to_taffy(world: &mut World) {
    let mut changed: Vec<(Entity, taffy::Style)> = vec![];
    for (entity, widget_style) in world
        .query_filtered::<(Entity, &WidgetStyle), Changed<WidgetStyle>>()
        .iter(world)
    {
        changed.push((entity, widget_style.style.0.clone()));
    }

    let mut layouts = world.resource_mut::<WindowLayouts>();
    for (entity, style) in changed {
        for layout in layouts.windows.values_mut() {
            if let Some(&node_id) = layout.entity_to_node.get(&entity) {
                let _ = layout.tree.0.set_style(node_id, style);
                break;
            }
        }
    }
}

/// Public style-mutation API: resizes a widget by rewriting its
/// [`WidgetStyle`] component. The next [`sync_styles_to_taffy`] pass dirties
/// the taffy mirror and [`layout_system`] resolves the new geometry around it.
/// This is the ECS-first way to drive dynamic UI: touch the component, let the
/// systems react.
pub fn set_widget_size(world: &mut World, entity: Entity, width: f32, height: f32) {
    if let Some(mut style) = world.get_mut::<WidgetStyle>(entity) {
        style.style.0.size = Size::from_lengths(width, height);
    }
}

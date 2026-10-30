//! Declarative widget tree builder.
//!
//! Builders are the *only* way to create widgets. They keep the ECS world and
//! the taffy mirror in sync by constructing both from the same [`Node`] tree:
//! each widget gets an ECS entity (with components) *and* a taffy node whose
//! context is the entity id. This is the "single-creator invariant" that keeps
//! the two trees from drifting apart.

use std::collections::HashMap;

use super::layout::{TaffyMirror, WindowLayout};
use super::style::{ComputedRect, TaffyStyle, WidgetStyle, WindowRoot, WindowSize};
use bevy_ecs::prelude::*;
use taffy::prelude::{length, zero, Display, FlexDirection, LengthPercentage, NodeId, Rect, Size};
use taffy::TaffyTree;

/// A widget tree node. Construct with [`block`], [`column`] and [`row`].
pub struct Node {
    pub style: taffy::Style,
    pub children: Vec<Node>,
    pub background: Option<[f32; 4]>,
}

/// A solid-color rectangle leaf. `size` is optional; when `None` the block
/// sizes itself from the available space (flex behavior).
pub fn block(color: [f32; 4], size: Option<[f32; 2]>) -> Node {
    let mut style = taffy::Style::default();
    if let Some(size) = size {
        style.size = Size::from_lengths(size[0], size[1]);
    }
    Node {
        style,
        children: Vec::new(),
        background: Some(color),
    }
}

/// A horizontal flex container.
pub fn row(children: Vec<Node>, gap: f32, padding: f32) -> Node {
    container(Display::Flex, FlexDirection::Row, children, gap, padding)
}

/// A vertical flex container.
pub fn column(children: Vec<Node>, gap: f32, padding: f32) -> Node {
    container(Display::Flex, FlexDirection::Column, children, gap, padding)
}

/// The gap between children must land on the container's *main* axis
/// (horizontal for a `row`, vertical for a `column`) — taffy resolves the gap
/// from `Size::main(direction)`.
fn container(
    display: Display,
    direction: FlexDirection,
    children: Vec<Node>,
    gap: f32,
    padding: f32,
) -> Node {
    let pad = length(padding as f64);
    let gap = length(gap as f64);
    let style = taffy::Style {
        display,
        flex_direction: direction,
        gap: match direction {
            FlexDirection::Column => Size {
                width: zero::<LengthPercentage>(),
                height: gap,
            },
            _ => Size {
                width: gap,
                height: zero::<LengthPercentage>(),
            },
        },
        padding: Rect {
            left: pad,
            right: pad,
            top: pad,
            bottom: pad,
        },
        ..Default::default()
    };
    Node {
        style,
        children,
        background: None,
    }
}

/// Builds the full widget tree for a window: spawns the ECS entities (with
/// `ChildOf` hierarchy), builds the persistent taffy mirror, and returns the
/// [`WindowLayout`] to be stored in the [`WindowLayouts` resource](super::layout::WindowLayouts).
pub fn build_tree(
    world: &mut World,
    window_id: usize,
    width: f32,
    height: f32,
    node: Node,
) -> WindowLayout {
    let mut tree = TaffyTree::new();
    let mut entity_to_node = HashMap::new();
    let (_, root_node) = build(
        world,
        &mut tree,
        &mut entity_to_node,
        None,
        Some((window_id, width, height)),
        node,
    );
    WindowLayout {
        tree: TaffyMirror(tree),
        root: root_node,
        size: None,
        entity_to_node: entity_to_node,
    }
}

fn build(
    world: &mut World,
    tree: &mut TaffyTree<Entity>,
    entity_to_node: &mut HashMap<Entity, NodeId>,
    parent: Option<Entity>,
    window: Option<(usize, f32, f32)>,
    node: Node,
) -> (Entity, NodeId) {
    // The style lives in both places: cloned into the taffy mirror and owned by
    // the ECS component.
    let mirror_style = node.style.clone();
    let header = WidgetStyle {
        style: TaffyStyle(node.style),
        background: node.background,
    };

    // Spawn the ECS entity (with the parent relationship when it is not the root).
    let entity = match window {
        Some((window_id, width, height)) => world
            .spawn((
                header,
                ComputedRect::default(),
                WindowRoot(window_id),
                WindowSize { width, height },
            ))
            .id(),
        None => world
            .spawn((header, ComputedRect::default(), ChildOf(parent.unwrap())))
            .id(),
    };

    // Mirror it into the taffy tree with the entity id as the node context.
    let node_id = tree.new_leaf_with_context(mirror_style, entity).unwrap();
    entity_to_node.insert(entity, node_id);

    // recursively build children
    let mut child_ids: Vec<NodeId> = vec![];
    for child in node.children {
        let (_, child_id) = build(world, tree, entity_to_node, Some(entity), None, child);
        child_ids.push(child_id);
    }
    // enforce hierarchy in taffy tree
    if !child_ids.is_empty() {
        _ = tree.set_children(node_id, &child_ids);
    }

    (entity, node_id)
}

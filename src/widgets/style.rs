//! Widget components.
//!
//! The ECS world is the source of truth for the widget tree:
//! - each widget is an entity,
//! - the tree structure is the `ChildOf`/`Children` relationship pair,
//! - all widget state is stored in components.
//!
//! Taffy computes the actual layout; it mirrors the ECS tree and writes the
//! result back into [`ComputedRect`] (which is relative to the parent, matching
//! how taffy reports layout).

use bevy_ecs::prelude::*;
use taffy::Style;

/// Newtype wrapper around a `taffy::Style` so it can live in a bevy component.
///
/// SAFETY: `taffy::Style` is explicitly single-threaded (it contains a phantom
/// `*const ()` internally). Bevy never moves components across threads in this
/// application, so opting in is sound. Same pattern as `TaffyMirror`.
#[derive(Clone, Debug, PartialEq)]
pub struct TaffyStyle(pub Style);
unsafe impl Send for TaffyStyle {}
unsafe impl Sync for TaffyStyle {}

/// All layout/visual style data for a widget, wrapping the `taffy::Style`.
///
/// The optional `background` color is what turns a widget into a rendered quad
/// (anything without a background is invisible, e.g. a pure layout container).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct WidgetStyle {
    pub style: TaffyStyle,
    pub background: Option<[f32; 4]>,
}

/// The computed layout of a widget, written by `layout_system` each frame.
///
/// `position` is relative to the parent widget's top-left corner
/// (the same convention taffy's `Layout::location` uses).
#[derive(Component, Copy, Clone, Debug, Default)]
pub struct ComputedRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
}

/// Marks the root widget entity of a window's widget tree, carrying the window id.
#[derive(Component, Copy, Clone, Debug)]
pub struct WindowRoot(pub usize);

/// The pixel size of the window's drawable area (the taffy "viewport").
#[derive(Component, Copy, Clone, Debug)]
pub struct WindowSize {
    pub width: f32,
    pub height: f32,
}

//! Tests for the ECS → taffy dirtying / recompute pipeline.
//!
//! These run headless: `build_tree` + the layout/flatten systems only touch a
//! bevy [`World`] and the [`WindowLayouts`] / [`QuadBuffers`] resources, so no
//! Wayland connection or GPU is required. They exercise the exact same code
//! path the schedule runs every frame (see `Application::run`).

use crate::time::TimeContext;
use crate::widgets::layout::WindowLayouts;
use crate::widgets::*;
use bevy_ecs::prelude::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};

const WINDOW: usize = 1;

fn example_tree() -> Node {
    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 0.8];
    let green = [0.0, 1.0, 0.0, 1.0];

    column(
        vec![
            block(red, Some([100.0, 100.0])),
            row(
                vec![
                    block(blue, Some([120.0, 60.0])),
                    block(green, Some([120.0, 60.0])),
                ],
                8.0,
                4.0,
            ),
        ],
        16.0,
        8.0,
    )
}

/// Builds a world with the layout resources and the example tree installed.
fn build_world() -> World {
    let mut world = World::new();
    world.init_resource::<QuadBuffers>();
    world.insert_resource(WindowLayouts {
        windows: HashMap::new(),
    });
    let layout = build_tree(&mut world, WINDOW, 800.0, 600.0, example_tree());
    world
        .resource_mut::<WindowLayouts>()
        .windows
        .insert(WINDOW, layout);
    world
}

/// The entity id of the root widget (the one carrying `WindowRoot`).
fn root_entity(world: &mut World) -> Entity {
    let mut root: Option<Entity> = None;
    for (entity, _) in world.query::<(Entity, &WindowRoot)>().iter(world) {
        root = Some(entity);
    }
    root.expect("no WindowRoot entity")
}

/// The root entity's `ComputedRect.size`, as written by `layout_system`.
fn root_size(world: &mut World) -> [f32; 2] {
    let root = root_entity(world);
    world.get::<ComputedRect>(root).unwrap().size
}

fn block_rect(world: &mut World, color: [f32; 4]) -> (Entity, [f32; 2], [f32; 2]) {
    for (entity, rect, style) in world
        .query::<(Entity, &ComputedRect, &WidgetStyle)>()
        .iter(world)
    {
        if style.background == Some(color) {
            return (entity, rect.position, rect.size);
        }
    }
    panic!("no widget with the requested background color")
}

/// Rewrites a widget's requested size through its `WidgetStyle` component,
/// mirroring what the public `set_widget_size` API does.
fn set_block_size(world: &mut World, color: [f32; 4], w: f32, h: f32) {
    let entity = block_rect(world, color).0;
    set_widget_size(world, entity, w, h);
}

/// True when the stored taffy mirror for [`WINDOW`] reports the root as dirty,
/// i.e. a relayout is pending.
fn taffy_dirty(world: &mut World) -> bool {
    let layout = world
        .resource::<WindowLayouts>()
        .windows
        .get(&WINDOW)
        .unwrap();
    layout.tree.0.dirty(layout.root).unwrap()
}

#[test]
fn initial_layout_is_computed() {
    let mut world = build_world();
    layout_system(&mut world);

    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 0.8];
    let green = [0.0, 1.0, 0.0, 1.0];

    // Sizes come straight from the requested widget styles.
    assert_eq!(block_rect(&mut world, red).2, [100.0, 100.0]);
    assert_eq!(block_rect(&mut world, blue).2, [120.0, 60.0]);
    assert_eq!(block_rect(&mut world, green).2, [120.0, 60.0]);

    // Column padding is 8px, so the red block sits at the content origin.
    assert_eq!(block_rect(&mut world, red).1, [8.0, 8.0]);

    // Positions are relative to the parent (the docs for `ComputedRect`). The
    // row's own position is [8, 124] relative to the root; its children are
    // inset by the row's 4px padding. Green follows blue + the 8px row gap,
    // which the builder wires onto the row's main (horizontal) axis.
    assert_eq!(block_rect(&mut world, blue).1, [4.0, 4.0]);
    assert_eq!(block_rect(&mut world, green).1, [132.0, 4.0]);

    // Root = 16px column padding + max child width; column grows with content.
    assert_eq!(root_size(&mut world), [272.0, 200.0]);

    // A fresh mirror starts dirty and is clean after the first pass.
    assert!(!taffy_dirty(&mut world));
}

#[test]
fn clean_tree_skips_recompute() {
    let mut world = build_world();
    layout_system(&mut world);

    let before = block_rect(&mut world, [0.0, 0.0, 1.0, 0.8]).2;
    let root_before = root_size(&mut world);

    layout_system(&mut world);

    // The layout was already cached: rects are unchanged and taffy reports no
    // pending work, so the second pass had nothing to recompute.
    assert_eq!(block_rect(&mut world, [0.0, 0.0, 1.0, 0.8]).2, before);
    assert_eq!(root_size(&mut world), root_before);
    assert!(!taffy_dirty(&mut world));
}

#[test]
fn style_mutation_dirties_and_relayouts() {
    let mut world = build_world();
    layout_system(&mut world);

    let blue = [0.0, 0.0, 1.0, 0.8];
    let blue_entity = block_rect(&mut world, blue).0;
    set_block_size(&mut world, blue, 120.0, 120.0);

    // Taffy doesn't know yet: the ECS component changed, but nothing was synced.
    assert!(!taffy_dirty(&mut world));

    // Push the change into the mirror, exactly as the schedule's
    // `sync_styles_to_taffy` step does.
    sync_styles_to_taffy(&mut world);
    assert!(taffy_dirty(&mut world));

    // Recompute. The blue block grew taller; the row and column grow with it.
    layout_system(&mut world);
    assert!(!taffy_dirty(&mut world));

    let (entity, _, size) = block_rect(&mut world, blue);
    assert_eq!(entity, blue_entity);
    assert_eq!(size, [120.0, 120.0]);
    // The row grew to 120 + 8px padding; the green block keeps its own size.
    assert_eq!(
        block_rect(&mut world, [0.0, 1.0, 0.0, 1.0]).2,
        [120.0, 60.0]
    );
    // Column height = 100 + 16 gap + 128 row + 16 padding.
    assert_eq!(root_size(&mut world), [272.0, 260.0]);
}

#[test]
fn background_change_updates_quads() {
    let mut world = build_world();
    layout_system(&mut world);
    flatten_system(&mut world);

    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 0.8];
    let new_color = [0.0, 1.0, 1.0, 1.0];
    let (red_entity, red_pos, red_size) = block_rect(&mut world, red);

    // Recolor the block without touching its size.
    world.get_mut::<WidgetStyle>(red_entity).unwrap().background = Some(new_color);
    sync_styles_to_taffy(&mut world);
    layout_system(&mut world);
    flatten_system(&mut world);

    let quads = world
        .resource::<QuadBuffers>()
        .windows
        .get(&WINDOW)
        .unwrap();
    let mut recolored = 0;
    let mut stale_red = 0;
    for quad in quads {
        if quad.color == new_color {
            recolored += 1;
            assert_eq!(quad.position, red_pos);
            assert_eq!(quad.size, red_size);
        }
        if quad.color == red {
            stale_red += 1;
        }
        if quad.color == blue {
            assert_eq!(quad.size, [120.0, 60.0]);
        }
    }
    assert_eq!(recolored, 1);
    assert_eq!(stale_red, 0);
}

#[test]
fn window_resize_recomputes() {
    let mut world = build_world();
    layout_system(&mut world);
    assert!(!taffy_dirty(&mut world));

    // Grow the window viewport; relayout is forced by the resize branch.
    let root = root_entity(&mut world);
    world.get_mut::<WindowSize>(root).unwrap().width = 1200.0;
    world.get_mut::<WindowSize>(root).unwrap().height = 900.0;
    layout_system(&mut world);

    let layouts = world.resource::<WindowLayouts>();
    assert_eq!(
        layouts.windows.get(&WINDOW).unwrap().size,
        Some([1200.0, 900.0])
    );
    assert!(!taffy_dirty(&mut world));

    // Changing the viewport back recomputes too.
    world.get_mut::<WindowSize>(root).unwrap().width = 800.0;
    world.get_mut::<WindowSize>(root).unwrap().height = 600.0;
    layout_system(&mut world);

    let layouts = world.resource::<WindowLayouts>();
    assert_eq!(
        layouts.windows.get(&WINDOW).unwrap().size,
        Some([800.0, 600.0])
    );
}

// ---------------------------------------------------------------------------
// TimeContext (GLib-style time sources) — deterministic via dispatch_at / next_timeout_at.

fn build_time_world() -> World {
    let mut world = World::new();
    world.init_resource::<TimeContext>();
    world
}

#[test]
fn timeout_fires_repeats_and_can_stop() {
    let mut world = build_time_world();
    let t0 = Instant::now();

    let count = Arc::new(Cell::new(0));
    let keep = count.clone();
    // Keep repeating while the count is below 3.
    let id = TimeContext::add_timeout(&mut world, Duration::from_secs(10), move |_| {
            count.deref().set(count.deref().get() + 1);
            count.deref().get() < 3
        });

    // Nothing before the first deadline.
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(5), false);
    assert_eq!(keep.deref().get(), 0);

    // After the first deadline: fires, then keeps rescheduling.
    let mut t = t0 + Duration::from_secs(20);
    TimeContext::dispatch_at(&mut world, t, false);
    assert_eq!(keep.deref().get(), 1);

    t = t + Duration::from_secs(15);
    TimeContext::dispatch_at(&mut world, t, false);
    assert_eq!(keep.deref().get(), 2);

    // Third fire returns false: the source removes itself.
    t = t + Duration::from_secs(15);
    TimeContext::dispatch_at(&mut world, t, false);
    assert_eq!(keep.deref().get(), 3);
    assert!(!TimeContext::remove_source(&mut world, id));
}

#[test]
fn timeout_once_fires_single() {
    let mut world = build_time_world();
    let t0 = Instant::now();

    let count = Arc::new(Cell::new(0));
    let keep = count.clone();
    TimeContext::add_timeout_once(&mut world, Duration::from_secs(10), move |_| {
            count.deref().set(count.deref().get() + 1);
        });

    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(5), false);
    assert_eq!(keep.deref().get(), 0);

    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(20), false);
    assert_eq!(keep.deref().get(), 1);

    // The one-shot never fires again.
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(60), false);
    assert_eq!(keep.deref().get(), 1);
    assert_eq!(world.resource::<TimeContext>().timeouts_len(), 0);
}

#[test]
fn tick_callback_runs_every_dispatch_and_can_stop() {
    let mut world = build_time_world();
    let t0 = Instant::now();

    let count = Arc::new(Cell::new(0));
    let keep_count = count.clone();
    let delta_sum = Arc::new(Cell::new(Duration::ZERO));
    let keep_delta = delta_sum.clone();
    let id = TimeContext::add_tick_callback(&mut world, move |_, delta| {
            count.deref().set(count.deref().get() + 1);
            delta_sum
                .deref()
                .set(delta_sum.deref().get().saturating_add(delta));
            count.deref().get() < 3
        });

    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(1), false);
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(2), false);
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(3), false);

    assert_eq!(keep_count.deref().get(), 3);
    // Deltas are the time since the *previous* dispatch.
    assert!(keep_delta.deref().get() > Duration::ZERO);
    assert!(!TimeContext::remove_source(&mut world, id));
}

#[test]
fn idle_runs_only_when_loop_was_idle() {
    let mut world = build_time_world();
    let t0 = Instant::now();

    let count = Arc::new(Cell::new(0));
    let keep = count.clone();
    TimeContext::add_idle(&mut world, move |_| {
        count.deref().set(count.deref().get() + 1);
        true
    });

    // A busy iteration (events were processed) skips idle callbacks.
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(1), false);
    assert_eq!(keep.deref().get(), 0);

    // An idle iteration runs them.
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(2), true);
    assert_eq!(keep.deref().get(), 1);
}

#[test]
fn remove_source_unregisters() {
    let mut world = build_time_world();

    let id = TimeContext::add_timeout(&mut world, Duration::from_secs(10), |_| true);
    assert!(TimeContext::remove_source(&mut world, id));
    assert!(!TimeContext::remove_source(&mut world, id));

    let t0 = Instant::now();
    TimeContext::dispatch_at(&mut world, t0 + Duration::from_secs(30), false);
    assert_eq!(world.resource::<TimeContext>().timeouts_len(), 0);
}

#[test]
fn next_timeout_guides_the_loop() {
    let mut world = build_time_world();
    let t0 = Instant::now();

    // Nothing scheduled -> the loop may block forever.
    assert!(TimeContext::next_timeout(&mut world).is_none());

    // A timeout far ahead -> a bounded wait around the deadline.
    TimeContext::add_timeout(&mut world, Duration::from_secs(60), |_| true);
    let wait = world
        .resource::<TimeContext>()
        .next_timeout_at(t0 + Duration::from_secs(10))
        .unwrap();
    assert!(wait > Duration::ZERO);
    // The deadline is `add_time + 60s`, so ~50s remain at t0 + 10s.
    assert!(wait <= Duration::from_secs(60));

    // A tick callback -> ~16.6ms cadence, regardless of deadlines.
    TimeContext::add_tick_callback(&mut world, |_, _| true);
    let tick_wait = world
        .resource::<TimeContext>()
        .next_timeout_at(t0 + Duration::from_secs(10))
        .unwrap();
    assert!(tick_wait <= Duration::from_micros(16_666));
}

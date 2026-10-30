# Velvet Widget & Layout System

This document describes the widget architecture added to Velvet: an ECS widget
tree (built with `bevy_ecs`) laid out by **taffy** (the Bevy UI flexbox/grid
engine), and rendered down to the wgpu quad pipeline that already existed.

## Design at a glance

```
 User code (builder DSL)
   │  column(...)/row(...)/block(...)            src/widgets/builder.rs
   ▼
 ECS world (source of truth)          ┌─────────────────────────────┐
   • widgets  = entities              │  WidgetStyle (taffy Style)  │
   • tree     = ChildOf/Children      │  ComputedRect (derived)     │
   • per-window root = WindowRoot     │  WindowSize (viewport)      │
   └──────────────────────────────────┴─────────────────────────────┘
        │  layout_system (every frame, src/widgets/layout.rs)
        ▼
 WindowLayouts resource (persistent taffy mirror per window)
   • TaffyTree<Entity>  — node context = ECS entity id
   • compute_layout() only when dirty / resized  (taffy dirty-tracking)
   • write taffy Layout back into each widget's ComputedRect
        │  flatten_system (every frame, src/widgets/flatten.rs)
        ▼
 QuadBuffers resource   ──>   render_all_gpu  ──>   wgpu quad pipeline
```

Two design principles drive everything:

1. **ECS is the source of truth.** Widget identity, hierarchy and state live in
   the `bevy_ecs` world as entities/components. Systems (`layout_system`,
   `flatten_system`) are the only logic.
2. **Taffy is a pure derivation.** The taffy tree is a *mirror* of the ECS tree,
   owned by the layout layer and persisted across frames so that taffy's
   internal dirty-tracking/caching works.

## ECS model (`src/widgets/style.rs`)

| Component | Purpose |
|---|---|
| `WidgetStyle { style: TaffyStyle, background: Option<[f32; 4]> }` | Wraps the full `taffy::Style` (flexbox/grid/block). `background` is what turns a widget into a visible quad; widgets without one (e.g. pure containers) are invisible |
| `ComputedRect { position, size }` | Layout output, written every frame by `layout_system`. `position` is **relative to the parent** (matches taffy's `Layout::location`) |
| `WindowRoot(usize)` | Marks the root widget entity of a window's tree; value is the window id |
| `WindowSize { width, height }` | The window's drawable size — the taffy "viewport" fed to `compute_layout` |

Hierarchy uses bevy's canonical relationship pair: child entities carry a
`ChildOf(parent)` component and `Children` is auto-maintained on parents.

## Taffy integration (`src/widgets/layout.rs`)

Each window has one persistent mirror:

```rust
pub struct WindowLayout {
    pub tree: TaffyMirror,   // TaffyTree<Entity>
    pub root: NodeId,
    pub size: Option<[f32; 2]>,
}
```

- `TaffyTree<Entity>` uses the **entity id as the node context**, giving a
  node → entity mapping for free when writing results back.
- `layout_system` skips `compute_layout` unless the tree is dirty or the window
  size changed. This is the *whole point* of keeping the tree alive: a static
  UI is laid out once, then only re-measured on invalidations.

### Send/Sync wrappers (important)

`taffy::Style` and `taffy::TaffyTree` are explicitly single-threaded
(`!Send`/`!Sync`), but bevy 0.18 requires components and resources to be
`Send + Sync`. They are wrapped in local marker types that opt in with
`unsafe impl Send` / `unsafe impl Sync`:

- `TaffyStyle(pub taffy::Style)` — for the `WidgetStyle` component
- `TaffyMirror(pub TaffyTree<Entity>)` — for the `WindowLayouts` resource

**Safety rationale:** bevy systems that touch these run on the application's
main thread and never move the values across threads; the `*const ()` phantom
inside `CompactLength` carries no real data. This mirrors the existing
`DisplayPtr` pattern in `src/widgets/window.rs`.

## Per-frame pipeline

Inside `Application::run` (src/app.rs), after pending wayland events are
drained and bridged into ECS, the schedule runs:

1. `message_update_system` — flush message buffers.
2. `process_window_closes` — destroy windows on `WindowCloseRequested`.
3. `layout_system.after(process_window_closes)` — recompute + write-back rects.
4. `flatten_system.after(layout_system)` — accumulate **absolute** pixel quads
   into `QuadBuffers`.

`render_all_gpu` (src/window_manager.rs) then reads `QuadBuffers.windows[id]`
and feeds the same `GpuSurface::render` → wgpu path as before. `VelvetWindow`
no longer stores quads at all.

Borrow-note: ECS query iterators hold an immutable borrow on `world`, so the
systems snapshot root entities/rects into `Vec`s first, then mutate resources —
that keeps `&mut World` use sequential and conflict-free.

## Builder DSL (`src/widgets/builder.rs`)

Builders are the **single-creator** of widgets: they build the ECS entities
*and* the taffy mirror from the same `Node` tree in one recursion, so the two
trees can't drift apart.

```rust
let ui = column(
    vec![
        block([1.0, 0.0, 0.0, 1.0], Some([100.0, 100.0])),
        row(vec![
            block([0.0, 0.0, 1.0, 0.8], Some([120.0, 60.0])),
            block([0.0, 1.0, 0.0, 1.0], Some([120.0, 60.0])),
        ], 8.0, 4.0),
    ],
    16.0, 8.0,
);
app.set_layout(win1, 800.0, 600.0, ui);
```

## Style mutation & dirtying

Style changes flow one way: **ECS component → taffy mirror**, driven entirely by
bevy's change detection.

1. A system (or user code) mutates a widget's `WidgetStyle` component
   (`world.get_mut::<WidgetStyle>(entity)`).
2. `sync_styles_to_taffy` (runs before `layout_system` every frame) queries
   `Changed<WidgetStyle>`, finds the matching node via `entity_to_node`, and
   calls `taffy::set_style` — which *also* calls `mark_dirty`, propagating
   "dirty" up to the root.
3. `layout_system` checks `tree.dirty(root)`: only then does it call
   `compute_layout` and write fresh `ComputedRect`s back.

`WindowSize` changes take a separate, value-compared branch in `layout_system`
(the `resized` check), so resizes never need the sync step.

Structural changes (add/remove widget, despawn) are **not** supported yet — they
would need to update `entity_to_node`, the taffy children, and either spawn or
clean up `ComputedRect`. Despawn in particular leaves dangling `entity_to_node`
entries (bevy reuses entity ids), so removal must remove the taffy node too.

`sync_styles_to_taffy` and `layout_system` are plain `fn(world: &mut World)`
functions, so the whole dirty→recompute loop is unit-testable headless (no
Wayland/GPU): see `src/widgets/tests.rs` (dirtying, skip-when-clean, quad
re-emit, resize). Run with `cargo test`.

The public mutation API is `set_widget_size(world, entity, w, h)` (resize a
widget via its component — `Application::set_layout` returns the root entity,
`Application::child(parent, i)` walks to children).

For time-driven UI there is **no per-frame hook in `Application`** and **no
binding to it either**. A standalone GLib-style source model lives in
`src/time.rs` — [`TimeContext`](velvet::TimeContext), a bevy resource. Like
`g_idle_add()`, every source-registration function takes the world directly,
so any system, callback or widget module holding a `&mut World` can schedule:

```rust
TimeContext::add_timeout(world, Duration::from_secs(1), move |world| { …; true });
TimeContext::add_timeout_once(world, delay, |world| { … });
TimeContext::add_tick_callback(world, move |world, delta| { …; true });
TimeContext::add_idle(world, move |world| { …; true });
TimeContext::remove_source(world, id);
```

(`Application::world()` hands out the world only for bootstrapping in `main()`.)
The run loop merely pumps the resource — each iteration it asks how long it may
sleep ([`TimeContext::next_timeout`]) and dispatches what is due
([`TimeContext::dispatch_pending`]) before the ECS schedule, so widget
mutations made inside a callback are laid out on the same frame. When nothing
is scheduled the loop still blocks indefinitely on the compositor (zero CPU).
See `examples/pulse.rs` (repeating timeout) and `examples/idle_add.rs`
(timeout + tick + idle).

Note: `row`/`column` wire their `gap` onto the container's *main* axis
(horizontal for rows, vertical for columns) — that's what taffy's
`Size::main(direction)` expects. The very first version put the gap on the
height axis for both, which silently dropped row gaps.

Future dynamic mutations (`add_widget`/`remove_widget`/`set_style`) must go
through APIs that update both trees + `mark_dirty`, preserving the single-creator
invariant.

## File map

| File | Contents |
|---|---|
| `src/widgets/style.rs` | Components (`WidgetStyle`, `ComputedRect`, `WindowRoot`, `WindowSize`) + `TaffyStyle` wrapper |
| `src/widgets/layout.rs` | `TaffyMirror`, `WindowLayout`, `WindowLayouts` resource, `layout_system`, write-back, `sync_styles_to_taffy`, `set_widget_size` |
| `src/widgets/quad_buffers.rs` | `QuadBuffers` resource |
| `src/widgets/flatten.rs` | `flatten_system`, quad collection |
| `src/widgets/builder.rs` | `Node` DSL, `block`/`column`/`row`, `build_tree` |
| `src/time.rs` | GLib-style `TimeContext`/`SourceId` source model (timeout/once/tick/idle/remove_source), world-based |

## Next steps (M2+)

- `set_style` / `set_size` widget handles that update both trees and `mark_dirty`.
- `taffy::Layout.order` as a z-index in `flatten_system` — honors z-order today.
- Text/image leaves via `compute_layout_with_measure` (node context already exists).
- Rich window resize → update `WindowSize` component + `mark_dirty` on root.
- Wayland input (`wl_seat`) → `Events<PointerMove>` → hit-test via `ComputedRect`
  → `Hoverable`/`Clickable` components.
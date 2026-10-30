//! GLib-style main-context time sources, decoupled from [`Application`](crate::app::Application).
//!
//! Models GLib's `GMainContext` / `GSource` model as a plain bevy
//! [`Resource`](bevy_ecs::prelude::Resource):
//!
//! | Velvet                    | GLib equivalent                      |
//! |---------------------------|---------------------------------------|
//! | [`TimeContext::add_timeout`]   | `g_timeout_add` (repeating)      |
//! | [`TimeContext::add_timeout_once`] | `g_timeout_add_once`         |
//! | [`TimeContext::add_idle`]   | `g_idle_add`                         |
//! | [`TimeContext::add_tick_callback`] | `gtk_widget_add_tick_callback` |
//! | [`TimeContext::remove_source`]    | `g_source_remove`             |
//! | `next_timeout` / `dispatch_pending` | main-context `prepare` / `iterate` |
//!
//! Registration is *not* bound to `Application`: like `g_idle_add()`, every
//! function takes the world directly, so any system, callback or widget module
//! that holds a `&mut World` can schedule sources —
//!
//! ```ignore
//! TimeContext::add_idle(&mut world, |_| { … });
//! ```
//!
//! Nothing here knows about windows, widgets or layouts. The event loop is the
//! only integrator: each iteration it asks how long it may sleep (see
//! [`TimeContext::next_timeout`]) and pumps whatever is due (see
//! [`TimeContext::dispatch_pending`]). There is no per-frame hook machinery or
//! animation special-casing in `Application` — a *timeout* that returns `true`
//! simply keeps rescheduling itself, exactly like GLib.
//!
//! ## Threading safety
//!
//! Callbacks are erased closures (`Box<dyn FnMut>`), so they are neither `Send`
//! nor `Sync`. The resource opts in with `unsafe impl` — the same rationale as
//! `TaffyMirror`: everything runs on the main thread inside the single event
//! loop and sources are never moved across threads.

use bevy_ecs::prelude::*;
use std::collections::HashMap;
use std::mem;
use std::time::{Duration, Instant};

/// Opaque handle to a registered source, for use with [`TimeContext::remove_source`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(u64);

type TimerCallback = Box<dyn FnMut(&mut World) -> bool + 'static>;
type TickCallback = Box<dyn FnMut(&mut World, Duration) -> bool + 'static>;

/// A repeating `g_timeout_add` source. Returns `true` from the callback to
/// keep the timeout alive.
struct TimeoutSource {
    interval: Duration,
    next_deadline: Instant,
    callback: TimerCallback,
}

/// A `gtk_widget_add_tick_callback` source: runs every frame with the time
/// since the previous tick. Returns `false` to stop.
struct TickSource {
    callback: TickCallback,
}

/// A `g_idle_add` source: runs while the loop has nothing else to do.
/// Returns `false` to unregister.
struct IdleSource {
    callback: TimerCallback,
}

/// All time sources for the app, stored as a bevy resource.
///
/// Registered via `world.init_resource::<TimeContext>()` (uses `Default`).
#[derive(Resource)]
pub struct TimeContext {
    next_source_id: u64,
    timeouts: HashMap<SourceId, TimeoutSource>,
    ticks: HashMap<SourceId, TickSource>,
    idles: HashMap<SourceId, IdleSource>,
    last_tick: Instant,
}

// SAFETY: single-threaded main loop only; see module docs.
unsafe impl Send for TimeContext {}
unsafe impl Sync for TimeContext {}

impl Default for TimeContext {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            next_source_id: 1,
            timeouts: HashMap::new(),
            ticks: HashMap::new(),
            idles: HashMap::new(),
            last_tick: now,
        }
    }
}

impl TimeContext {
    fn next_id(&mut self) -> SourceId {
        let id = SourceId(self.next_source_id);
        self.next_source_id += 1;
        id
    }

    /// Registers a per-frame callback (GLib's `gtk_widget_add_tick_callback`).
    ///
    /// The callback receives the elapsed time since the previous tick. Return
    /// `false` to remove the callback, matching GTK.
    pub fn add_tick_callback<F>(world: &mut World, callback: F) -> SourceId
    where
        F: FnMut(&mut World, Duration) -> bool + 'static,
    {
        let mut time = world.resource_mut::<TimeContext>();
        let id = time.next_id();
        time.ticks.insert(
            id,
            TickSource {
                callback: Box::new(callback),
            },
        );
        id
    }

    /// Registers a repeating timer (GLib's `g_timeout_add`).
    ///
    /// Fires once every `interval`, starting `interval` from now. Return `true`
    /// from the callback to keep repeating, `false` to stop.
    pub fn add_timeout<F>(world: &mut World, interval: Duration, callback: F) -> SourceId
    where
        F: FnMut(&mut World) -> bool + 'static,
    {
        let mut time = world.resource_mut::<TimeContext>();
        let id = time.next_id();
        let next_deadline = Instant::now() + interval;
        time.timeouts.insert(
            id,
            TimeoutSource {
                interval,
                next_deadline,
                callback: Box::new(callback),
            },
        );
        id
    }

    /// Registers a single-shot timer (GLib's `g_timeout_add_once`).
    pub fn add_timeout_once<F>(world: &mut World, delay: Duration, mut callback: F) -> SourceId
    where
        F: FnMut(&mut World) + 'static,
    {
        Self::add_timeout(world, delay, move |world| {
            callback(world);
            false
        })
    }

    /// Registers a callback that runs while the loop is idle (GLib's `g_idle_add`).
    ///
    /// Fires every frame in which no Wayland events were dispatched. Return
    /// `false` to remove the callback.
    pub fn add_idle<F>(world: &mut World, callback: F) -> SourceId
    where
        F: FnMut(&mut World) -> bool + 'static,
    {
        let mut time = world.resource_mut::<TimeContext>();
        let id = time.next_id();
        time.idles.insert(
            id,
            IdleSource {
                callback: Box::new(callback),
            },
        );
        id
    }

    /// Removes a source by id (GLib's `g_source_remove`). Returns `true` if a
    /// source was removed.
    pub fn remove_source(world: &mut World, id: SourceId) -> bool {
        let mut time = world.resource_mut::<TimeContext>();
        time.timeouts.remove(&id).is_some()
            || time.ticks.remove(&id).is_some()
            || time.idles.remove(&id).is_some()
    }

    /// Number of registered interval timeouts (debug / test helper).
    pub fn timeouts_len(&self) -> usize {
        self.timeouts.len()
    }

    /// How long the event loop may sleep before it must dispatch again
    /// (main-context `prepare`).
    ///
    /// - `None` — nothing scheduled; the loop can block forever on I/O.
    /// - `Some(d)` — at most `d` (usually the earliest timeout deadline; ~16ms
    ///   when tick or idle callbacks are active, keeping animations at ~60Hz).
    pub fn next_timeout(world: &mut World) -> Option<Duration> {
        world
            .resource::<TimeContext>()
            .next_timeout_at(Instant::now())
    }

    /// [`Self::next_timeout`] with an explicit `now` (deterministic in tests).
    pub fn next_timeout_at(&self, now: Instant) -> Option<Duration> {
        // Tick and idle sources want a steady cadence rather than "asap".
        if !self.ticks.is_empty() || !self.idles.is_empty() {
            let target = Duration::from_micros(16_666);
            let elapsed = now.saturating_duration_since(self.last_tick);
            return Some(target.saturating_sub(elapsed));
        }

        // Otherwise wait until the earliest timeout deadline (or forever).
        let mut found = false;
        let mut nearest = Duration::ZERO;
        for src in self.timeouts.values() {
            let wait = src.next_deadline.saturating_duration_since(now);
            if !found || wait < nearest {
                nearest = wait;
                found = true;
            }
        }
        if found {
            Some(nearest)
        } else {
            None
        }
    }

    /// Dispatches due sources (main-context `iterate`). Call once per loop
    /// iteration with `was_idle` = whether the loop had no events to process.
    pub fn dispatch_pending(world: &mut World, was_idle: bool) {
        Self::dispatch_at(world, Instant::now(), was_idle)
    }

    /// [`Self::dispatch_pending`] with an explicit `now` (deterministic in tests).
    pub fn dispatch_at(world: &mut World, now: Instant, was_idle: bool) {
        // Take the source maps out of the resource first, so callbacks may
        // safely register/remove sources reentrantly: the resource guard is not
        // held while they run. Survivors are merged back afterwards.
        let (mut timeouts, mut ticks, mut idles, mut last_tick) = {
            let mut time = world.resource_mut::<TimeContext>();
            (
                mem::take(&mut time.timeouts),
                mem::take(&mut time.ticks),
                mem::take(&mut time.idles),
                time.last_tick,
            )
        };

        let delta = now.saturating_duration_since(last_tick);
        last_tick = now;

        // tick callbacks run every frame with the frame delta.
        ticks.retain(|_, src| (src.callback)(world, delta));

        // idle callbacks run only when the loop had nothing else to do.
        if was_idle {
            idles.retain(|_, src| (src.callback)(world));
        }

        // interval timeouts fire when their deadline has passed; returning
        // `true` reschedules them `interval` from now.
        timeouts.retain(|_, src| {
            if now >= src.next_deadline {
                let keep = (src.callback)(world);
                if keep {
                    src.next_deadline = now + src.interval;
                }
                keep
            } else {
                true
            }
        });

        // Merge the survivors back, preserving sources added during dispatch.
        let mut time = world.resource_mut::<TimeContext>();
        time.timeouts.extend(timeouts);
        time.ticks.extend(ticks);
        time.idles.extend(idles);
        time.last_tick = last_tick;
    }
}

//! GTK-style naming for Bevy's built-in, entity-targeted observer events.
//!
//! This module adds no event queue or callback registry. Bevy owns signal
//! delivery, observer scheduling, and observer cleanup when entities despawn.
use bevy_ecs::{
    bundle::Bundle,
    event::EntityEvent,
    system::{EntityCommands, IntoObserverSystem},
};

/// Adds a `connect` spelling for attaching a Bevy observer to a widget entity.
///
/// Signal types must derive [`EntityEvent`]. Emit them using Bevy's
/// `Commands::trigger` or `World::trigger` APIs. Add this trait to scope to use
/// `.connect(...)` on `EntityCommands`.
pub trait ConnectSignal {
    /// Attach an observer for an event targeted at this entity.
    fn connect<E: EntityEvent, B: Bundle, M>(
        &mut self,
        observer: impl IntoObserverSystem<E, B, M>,
    ) -> &mut Self;
}

impl ConnectSignal for EntityCommands<'_> {
    fn connect<E: EntityEvent, B: Bundle, M>(
        &mut self,
        observer: impl IntoObserverSystem<E, B, M>,
    ) -> &mut Self {
        self.observe(observer)
    }
}

//! VM identity + the `VmTag` component used to scope entities to a single VM.

use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a [`crate::VmInstance`]. Process-wide, monotonic.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub struct VmId(pub u64);

impl VmId {
    /// Allocate a fresh, never-before-used [`VmId`].
    #[must_use]
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// Component automatically attached to every entity a VM spawns into the
/// shared world. Host functions filter queries by it; unloading a VM
/// despawns every entity carrying its tag.
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct VmTag {
    /// VM that owns this entity.
    pub vm: VmId,
}

impl Default for VmTag {
    fn default() -> Self {
        Self { vm: VmId(0) }
    }
}

impl VmTag {
    /// Tag entities for `vm`.
    #[must_use]
    pub fn new(vm: VmId) -> Self {
        Self { vm }
    }
}

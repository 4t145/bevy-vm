//! Bevy-facing half of the resource bridge: trait, build context, and cache.
//!
//! Only compiled with the `bevy-bridge` feature; the builder enums themselves
//! (`ImageBuilder` / `MeshBuilder` / `MaterialBuilder`) live one level up
//! and are pure data, available even without Bevy.

use crate::resource::CacheKey;
use bevy::asset::{Asset, AssetServer, Assets, Handle};
use bevy::image::Image;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use std::any::TypeId;
use std::collections::HashMap;

/// Trait implemented by every resource builder enum.
///
/// `cache_key` should produce the same value for two builder instances that
/// describe the same logical resource (so the cache can dedupe). Hashing
/// the builder's serde representation works in most cases.
pub trait ResourceBuilder {
    /// Bevy asset type produced by [`Self::build`].
    type Resource: Asset;
    /// Build (or reuse via cache) the asset and return a Bevy `Handle`.
    fn build(&self, ctx: &mut BuildContext<'_>) -> Handle<Self::Resource>;
    /// Stable cache key — same logical resource ⇒ same key.
    fn cache_key(&self) -> CacheKey<Self::Resource>;
}

/// System-param-like aggregate the sync layer hands to `build()`.
pub struct BuildContext<'a> {
    /// Filesystem / async asset server for `server.load("path")` style.
    pub server: &'a AssetServer,
    /// Mesh asset storage for inserting procedural meshes.
    pub meshes: &'a mut Assets<Mesh>,
    /// Material asset storage for inserting `StandardMaterial`s built from
    /// nested PBR specs.
    pub materials: &'a mut Assets<StandardMaterial>,
    /// Image asset storage (`Assets<Image>`); kept for symmetry, although
    /// most image flows go through `server.load()`.
    pub images: &'a mut Assets<Image>,
    /// Cache: lookup before build, insert after build.
    pub cache: &'a mut ResourceCache,
}

/// Per-world cache mapping `CacheKey<R>` to `Handle<R>`, by erased asset type.
///
/// Keyed by `(TypeId::of::<R>, key.bits())`; lookups are typed via the
/// generic methods so callers never hand-roll the type id.
#[derive(Resource, Default)]
pub struct ResourceCache {
    handles: HashMap<(TypeId, u64), ErasedHandle>,
}

impl ResourceCache {
    /// Look up a previously-built handle, returning `None` on miss.
    pub fn get<R: Asset>(&self, key: CacheKey<R>) -> Option<Handle<R>> {
        let entry = self.handles.get(&(TypeId::of::<R>(), key.bits()))?;
        entry.downcast::<R>()
    }

    /// Insert (or overwrite) a handle for the given key.
    pub fn insert<R: Asset>(&mut self, key: CacheKey<R>, handle: Handle<R>) {
        self.handles
            .insert((TypeId::of::<R>(), key.bits()), ErasedHandle::new(handle));
    }

    /// Convenience: look up by key, or build via the closure and insert.
    /// Both `get` and `build` share the same allocation life-cycle.
    pub fn get_or_build_with<R: Asset>(
        &mut self,
        key: CacheKey<R>,
        build: impl FnOnce(&mut Self) -> Handle<R>,
    ) -> Handle<R> {
        if let Some(handle) = self.get(key) {
            return handle;
        }
        let handle = build(self);
        self.insert(key, handle.clone());
        handle
    }
}

/// Type-erased Bevy handle. Stored as `Box<dyn Any>` and downcast on read.
struct ErasedHandle {
    inner: Box<dyn std::any::Any + Send + Sync>,
}

impl ErasedHandle {
    fn new<R: Asset>(handle: Handle<R>) -> Self {
        Self {
            inner: Box::new(handle),
        }
    }

    fn downcast<R: Asset>(&self) -> Option<Handle<R>> {
        self.inner.downcast_ref::<Handle<R>>().cloned()
    }
}

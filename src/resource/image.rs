//! Image resource builder.
//!
//! Today this only knows how to load by asset path; in the future it can
//! grow `InlineBytes`, `Procedural`, etc. variants without breaking
//! consumers, since the enum is `#[serde(tag = "kind")]` and existing
//! variant names stay stable.

use bevy_reflect::Reflect;

#[cfg(feature = "bevy-bridge")]
use crate::resource::{BuildContext, CacheKey, ResourceBuilder};
#[cfg(feature = "bevy-bridge")]
use bevy::asset::Handle;
#[cfg(feature = "bevy-bridge")]
use bevy::image::Image;
#[cfg(feature = "bevy-bridge")]
use std::hash::{DefaultHasher, Hash, Hasher};

/// How to obtain an image asset.
///
/// reflect 序列化形态：`{"AssetPath": {"path": "textures/grass.png"}}`。
#[derive(Reflect, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImageBuilder {
    /// Load from `AssetServer` using a path string (e.g. `"textures/grass.png"`).
    AssetPath {
        /// Asset path (relative to the configured asset source).
        path: String,
    },
}

#[cfg(feature = "bevy-bridge")]
impl ResourceBuilder for ImageBuilder {
    type Resource = Image;

    fn build(&self, ctx: &mut BuildContext<'_>) -> Handle<Image> {
        match self {
            Self::AssetPath { path } => ctx.server.load(path.clone()),
        }
    }

    fn cache_key(&self) -> CacheKey<Image> {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        CacheKey::from_bits(hasher.finish())
    }
}

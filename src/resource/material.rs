//! Material resource builder.
//!
//! Today supports two flavors:
//! - [`MaterialBuilder::AssetPath`] — load a material asset from disk.
//! - [`MaterialBuilder::Pbr`] — describe a PBR material inline; the sync
//!   layer constructs a Bevy `StandardMaterial` from the [`PbrMaterial`]
//!   spec.
//!
//! [`PbrMaterial`] currently exposes the most commonly tweaked subset of
//! `StandardMaterial`'s fields. Adding more is a matter of widening the
//! struct + the conversion in `build()`.

use crate::resource::image::ImageBuilder;
use bevy_color::{Color, LinearRgba};
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;
use std::hash::{Hash, Hasher};

#[cfg(feature = "bevy-bridge")]
use crate::resource::{BuildContext, CacheKey, ResourceBuilder};
#[cfg(feature = "bevy-bridge")]
use bevy::asset::Handle;
#[cfg(feature = "bevy-bridge")]
use bevy::pbr::StandardMaterial;
#[cfg(feature = "bevy-bridge")]
use bevy::prelude::*;
#[cfg(feature = "bevy-bridge")]
use std::hash::DefaultHasher;

/// How to obtain a material asset.
///
/// reflect 序列化形态：variant 是外层 key，例如
/// `{"AssetPath": {"path": "..."}}` 或 `{"Pbr": {"base_color": ..., ...}}`。
#[derive(Reflect, Debug, Clone, PartialEq)]
pub enum MaterialBuilder {
    /// Load from an asset path (e.g. `"materials/metal.toml"`).
    AssetPath { path: String },
    /// Inline PBR description.
    Pbr(PbrMaterial),
}

/// Inline PBR material spec — flattened to the `StandardMaterial` fields
/// most often tweaked from configs/scripts.
#[derive(Reflect, Debug, Clone, PartialEq)]
#[reflect(Default)]
pub struct PbrMaterial {
    /// Surface base color. Defaults to white.
    pub base_color: Color,
    /// Optional base color texture (multiplies `base_color`).
    pub base_color_texture: Option<ImageBuilder>,
    /// Optional normal map texture.
    pub normal_map_texture: Option<ImageBuilder>,
    /// Self-emissive color (linear RGB; values can exceed 1.0 for HDR).
    pub emissive: LinearRgba,
    /// 0..=1, dielectric → metallic.
    pub metallic: f32,
    /// 0..=1 perceptual roughness. Bevy clamps to ≥ 0.089.
    pub roughness: f32,
    /// Specular intensity for non-metals (0..=1). Bevy default 0.5.
    pub reflectance: f32,
    /// Index of refraction. Bevy default 1.5 (glass).
    pub ior: f32,
    /// 0..=1; how cleared the surface is (separate clear-coat layer).
    pub clearcoat: f32,
    /// 0..=1 perceptual roughness of the clear-coat layer.
    pub clearcoat_roughness: f32,
}

impl Default for PbrMaterial {
    fn default() -> Self {
        Self {
            base_color: Color::WHITE,
            base_color_texture: None,
            normal_map_texture: None,
            emissive: LinearRgba::BLACK,
            metallic: 0.0,
            roughness: 0.5,
            reflectance: 0.5,
            ior: 1.5,
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
        }
    }
}

impl Hash for MaterialBuilder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::AssetPath { path } => path.hash(state),
            Self::Pbr(pbr) => pbr.hash(state),
        }
    }
}

impl Hash for PbrMaterial {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Color/LinearRgba 没有 Hash——把它们的内部 LinearRgba 字段 to_bits 哈希。
        let lin = self.base_color.to_linear();
        for f in [lin.red, lin.green, lin.blue, lin.alpha] {
            f.to_bits().hash(state);
        }
        self.base_color_texture.hash(state);
        self.normal_map_texture.hash(state);
        for f in [
            self.emissive.red,
            self.emissive.green,
            self.emissive.blue,
            self.emissive.alpha,
        ] {
            f.to_bits().hash(state);
        }
        self.metallic.to_bits().hash(state);
        self.roughness.to_bits().hash(state);
        self.reflectance.to_bits().hash(state);
        self.ior.to_bits().hash(state);
        self.clearcoat.to_bits().hash(state);
        self.clearcoat_roughness.to_bits().hash(state);
    }
}

#[cfg(feature = "bevy-bridge")]
impl ResourceBuilder for MaterialBuilder {
    type Resource = StandardMaterial;

    fn build(&self, ctx: &mut BuildContext<'_>) -> Handle<StandardMaterial> {
        match self {
            Self::AssetPath { path } => ctx.server.load(path.clone()),
            Self::Pbr(pbr) => {
                let material = build_standard_material(pbr, ctx);
                ctx.materials.add(material)
            }
        }
    }

    fn cache_key(&self) -> CacheKey<StandardMaterial> {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        CacheKey::from_bits(hasher.finish())
    }
}

/// Convert a [`PbrMaterial`] spec into a Bevy `StandardMaterial` with the
/// nested texture builders resolved into `Handle<Image>`.
#[cfg(feature = "bevy-bridge")]
fn build_standard_material(spec: &PbrMaterial, ctx: &mut BuildContext<'_>) -> StandardMaterial {
    let base_color_texture = spec
        .base_color_texture
        .as_ref()
        .map(|builder| builder_to_handle(builder, ctx));
    let normal_map_texture = spec
        .normal_map_texture
        .as_ref()
        .map(|builder| builder_to_handle(builder, ctx));

    let alpha = spec.base_color.to_linear().alpha;

    let mut material = StandardMaterial {
        base_color: spec.base_color,
        base_color_texture,
        normal_map_texture,
        emissive: spec.emissive,
        metallic: spec.metallic,
        perceptual_roughness: spec.roughness,
        reflectance: spec.reflectance,
        ior: spec.ior,
        clearcoat: spec.clearcoat,
        clearcoat_perceptual_roughness: spec.clearcoat_roughness,
        ..Default::default()
    };
    if alpha < 1.0 {
        material.alpha_mode = AlphaMode::Blend;
    }
    material
}

/// Resolve an `ImageBuilder` through the resource cache, then look up (or
/// build) the underlying handle.
#[cfg(feature = "bevy-bridge")]
fn builder_to_handle(
    builder: &ImageBuilder,
    ctx: &mut BuildContext<'_>,
) -> Handle<bevy::image::Image> {
    let key = builder.cache_key();
    if let Some(handle) = ctx.cache.get(key) {
        return handle;
    }
    let handle = builder.build(ctx);
    ctx.cache.insert(key, handle.clone());
    handle
}

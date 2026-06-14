//! Mesh resource builder.
//!
//! Either an asset path (loaded via `AssetServer`) or one of Bevy's built-in
//! procedural primitives, dimensioned in world units.

use bevy_reflect::Reflect;
use std::hash::{Hash, Hasher};

#[cfg(feature = "bevy-bridge")]
use crate::resource::{BuildContext, CacheKey, ResourceBuilder};
#[cfg(feature = "bevy-bridge")]
use bevy::asset::Handle;
#[cfg(feature = "bevy-bridge")]
use bevy::math::primitives::{
    Capsule3d, ConicalFrustum, Cuboid, Cylinder, Plane3d, Sphere, Tetrahedron, Torus,
};
#[cfg(feature = "bevy-bridge")]
use bevy::prelude::Mesh;
#[cfg(feature = "bevy-bridge")]
use std::hash::DefaultHasher;

/// How to obtain a mesh asset — either loaded from disk or generated from
/// Bevy's primitive shape library.
///
/// Geometry-bearing variants are dimensioned in world units. Two builder
/// values that compare equal ⇒ same `cache_key` ⇒ Bevy mesh is built once
/// and shared.
///
/// `Hash` hashes f32 fields via `to_bits()`, while derived `PartialEq`
/// uses ordinary float equality — these disagree only on NaN, which has
/// no meaningful place in a mesh dimension. Keep f32 fields finite.
///
/// reflect 序列化形态：每个变体输出 `{"VariantName": {field: ..., ...}}`，
/// 例如 `{"Cube": {"size": [1.0, 1.0, 1.0]}}`。
#[derive(Reflect, Debug, Clone, PartialEq)]
pub enum MeshBuilder {
    /// Load from asset path (e.g. `"models/foo.glb#Mesh0/Primitive0"`).
    AssetPath { path: String },
    /// Cuboid / cube. `size` holds width/height/depth.
    Cube { size: [f32; 3] },
    /// Sphere with a single radius.
    Sphere { radius: f32 },
    /// Cylinder (capped). `radius` + `height`.
    Cylinder { radius: f32, height: f32 },
    /// Capsule3d. `radius` + `length` (cylinder section length, end caps add).
    Capsule { radius: f32, length: f32 },
    /// Cone. `radius` (base) + `height`.
    Cone { radius: f32, height: f32 },
    /// Conical frustum (truncated cone).
    ConicalFrustum {
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
    },
    /// Torus. `inner_radius` (tube) + `outer_radius` (overall).
    Torus {
        inner_radius: f32,
        outer_radius: f32,
    },
    /// Plane (XZ). `size` is the half-extents `[x_half, z_half]`.
    Plane { half_extents: [f32; 2] },
    /// Regular tetrahedron, parameterized by the bounding cube edge length.
    Tetrahedron { size: f32 },
}

impl Hash for MeshBuilder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::AssetPath { path } => path.hash(state),
            Self::Cube { size } => size.iter().for_each(|f| f.to_bits().hash(state)),
            Self::Sphere { radius } => radius.to_bits().hash(state),
            Self::Cylinder { radius, height } => {
                radius.to_bits().hash(state);
                height.to_bits().hash(state);
            }
            Self::Capsule { radius, length } => {
                radius.to_bits().hash(state);
                length.to_bits().hash(state);
            }
            Self::Cone { radius, height } => {
                radius.to_bits().hash(state);
                height.to_bits().hash(state);
            }
            Self::ConicalFrustum {
                radius_top,
                radius_bottom,
                height,
            } => {
                radius_top.to_bits().hash(state);
                radius_bottom.to_bits().hash(state);
                height.to_bits().hash(state);
            }
            Self::Torus {
                inner_radius,
                outer_radius,
            } => {
                inner_radius.to_bits().hash(state);
                outer_radius.to_bits().hash(state);
            }
            Self::Plane { half_extents } => {
                half_extents.iter().for_each(|f| f.to_bits().hash(state));
            }
            Self::Tetrahedron { size } => size.to_bits().hash(state),
        }
    }
}

#[cfg(feature = "bevy-bridge")]
impl ResourceBuilder for MeshBuilder {
    type Resource = Mesh;

    fn build(&self, ctx: &mut BuildContext<'_>) -> Handle<Mesh> {
        match self {
            Self::AssetPath { path } => ctx.server.load(path.clone()),
            Self::Cube { size: [x, y, z] } => ctx.meshes.add(Cuboid::new(*x, *y, *z)),
            Self::Sphere { radius } => ctx.meshes.add(Sphere::new(*radius)),
            Self::Cylinder { radius, height } => ctx.meshes.add(Cylinder::new(*radius, *height)),
            Self::Capsule { radius, length } => ctx.meshes.add(Capsule3d::new(*radius, *length)),
            Self::Cone { radius, height } => ctx.meshes.add(bevy::math::primitives::Cone {
                radius: *radius,
                height: *height,
            }),
            Self::ConicalFrustum {
                radius_top,
                radius_bottom,
                height,
            } => ctx.meshes.add(ConicalFrustum {
                radius_top: *radius_top,
                radius_bottom: *radius_bottom,
                height: *height,
            }),
            Self::Torus {
                inner_radius,
                outer_radius,
            } => ctx.meshes.add(Torus::new(*inner_radius, *outer_radius)),
            Self::Plane {
                half_extents: [hx, hz],
            } => ctx.meshes.add(Plane3d::new(
                bevy::math::Vec3::Y,
                bevy::math::Vec2::new(*hx, *hz),
            )),
            Self::Tetrahedron { size } => {
                let h = size * 0.5;
                ctx.meshes.add(Tetrahedron::new(
                    bevy::math::Vec3::new(h, h, h),
                    bevy::math::Vec3::new(h, -h, -h),
                    bevy::math::Vec3::new(-h, h, -h),
                    bevy::math::Vec3::new(-h, -h, h),
                ))
            }
        }
    }

    fn cache_key(&self) -> CacheKey<Mesh> {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        CacheKey::from_bits(hasher.finish())
    }
}

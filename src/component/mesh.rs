//! 3D mesh + material typed component.
//!
//! Counterpart of Bevy's `(Mesh3d, MeshMaterial3d<StandardMaterial>)`
//! component pair. VM consolidates them into one component for
//! script-friendliness: a tile / cube is "one thing", not two.

use crate::resource::material::MaterialBuilder;
use crate::resource::mesh::MeshBuilder;
use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

/// 3D 网格 + 材质——配合 [`crate::component::camera::Camera3d`] 渲染。
///
/// VM 把 Bevy 的 `Mesh3d(Handle<Mesh>)` + `MeshMaterial3d<StandardMaterial>(Handle)`
/// 合成单一组件——`mesh` 与 `material` 字段各自描述资源（builder），由同步
/// 层解析成 Bevy handles。这种合成对脚本作者更友好（"画一个红色立方体"
/// 是一条声明，不是两条）。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component, Default)]
pub struct Mesh3dRender {
    /// 网格资源描述（程序图元或资产路径）。
    pub mesh: MeshBuilder,
    /// 材质资源描述（资产路径或 PBR 内联）。
    pub material: MaterialBuilder,
}

impl Default for Mesh3dRender {
    fn default() -> Self {
        Self {
            mesh: MeshBuilder::Cube {
                size: [1.0, 1.0, 1.0],
            },
            material: default_material(),
        }
    }
}

fn default_material() -> MaterialBuilder {
    MaterialBuilder::Pbr(crate::resource::material::PbrMaterial::default())
}

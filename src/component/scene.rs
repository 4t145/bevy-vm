//! glTF scene typed component.
//!
//! Mirror of Bevy's `SceneRoot(Handle<Scene>)`. VM 端字段是资产路径，
//! 同步层加载后获得 `Handle<Scene>` 并 spawn `SceneRoot`。
//!
//! 场景内含的网格 / 材质 / 子实体都由 Bevy 的 scene 加载流水线处理——
//! VM 不递归展开它们。

use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

/// glTF / Bevy scene 的引用。
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[reflect(Component, Default)]
pub struct SceneRender {
    /// 场景资产路径，如 `"models/tree.glb#Scene0"`。
    pub asset: String,
}

//! Picking 相关的强类型组件。
//!
//! 当前只暴露一个开关型 [`Pickable`]：挂在 VM 实体上即启用 Bevy 的
//! mesh / sprite picking——具体后端由
//! [`crate::plugin::picking::PickingPlugin`] 在 Bevy 端配置。
//!
//! 不依赖 `bevy-bridge` feature 编译——纯逻辑层的 typed component，headless
//! 模拟也能挂（虽然此时没有渲染没人 pick）。

use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

/// 标记一个实体可被 picking 选中。
///
/// 当前没有字段，留作未来扩展（比如 `block_lower: bool` 控制堆叠遮挡）。
/// 单 World 架构下 VM 实体直接住在主世界，pick observer 拿 entity id
/// 即可，再通过 [`crate::VmTag`] 反查归属哪个 VM 实例。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Pickable;

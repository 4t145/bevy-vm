//! Picking 相关的强类型组件。
//!
//! 当前只暴露一个开关型 [`Pickable`]：挂在 VM 实体上，告诉渲染同步层"在
//! 这个实体的渲染镜像上启用 picking"——具体的 picking 后端（mesh / sprite）
//! 由 [`crate::plugin::picking::PickingPlugin`] 在 Bevy 端配置。
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
/// 渲染同步层见到这个组件就在镜像渲染实体上挂 Bevy 的 `Pickable` +
/// 一份 [`crate::render::VmEntityRef`] 反向引用，picking observer 据此
/// 把"哪个 VM 实体被 pick"转回 VM 端的 typed event。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Pickable;

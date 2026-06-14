//! 动态组件：运行时注册、无 Rust 类型、值为 [`Value`] 的组件。
//!
//! 这是「值系统」的内容层（统一的路线 Y）。每个动态组件在运行时通过
//! [`World::register_component_with_descriptor`] 注册，获得独立的
//! [`ComponentId`]——因而可被 `query` 按 archetype 分桶命中，不退化为线性扫描。
//! 组件的字节存储就是一个 [`Value`]（方案 A：值进 ECS，单一数据源，despawn 时
//! 由 ECS 自动 drop）。值类型采用 [`ron::Value`]，对 Rust 更友好（数值区分整数/
//! 浮点，支持 `Char`/`Option`/`Unit`，并可 `into_rust` 转强类型）。
//!
//! 本模块集中了全部与「类型擦除组件存储」相关的 `unsafe`，对外只暴露安全接口。

#[cfg(test)]
mod tests;

use bevy_ecs::component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType};
use bevy_ecs::entity::Entity;
use bevy_ecs::ptr::OwningPtr;
use bevy_ecs::world::World;
use serde_json::Value;
use std::alloc::Layout;

/// 一个已注册动态组件的元信息。
#[derive(Debug, Clone)]
pub struct DynComponent {
    /// 运行时注册得到的组件 id，`query` 与按 id 存取均用它。
    pub id: ComponentId,
    /// spawn 实体时写入的默认值模板。
    pub default: Value,
}

/// 在世界中注册一个以 [`Value`] 为存储的动态组件。
///
/// `name` 是该组件对配置与脚本可见的名字；`default` 是 spawn 时的初始值模板。
#[must_use]
pub fn register(world: &mut World, name: &str, default: Value) -> DynComponent {
    let descriptor = unsafe {
        // SAFETY:
        // - layout 为 `Value` 的布局，`drop` 以 `Value` 解释指针，二者一致。
        // - `Value: Send + Sync`，满足类型擦除组件的线程安全要求。
        // - 不涉及关系组件，relationship_accessor 为 None。
        ComponentDescriptor::new_with_layout(
            name.to_owned(),
            StorageType::Table,
            Layout::new::<Value>(),
            Some(drop_value),
            /* mutable */ true,
            ComponentCloneBehavior::Default,
            None,
        )
    };
    let id = world.register_component_with_descriptor(descriptor);
    DynComponent { id, default }
}

/// 把一个 [`Value`] 作为指定动态组件插入实体（覆盖同组件旧值）。
///
/// 调用方须保证 `id` 来自同一个 `world` 且确为 [`register`] 注册的动态组件。
pub fn insert(world: &mut World, entity: Entity, id: ComponentId, value: Value) {
    let mut entity_mut = world.entity_mut(entity);
    OwningPtr::make(value, |ptr| {
        // SAFETY: `id` 来自同一 world 且其布局为 `Value`；`ptr` 正是一个 `Value`，
        // 与该组件的布局/类型一致。
        unsafe {
            entity_mut.insert_by_id(id, ptr);
        }
    });
}

/// 读取实体上某动态组件的值。
///
/// 实体不存在该组件，或实体已销毁时返回 `None`。
#[must_use]
pub fn get(world: &World, entity: Entity, id: ComponentId) -> Option<&Value> {
    let entity_ref = world.get_entity(entity).ok()?;
    let ptr = entity_ref.get_by_id(id).ok()?;
    // SAFETY: 该组件以 `Value` 布局注册，存入的字节正是一个 `Value`。
    Some(unsafe { ptr.deref::<Value>() })
}

/// 获取实体上某动态组件值的可变引用。
///
/// 实体不存在该组件，或实体已销毁时返回 `None`。
#[must_use]
pub fn get_mut(world: &mut World, entity: Entity, id: ComponentId) -> Option<&mut Value> {
    let entity_mut = world.get_entity_mut(entity).ok()?;
    let mut_untyped = entity_mut.into_mut_by_id(id).ok()?;
    // SAFETY: 该组件以 `Value` 布局注册，存入的字节正是一个 `Value`。
    Some(unsafe { mut_untyped.into_inner().deref_mut::<Value>() })
}

/// 组件被移除/实体销毁时，以 `Value` 语义释放其字节。
///
/// # Safety
///
/// `ptr` 必须指向一个由本模块以 `Value` 布局写入的有效值。
unsafe fn drop_value(ptr: OwningPtr<'_>) {
    // SAFETY: 调用契约保证 `ptr` 是一个 `Value`。
    unsafe { ptr.drop_as::<Value>() };
}

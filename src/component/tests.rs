//! `requires` 机制的白盒测试：
//! - 注册 typed 组件时声明 `requires("X")`，set 该组件应自动连带 `X` 的 default。
//! - `validate_requires` 检测拼错的 require 名。
//! - `validate_requires` 检测依赖环。

use crate::component::{ComponentRegistry, Position, RegistryError, Velocity};
use crate::world_access;
use bevy_ecs::world::World;

#[test]
fn set_typed_component_auto_inserts_required_default() {
    let mut world = World::new();
    let registry = ComponentRegistry::with_builtins(&mut world);

    // Velocity requires Position；spawn 后只设 Velocity，Position 应被自动连带。
    let entity = world.spawn_empty().id();
    world_access::set(
        &mut world,
        &registry,
        entity,
        "Velocity",
        "x",
        serde_json::json!(2.0_f32),
    )
    .expect("set Velocity 应当成功");

    let position = world
        .entity(entity)
        .get::<Position>()
        .expect("Velocity requires Position：应自动挂上 Position::default");
    assert_eq!(position.x, 0.0, "自动连带应使用 Default 值");
    assert_eq!(position.y, 0.0);
    assert_eq!(position.z, 0.0);

    let velocity = world
        .entity(entity)
        .get::<Velocity>()
        .expect("set 的目标组件本身也要在");
    assert_eq!(velocity.x, 2.0);
}

#[test]
fn existing_required_component_is_not_overwritten() {
    let mut world = World::new();
    let registry = ComponentRegistry::with_builtins(&mut world);

    // 先把 Position 设成非默认值，再 set Velocity；Position 应保持原值。
    let entity = world.spawn_empty().id();
    world_access::set(
        &mut world,
        &registry,
        entity,
        "Position",
        "x",
        serde_json::json!(99.0_f32),
    )
    .expect("set Position 应当成功");
    world_access::set(
        &mut world,
        &registry,
        entity,
        "Velocity",
        "x",
        serde_json::json!(1.0_f32),
    )
    .expect("set Velocity 应当成功");

    let position = world
        .entity(entity)
        .get::<Position>()
        .expect("Position 仍存在");
    assert_eq!(position.x, 99.0, "已挂的 required 组件不应被自动连带覆盖");
}

#[test]
fn validate_requires_rejects_unknown_target() {
    let mut world = World::new();
    let mut registry = ComponentRegistry::with_builtins(&mut world);
    // 直接污染内部表，模拟"声明 require 了一个未注册的名字"。
    registry
        .typed
        .get_mut("Position")
        .expect("Position 必然已注册")
        .requires
        .push("DoesNotExist".to_owned());

    let err = registry
        .validate_requires()
        .expect_err("拼错的 require 名应被拒绝");
    match err {
        RegistryError::UnknownRequired {
            component,
            required,
        } => {
            assert_eq!(component, "Position");
            assert_eq!(required, "DoesNotExist");
        }
        other => panic!("期望 UnknownRequired，得到 {other:?}"),
    }
}

#[test]
fn validate_requires_detects_cycle() {
    let mut world = World::new();
    let mut registry = ComponentRegistry::with_builtins(&mut world);
    // 构造 Position -> Velocity -> Position 的环（Velocity 已经 requires Position；
    // 这里再让 Position 也 requires Velocity）。
    registry
        .typed
        .get_mut("Position")
        .expect("Position 必然已注册")
        .requires
        .push("Velocity".to_owned());

    let err = registry.validate_requires().expect_err("依赖环应被拒绝");
    assert!(
        matches!(err, RegistryError::RequiresCycle { .. }),
        "期望 RequiresCycle，得到 {err:?}"
    );
}

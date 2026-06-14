//! `requires` 机制的白盒测试。Position/Velocity 已删除，这里改用 Pickable
//! 自身（无 requires）+ 直接污染 typed 表来构造测试场景。

use crate::component::{ComponentRegistry, RegistryError};
use bevy_ecs::world::World;

#[test]
fn validate_requires_rejects_unknown_target() {
    let mut world = World::new();
    let mut registry = ComponentRegistry::with_builtins(&mut world);
    registry
        .typed
        .get_mut("Pickable")
        .expect("Pickable 必然已注册")
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
            assert_eq!(component, "Pickable");
            assert_eq!(required, "DoesNotExist");
        }
        other => panic!("期望 UnknownRequired，得到 {other:?}"),
    }
}

#[test]
fn validate_requires_detects_cycle() {
    let mut world = World::new();
    let mut registry = ComponentRegistry::with_builtins(&mut world);
    // Pickable -> Pickable 自环。
    registry
        .typed
        .get_mut("Pickable")
        .expect("Pickable 必然已注册")
        .requires
        .push("Pickable".to_owned());

    let err = registry.validate_requires().expect_err("依赖环应被拒绝");
    assert!(
        matches!(err, RegistryError::RequiresCycle { .. }),
        "期望 RequiresCycle，得到 {err:?}"
    );
}

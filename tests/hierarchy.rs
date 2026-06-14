//! Hierarchy bridge tests: VM 端的 `ChildOf` 关系操作。
//!
//! 渲染端的 sync_hierarchy 走的是 Bevy App，单独测试。这里只验证
//! VM World 内部、`world_access::set_parent` / `clear_parent` /
//! `parent_of` / `children_of` 的行为——即脚本 host 函数背后的实现。

use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::world::World;
use bevy_vm::world_access;

/// 模拟 VM World 的初始化：把 `ChildOf` 注册到 World。
fn fresh_world() -> World {
    let mut world = World::new();
    world.register_component::<ChildOf>();
    world
}

#[test]
fn set_parent_creates_child_of_and_children() {
    let mut world = fresh_world();
    let parent = world.spawn_empty().id();
    let child_a = world.spawn_empty().id();
    let child_b = world.spawn_empty().id();

    assert_eq!(world_access::parent_of(&world, child_a), None);
    assert!(world_access::children_of(&world, parent).is_empty());

    assert!(world_access::set_parent(&mut world, child_a, parent));
    assert!(world_access::set_parent(&mut world, child_b, parent));

    assert_eq!(world_access::parent_of(&world, child_a), Some(parent));
    assert_eq!(world_access::parent_of(&world, child_b), Some(parent));

    let mut kids = world_access::children_of(&world, parent);
    kids.sort_by_key(|e| e.to_bits());
    let mut expected = vec![child_a, child_b];
    expected.sort_by_key(|e| e.to_bits());
    assert_eq!(kids, expected);
}

#[test]
fn clear_parent_removes_relation() {
    let mut world = fresh_world();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();

    world_access::set_parent(&mut world, child, parent);
    assert_eq!(world_access::parent_of(&world, child), Some(parent));

    assert!(world_access::clear_parent(&mut world, child));
    assert_eq!(world_access::parent_of(&world, child), None);
    assert!(world_access::children_of(&world, parent).is_empty());
}

#[test]
fn set_parent_with_dead_entities_is_noop() {
    let mut world = fresh_world();
    let alive = world.spawn_empty().id();
    let ghost = world.spawn_empty().id();
    world.despawn(ghost);

    assert!(!world_access::set_parent(&mut world, alive, ghost));
    assert!(!world_access::set_parent(&mut world, ghost, alive));
    assert_eq!(world_access::parent_of(&world, alive), None);
}

/// Bevy 0.18 的 `ChildOf` 关系开启了 `linked_spawn` 语义——父被 despawn
/// 时整棵子树跟着被 despawn。验证 VM World 内的 register_component 让这条
/// 行为在 VM 这边也工作。
///
/// 这是场景图的自然语义：一个 UI 面板 despawn 时它的所有按钮文本一起消失。
/// 脚本作者要在父亡时保留子，需要先 `clear_parent` 再 `despawn(parent)`。
#[test]
fn despawning_parent_cascades_to_children() {
    let mut world = fresh_world();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    let grandchild = world.spawn_empty().id();

    world_access::set_parent(&mut world, child, parent);
    world_access::set_parent(&mut world, grandchild, child);

    world.despawn(parent);

    // 整棵子树都应已 despawn。
    assert!(world.get_entity(parent).is_err(), "parent 已 despawn");
    assert!(
        world.get_entity(child).is_err(),
        "linked_spawn：child 应被级联 despawn",
    );
    assert!(
        world.get_entity(grandchild).is_err(),
        "linked_spawn：grandchild 也被级联 despawn",
    );
}

/// 显式调 `clear_parent` 后再 despawn 父，子保留。
#[test]
fn clearing_parent_before_despawn_keeps_child() {
    let mut world = fresh_world();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();

    world_access::set_parent(&mut world, child, parent);
    world_access::clear_parent(&mut world, child);

    world.despawn(parent);

    assert!(world.get_entity(child).is_ok(), "已脱离父的 child 不被级联");
}

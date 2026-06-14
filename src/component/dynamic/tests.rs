//! 动态组件存储的白箱测试：注册 -> 插入 -> 读回 -> 改写 -> 查询 -> drop。

use super::{get, get_mut, insert, register};
use bevy_ecs::world::World;
use serde_json::Value;

fn value(text: &str) -> Value {
    serde_json::from_str(text).expect("test value should be valid JSON")
}

#[test]
fn register_insert_and_read_back() {
    let mut world = World::new();
    let health = register(&mut world, "Health", value(r#"{"value": 100.0}"#));
    let entity = world.spawn_empty().id();
    insert(&mut world, entity, health.id, value(r#"{"value": 30.0}"#));

    let stored = get(&world, entity, health.id).expect("应能读回动态组件值");
    assert_eq!(stored, &value(r#"{"value": 30.0}"#));
}

#[test]
fn get_mut_allows_in_place_edit() {
    let mut world = World::new();
    let health = register(&mut world, "Health", value(r#"{"value": 0.0}"#));
    let entity = world.spawn_empty().id();
    insert(&mut world, entity, health.id, value(r#"{"value": 10.0}"#));

    let stored = get_mut(&mut world, entity, health.id).expect("应能取到可变引用");
    *stored = value(r#"{"value": 7.0}"#);

    assert_eq!(
        get(&world, entity, health.id).expect("应能读回"),
        &value(r#"{"value": 7.0}"#)
    );
}

#[test]
fn missing_component_returns_none() {
    let mut world = World::new();
    let health = register(&mut world, "Health", value("{}"));
    let entity = world.spawn_empty().id();

    assert!(
        get(&world, entity, health.id).is_none(),
        "未插入该组件的实体应返回 None"
    );
}

#[test]
fn dynamic_component_is_queryable_by_id() {
    let mut world = World::new();
    let health = register(&mut world, "Health", value("{}"));
    let with_health = world.spawn_empty().id();
    insert(
        &mut world,
        with_health,
        health.id,
        value(r#"{"value": 1.0}"#),
    );
    world.spawn_empty();

    let mut query = bevy_ecs::prelude::QueryBuilder::<bevy_ecs::entity::Entity>::new(&mut world)
        .with_id(health.id)
        .build();
    let hits: Vec<_> = query.iter(&world).collect();

    assert_eq!(
        hits,
        vec![with_health],
        "query 应只命中挂有该动态组件的实体"
    );
}

#[test]
fn despawn_drops_value_without_leak() {
    // 用一个嵌套堆分配值，验证 despawn 触发 Value 的析构路径。
    let mut world = World::new();
    let bag = register(&mut world, "Bag", value("{}"));
    let entity = world.spawn_empty().id();
    insert(
        &mut world,
        entity,
        bag.id,
        value(r#"{"items": ["a", "b", "c"]}"#),
    );

    world.despawn(entity);

    assert!(
        get(&world, entity, bag.id).is_none(),
        "实体销毁后不应再读到其动态组件"
    );
}

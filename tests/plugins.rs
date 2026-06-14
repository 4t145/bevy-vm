//! plugin 加载器 + 命名空间基础设施测试。
//!
//! 主要场景：
//! - 文件夹入口（`tests/worlds/plugin_smoke/`）→ 自动找 world.ron
//! - 多 plugin 加载、拓扑排序、命名空间注册
//! - 跨 plugin 全限定引用（`tiles::Tile`）
//! - plugin 内短名解析（自家组件 + fallback 全局）
//! - 共享 helper.rhai 通过 import 复用

use bevy_vm::VmInstance;
use std::path::PathBuf;

fn smoke_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/worlds/plugin_smoke")
}

#[test]
fn folder_entry_loads_world_ron_with_plugins() {
    let mut world = bevy_ecs::world::World::new();
    let mut vm = VmInstance::load(&mut world, smoke_dir()).expect("load folder via world.ron");

    // 根 world 在 entities 段挂了一个 tiles::Tile，确认它被 spawn。
    let tiles_before_tick = vm.query(&mut world, "tiles::Tile");
    assert_eq!(
        tiles_before_tick.len(),
        1,
        "root world should spawn 1 tile entity",
    );

    // 跑一个 tick：hud.rhai 应执行——spawn 一个 marker 实体（带 hud::HudKind +
    // tiles::Tile），并把 root 那个 tile 的 value 加 1。
    vm.tick(&mut world).expect("tick");

    // 现在 tiles::Tile 应该有 2 个：root spawn 的 + hud 脚本 spawn 的 marker
    let tiles_after = vm.query(&mut world, "tiles::Tile");
    assert_eq!(tiles_after.len(), 2);

    // marker 实体的 HudKind.kind 应该是 "report"
    let hud_entities = vm.query(&mut world, "hud::HudKind");
    assert_eq!(hud_entities.len(), 1, "marker entity tagged with HudKind");
    let kind = vm
        .get(&world, hud_entities[0], "hud::HudKind", "kind")
        .expect("get HudKind.kind")
        .as_str()
        .unwrap_or("")
        .to_owned();
    assert_eq!(kind, "report");

    // marker 的 tiles::Tile.value 应该是 1（脚本写入）
    let marker_value = vm
        .get(&world, hud_entities[0], "tiles::Tile", "value")
        .expect("get marker value")
        .as_i64()
        .expect("i64");
    assert_eq!(marker_value, 1, "marker value 来自 helpers::increment()");

    // root 的原始 tile value 由 42 加 1 → 43
    let root_tile = tiles_after
        .iter()
        .copied()
        .find(|t| *t != hud_entities[0])
        .unwrap();
    let root_value = vm
        .get(&world, root_tile, "tiles::Tile", "value")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(root_value, 43, "root tile value: 42 + helpers::increment()");
}

#[test]
fn missing_dependency_reports_error() {
    use bevy_vm::VmError;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/worlds/plugin_missing_dep");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(
        dir.join("world.ron"),
        r#"(plugins: ["only"], entities: [])"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("only.ron"),
        r#"(dependencies: ["ghost"], entities: [])"#,
    )
    .unwrap();

    let result = { let mut world = bevy_ecs::world::World::new(); VmInstance::load(&mut world, &dir) };
    let Err(err) = result else {
        panic!("missing dependency should fail");
    };
    let VmError::PluginMissingDependency {
        ref plugin,
        ref missing,
    } = err
    else {
        panic!("got {err:?}");
    };
    assert_eq!(plugin, "only");
    assert_eq!(missing, "ghost");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dependency_cycle_reports_error() {
    use bevy_vm::VmError;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/worlds/plugin_cycle");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(
        dir.join("world.ron"),
        r#"(plugins: ["a", "b"], entities: [])"#,
    )
    .unwrap();
    std::fs::write(dir.join("a.ron"), r#"(dependencies: ["b"], entities: [])"#).unwrap();
    std::fs::write(dir.join("b.ron"), r#"(dependencies: ["a"], entities: [])"#).unwrap();

    let result = { let mut world = bevy_ecs::world::World::new(); VmInstance::load(&mut world, &dir) };
    let Err(err) = result else {
        panic!("cycle should fail");
    };
    assert!(matches!(err, VmError::PluginCycle { .. }), "got {err:?}");

    std::fs::remove_dir_all(&dir).ok();
}

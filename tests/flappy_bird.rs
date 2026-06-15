//! Flappy Bird headless smoke：world 加载、初始 game / bird / spawner 单例存在。
//!
//! 与 minesweeper / tetris 同思路：headless 跑不到 attach_* 资源路径，只验证
//! module 解析 + entity spawn 在加载阶段成功，渲染交给 viewer。

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/flappy_bird")
}

#[test]
fn flappy_bird_world_loads_with_initial_entities() {
    let mut world = World::new();
    let vm = VmInstance::load(&mut world, world_path()).expect("flappy_bird loads");

    let games = vm.query(&mut world, "game::Game");
    assert_eq!(games.len(), 1, "1 game singleton");
    let birds = vm.query(&mut world, "bird::Bird");
    assert_eq!(birds.len(), 1, "1 bird singleton");
    let spawners = vm.query(&mut world, "pipes::PipeSpawner");
    assert_eq!(spawners.len(), 1, "1 pipe spawner singleton");
}

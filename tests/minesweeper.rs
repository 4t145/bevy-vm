//! Minesweeper headless smoke：world 加载、初始 board entity 存在。
//!
//! attach_* host fn 需要 Bevy 资源（Assets<Image> 等）—— headless 跑 attach
//! 路径会缺资源。这里只验证 module 解析 + entity spawn 在加载阶段成功。

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/minesweeper")
}

#[test]
fn minesweeper_world_loads_with_initial_entities() {
    let mut world = World::new();
    let vm = VmInstance::load(&mut world, world_path()).expect("minesweeper loads");

    let boards = vm.query(&mut world, "board::Board");
    assert_eq!(boards.len(), 1, "1 board singleton");
    let timers = vm.query(&mut world, "timer::GameTimer");
    assert_eq!(timers.len(), 1, "1 game timer singleton");
}

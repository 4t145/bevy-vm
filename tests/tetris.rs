//! Tetris headless smoke：world 加载、初始 board / piece / score 单例存在。
//!
//! 与 minesweeper 同思路：headless 跑不到 attach_* 资源路径，只验证 module
//! 解析 + entity spawn 在加载阶段成功，runtime 渲染交给 viewer 跑。

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/tetris")
}

#[test]
fn tetris_world_loads_with_initial_entities() {
    let mut world = World::new();
    let vm = VmInstance::load(&mut world, world_path()).expect("tetris loads");

    let boards = vm.query(&mut world, "board::Board");
    assert_eq!(boards.len(), 1, "1 board singleton");
    let pieces = vm.query(&mut world, "piece::Piece");
    assert_eq!(pieces.len(), 1, "1 active piece singleton");
    let scores = vm.query(&mut world, "piece::Score");
    assert_eq!(scores.len(), 1, "1 score singleton");
}

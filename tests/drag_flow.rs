//! drag headless smoke：world 加载。
//!
//! 拖动行为依赖 cursor / mouse 事件进 VM——headless 给一个最小验证：
//! world 编译 + DragState 单例存在。完整鼠标事件路径在 input_events.rs。

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/drag")
}

#[test]
fn drag_world_loads() {
    let mut world = World::new();
    let vm = VmInstance::load(&mut world, world_path()).expect("loads");
    assert_eq!(vm.query(&mut world, "DragState").len(), 1);
    assert_eq!(vm.query(&mut world, "DragCam").len(), 1);
}

//! alien_cake_addict headless smoke：world 加载 + 初始 entity 存在。

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/alien_cake_addict")
}

#[test]
fn alien_cake_addict_world_loads() {
    let mut world = World::new();
    let vm = VmInstance::load(&mut world, world_path()).expect("loads");
    assert_eq!(vm.query(&mut world, "board::Game").len(), 1);
}

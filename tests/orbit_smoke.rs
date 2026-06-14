//! Headless smoke for the orbit world.
//!
//! `attach_*` host fns access Bevy `Assets<...>` resources which only exist
//! under a real Bevy `App`. Headless test just confirms the world parses +
//! script systems compile without running setup.rhai (whose first call
//! into attach_mesh would panic on the missing `Assets<Mesh>` resource).
//!
//! End-to-end visual is exercised by the viewer example.

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/orbit")
}

#[test]
fn orbit_world_loads() {
    let mut world = World::new();
    let vm = VmInstance::load(&mut world, world_path()).expect("orbit loads");
    let movers = vm.query(&mut world, "Mover");
    assert_eq!(movers.len(), 3);
    let cams = vm.query(&mut world, "OrbitCamera");
    assert_eq!(cams.len(), 1);
}

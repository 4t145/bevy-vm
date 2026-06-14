//! Time host functions: scripts can read `time()` / `delta()` driven by
//! [`VmInstance::advance_time`].

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn script_reads_time_and_delta() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/worlds");
    let ron_path = dir.join("_time_smoke.ron");
    let rhai_path = dir.join("_time_smoke.rhai");

    std::fs::write(
        &rhai_path,
        r#"
            for entity in query("Clock") {
                set(entity, "Clock", "elapsed", time());
                set(entity, "Clock", "delta", delta());
            }
        "#,
    )
    .expect("write rhai");
    std::fs::write(
        &ron_path,
        r#"(
            components: [
                (name: "Clock", default: (elapsed: 0.0, delta: 0.0)),
            ],
            entities: [
                (components: { "Clock": (elapsed: 0.0, delta: 0.0) }),
            ],
            systems: [
                Script(path: "_time_smoke.rhai"),
            ],
        )"#,
    )
    .expect("write ron");

    let mut world = World::new();
    let mut vm = VmInstance::load(&mut world, &ron_path).expect("load");

    vm.advance_time(Duration::from_millis(16));
    vm.tick(&mut world).expect("tick 1");

    let entity = vm.query(&mut world, "Clock")[0];
    let elapsed = vm
        .get(&world, entity, "Clock", "elapsed")
        .expect("get elapsed")
        .as_f64()
        .expect("f64");
    let delta = vm
        .get(&world, entity, "Clock", "delta")
        .expect("get delta")
        .as_f64()
        .expect("f64");
    assert!((elapsed - 0.016).abs() < 1e-9, "elapsed={elapsed}");
    assert!((delta - 0.016).abs() < 1e-9, "delta={delta}");

    vm.advance_time(Duration::from_millis(32));
    vm.tick(&mut world).expect("tick 2");
    let elapsed = vm
        .get(&world, entity, "Clock", "elapsed")
        .unwrap()
        .as_f64()
        .unwrap();
    let delta = vm
        .get(&world, entity, "Clock", "delta")
        .unwrap()
        .as_f64()
        .unwrap();
    assert!((elapsed - 0.048).abs() < 1e-9, "elapsed={elapsed}");
    assert!((delta - 0.032).abs() < 1e-9, "delta={delta}");

    std::fs::remove_file(&ron_path).ok();
    std::fs::remove_file(&rhai_path).ok();
}

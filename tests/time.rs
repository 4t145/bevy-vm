//! Time bridge tests：直接复用 `bevy_time::Time<()>` 资源。
//!
//! 验证：
//! 1. VM 构建后 Time 资源已存在，初始 elapsed=0、delta=0。
//! 2. `advance_time(d)` 把 dt 推到 Time，下一次 tick / 读取看到正确值。
//! 3. 多次 advance 累加 elapsed。
//! 4. 脚本 host `time()` / `delta()` 读到 Rust 端推进的值。

use bevy_vm::VmWorld;
use std::path::PathBuf;
use std::time::Duration;

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/worlds")
        .join(name)
}

#[test]
fn time_resource_starts_zero() {
    let mut vm = VmWorld::load(world_path("movement.ron")).expect("load");
    let time = *vm.world_mut().resource::<bevy_time::Time>();
    assert_eq!(time.elapsed(), Duration::ZERO);
    assert_eq!(time.delta(), Duration::ZERO);
}

#[test]
fn advance_time_updates_elapsed_and_delta() {
    let mut vm = VmWorld::load(world_path("movement.ron")).expect("load");
    let step = Duration::from_millis(16);

    vm.advance_time(step);
    let time = vm.world_mut().resource::<bevy_time::Time>();
    assert_eq!(time.delta(), step);
    assert_eq!(time.elapsed(), step);

    vm.advance_time(step);
    let time = vm.world_mut().resource::<bevy_time::Time>();
    assert_eq!(time.delta(), step);
    assert_eq!(time.elapsed(), step * 2);
}

/// 脚本 host 函数 `time()` / `delta()` 读到正确值。
/// 用 inventory 配置——它有脚本，借此把 VM 端 time/delta 写回 dynamic 组件。
#[test]
fn script_reads_time_and_delta() {
    // 在 tests/worlds/_time_smoke.ron + .rhai 临时目录里写一对，跑完清理。
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/worlds");
    let ron_path = dir.join("_time_smoke.ron");
    let rhai_path = dir.join("_time_smoke.rhai");

    std::fs::write(
        &rhai_path,
        r#"
            // 把 host 函数 time()/delta() 的读数写到 dynamic 组件 Clock。
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

    let mut vm = VmWorld::load(&ron_path).expect("load");

    // 帧 1：advance 16ms 然后 tick——脚本应读到 elapsed=0.016, delta=0.016
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("tick 1");

    let entity = vm.query("Clock")[0];
    let elapsed = vm
        .get(entity, "Clock", "elapsed")
        .expect("get elapsed")
        .as_f64()
        .expect("f64");
    let delta = vm
        .get(entity, "Clock", "delta")
        .expect("get delta")
        .as_f64()
        .expect("f64");
    assert!((elapsed - 0.016).abs() < 1e-9, "elapsed={elapsed}");
    assert!((delta - 0.016).abs() < 1e-9, "delta={delta}");

    // 帧 2：再推 32ms，elapsed 应到 0.048，delta 应换成 0.032
    vm.advance_time(Duration::from_millis(32));
    vm.tick().expect("tick 2");
    let elapsed = vm
        .get(entity, "Clock", "elapsed")
        .unwrap()
        .as_f64()
        .unwrap();
    let delta = vm.get(entity, "Clock", "delta").unwrap().as_f64().unwrap();
    assert!((elapsed - 0.048).abs() < 1e-9, "elapsed={elapsed}");
    assert!((delta - 0.032).abs() < 1e-9, "delta={delta}");

    std::fs::remove_file(&ron_path).ok();
    std::fs::remove_file(&rhai_path).ok();
}

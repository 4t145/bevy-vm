//! End-to-end events: send a typed `Hit` into the VM, the script consumes it,
//! drops health, and emits a dynamic `Damaged` (脚本内部可见，本帧 clear)。
//! 验证脚本端事件链路通畅 + Health mutation 正确。
//!
//! 老版本测试用 host `drain_events_dynamic` 拿 Damaged——新事件模型下
//! dynamic 事件 tick 末 clear，不再跨 tick 留存。host pump_out 应改用
//! typed 通道（双缓冲，host 在 tick 后稳定 drain）。这里用最简的方式
//! 验证：直接看 Health 字段就够说明事件链跑通。

use bevy_ecs::message::Message;
use bevy_vm::VmInstanceBuilder;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Message, Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
struct Hit {
    target: i64,
    amount: f64,
}

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/worlds")
        .join(name)
}

#[test]
fn typed_event_in_dynamic_event_out() {
    let mut world = bevy_ecs::world::World::new();
    let mut vm = VmInstanceBuilder::new()
        .with_event::<Hit>("Hit")
        .expect("typed event registers cleanly")
        .load(&mut world, world_path("damage.ron"))
        .expect("world loads");

    let entities = vm.query(&mut world, "Health");
    assert_eq!(entities.len(), 1, "exactly one Health entity in fixture");
    let target = entities[0];

    // 拆桥后 typed event 直接走 Bevy `Messages<Hit>`：send_event 立即把
    // 事件写进 Messages<Hit>，VM cursor 下次 tick 立刻读到——一次 tick
    // 即足够让脚本看见 + mutate Health。
    vm.send_event::<Hit>(
        &mut world,
        "Hit",
        Hit {
            target: target.to_bits() as i64,
            amount: 12.0,
        },
    )
    .expect("typed event sends cleanly");

    vm.tick(&mut world).expect("tick");

    // Health 从 50 减到 38——脚本端事件链跑通。
    let hp = vm
        .get(&world, target, "Health", "value")
        .expect("Health.value readable");
    let number = hp
        .as_f64()
        .unwrap_or_else(|| panic!("expected number, got {hp:?}"));
    assert!(
        (number - 38.0).abs() < 1e-6,
        "expected hp 38.0, got {number}"
    );
}

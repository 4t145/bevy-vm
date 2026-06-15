//! Headless verification: `MouseButtonInput` flows from outside the VM →
//! script reads `events("MouseButton")` → state mutates accordingly.
//!
//! Render-feature gated because the Bevy input types only build with `dep:bevy`.

#![cfg(feature = "bevy-bridge")]

use bevy::ecs::entity::Entity as BevyEntity;
use bevy::input::ButtonState;
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy_vm::VmInstanceBuilder;
use bevy_vm::plugin::BuilderVmPluginExt;
use bevy_vm::plugin::input::{self, InputPlugin};
use std::path::PathBuf;

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/worlds")
        .join(name)
}

#[test]
fn typed_mouse_button_event_reaches_script() {
    let mut world = bevy_ecs::world::World::new();
    let plugin = InputPlugin;
    let mut vm = VmInstanceBuilder::new()
        .add_plugin(&plugin)
        .expect("input plugin builds the VM side cleanly")
        .load(&mut world, world_path("input_counter.ron"))
        .expect("world loads");

    let entities = vm.query(&mut world, "Counter");
    assert_eq!(entities.len(), 1);
    let counter_entity = entities[0];

    // Synthesize three press events. `Entity::PLACEHOLDER` is fine here —
    // the script does not interpret the window id.
    let make_press = || MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Pressed,
        window: BevyEntity::PLACEHOLDER,
    };

    for _ in 0..3 {
        vm.send_event::<MouseButtonInput>(&mut world, input::MOUSE_BUTTON, make_press())
            .expect("typed MouseButton sends cleanly");
    }

    // 拆桥后 typed event 直接走 Bevy `Messages<MouseButtonInput>`：一次 tick
    // 即可让脚本读到全部 3 个 click 并写入 Counter.clicks。
    vm.tick(&mut world).expect("tick");

    let clicks = vm
        .get(&world, counter_entity, "Counter", "clicks")
        .expect("Counter.clicks readable");
    let number = clicks
        .as_f64()
        .unwrap_or_else(|| panic!("expected number, got {clicks:?}"));
    assert!(
        (number - 3.0).abs() < 1e-6,
        "expected 3 clicks, got {number}"
    );
}

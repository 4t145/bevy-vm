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
        vm.send_event::<MouseButtonInput>(input::MOUSE_BUTTON, make_press())
            .expect("typed MouseButton sends cleanly");
    }

    // Strict double-buffer:
    //   tick 1: events sit in MouseButton.back; script's `events("MouseButton")` is empty.
    //           tick-end swap promotes them to front.
    //   tick 2: script reads 3 events, sets Counter.clicks = 0 + 3.
    vm.tick(&mut world).expect("tick 1");
    vm.tick(&mut world).expect("tick 2");

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

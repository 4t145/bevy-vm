//! End-to-end verification of the drag example: typed `MouseButton` and
//! `MouseMotion` events flow into the VM and the script mutates `Position`,
//! exercising serde-json's enum-as-string preservation along the way.

#![cfg(feature = "bevy-bridge")]

use bevy::ecs::entity::Entity as BevyEntity;
use bevy::input::ButtonState;
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy::math::Vec2;
use bevy::window::CursorMoved;
use bevy_vm::VmWorldBuilder;
use bevy_vm::plugin::BuilderVmPluginExt;
use bevy_vm::plugin::input::{self, InputPlugin};
use std::path::PathBuf;

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/worlds")
        .join(name)
}

fn coord(value: &serde_json::Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("expected number, got {value:?}"))
}

#[test]
fn left_button_drag_moves_cube() {
    let plugin = InputPlugin;
    let mut vm = VmWorldBuilder::new()
        .add_plugin(&plugin)
        .expect("input plugin builds the VM side cleanly")
        .load(world_path("drag"))
        .expect("drag world loads");

    let entities = vm.query("DragState");
    assert_eq!(entities.len(), 1);
    let entity = entities[0];

    // Mouse button down: enables drag state on the next tick.
    vm.send_event::<MouseButtonInput>(
        input::MOUSE_BUTTON,
        MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            window: BevyEntity::PLACEHOLDER,
        },
    )
    .expect("mouse-press sends");

    // Strict double-buffer:
    //   tick 1: events sit in back; script's events("MouseButton") is empty.
    //           tick-end swap: MouseButton.front gains the press event.
    //   tick 2: script reads it, sets DragState.active = 1.
    vm.tick().expect("tick 1");
    vm.tick().expect("tick 2");

    // Send a 100x50 px cursor move. With sensitivity 0.01 the script should
    // add dx=1.0 to Position.x and dy=-0.5 to Position.y (screen Y flipped).
    vm.send_event::<CursorMoved>(
        input::CURSOR_MOVED,
        CursorMoved {
            window: BevyEntity::PLACEHOLDER,
            position: Vec2::new(100.0, 50.0),
            delta: Some(Vec2::new(100.0, 50.0)),
        },
    )
    .expect("cursor-moved sends");

    // Same double-buffer rhythm: the motion event reaches the script on tick 4.
    vm.tick().expect("tick 3");
    vm.tick().expect("tick 4");

    let x = vm
        .get(entity, "Position", "x")
        .expect("Position.x readable");
    let y = vm
        .get(entity, "Position", "y")
        .expect("Position.y readable");
    assert!(
        (coord(&x) - 1.0).abs() < 1e-6,
        "expected Position.x ≈ 1.0 (100px * 0.01), got {x:?}"
    );
    assert!(
        (coord(&y) + 0.5).abs() < 1e-6,
        "expected Position.y ≈ -0.5 (50px * 0.01 negated), got {y:?}"
    );

    // Release: motions arriving afterwards should not move the cube.
    vm.send_event::<MouseButtonInput>(
        input::MOUSE_BUTTON,
        MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Released,
            window: BevyEntity::PLACEHOLDER,
        },
    )
    .expect("mouse-release sends");
    vm.tick().expect("tick 5");
    vm.tick().expect("tick 6");

    vm.send_event::<CursorMoved>(
        input::CURSOR_MOVED,
        CursorMoved {
            window: BevyEntity::PLACEHOLDER,
            position: Vec2::new(600.0, 550.0),
            delta: Some(Vec2::new(500.0, 500.0)),
        },
    )
    .expect("post-release cursor-moved sends");
    vm.tick().expect("tick 7");
    vm.tick().expect("tick 8");

    let x_after = vm
        .get(entity, "Position", "x")
        .expect("Position.x readable");
    let y_after = vm
        .get(entity, "Position", "y")
        .expect("Position.y readable");
    assert!(
        (coord(&x_after) - 1.0).abs() < 1e-6,
        "Position.x should not advance after release, got {x_after:?}"
    );
    assert!(
        (coord(&y_after) + 0.5).abs() < 1e-6,
        "Position.y should not advance after release, got {y_after:?}"
    );
}

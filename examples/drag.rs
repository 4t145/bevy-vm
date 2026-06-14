//! Mouse-drag demo: a cube that follows the cursor while the left button is held.
//!
//! Run: `cargo run --example drag --features render`
//!
//! Wiring shape (mirrors what real apps do):
//! - VM side: `VmWorldBuilder` + `InputPlugin` registers the mouse/keyboard
//!   typed event channels under stable names.
//! - Bevy side: same `&plugin` instance fed to `app.add_vm_plugin(...)` so
//!   pump systems forward Bevy `Events<T>` into the VM's event store.
//! - The script (`drag.rhai`) reads those events and mutates `Position`.

use bevy::prelude::*;
use bevy_vm::plugin::{AppVmPluginExt, BuilderVmPluginExt, input::InputPlugin};
use bevy_vm::render::insert_vm_world;
use bevy_vm::{VmWorld, VmWorldBuilder};
use std::path::PathBuf;

fn main() {
    let plugin = InputPlugin;

    let world_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/drag");
    let vm: VmWorld = match VmWorldBuilder::new()
        .add_plugin(&plugin)
        .and_then(|b| b.load(&world_path))
    {
        Ok(vm) => vm,
        Err(error) => {
            eprintln!(
                "Failed to load drag world ({}): {error}",
                world_path.display()
            );
            return;
        }
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    // Install the VM viewer plugin (and `tick_vm` system) BEFORE wiring the
    // event bridges, so the `.before(tick_vm)` / `.after(tick_vm)` orderings
    // installed by `add_vm_plugin` resolve to the real system.
    insert_vm_world(&mut app, vm);
    app.add_vm_plugin(&plugin);
    app.add_systems(Startup, setup_scene);
    app.run();
}

/// Light only — the camera is now a VM entity declared in `drag.ron` and
/// spawned/synchronized by the render layer's camera pass. Lights are still
/// hand-spawned on the Bevy side until they get the same component treatment.
fn setup_scene(mut commands: Commands) {
    commands.spawn((
        PointLight {
            shadows_enabled: false,
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 6.0, 6.0),
    ));
}

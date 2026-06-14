//! Minimal viewer for the single-world architecture.
//!
//! Loads one VM world (CLI arg or default) into the main Bevy World as a
//! [`VmInstance`] inside [`VmRegistry`]. No more sync layer — script-side
//! `attach_mesh` / `attach_camera_3d` / `attach_pbr` / `set_transform` etc.
//! drop Bevy native components on the same entity directly.

use bevy::prelude::*;
use bevy_vm::plugin::cursor::CursorPlugin;
use bevy_vm::plugin::input::InputPlugin;
use bevy_vm::plugin::picking::PickingPlugin;
use bevy_vm::plugin::{AppVmPluginExt, BuilderVmPluginExt};
use bevy_vm::render::insert_vm_instance;
use bevy_vm::{VmInstance, VmInstanceBuilder};
use std::path::PathBuf;

const WORLDS_DIR: &str = "examples/worlds";
const VIEWER_LIGHT_INTENSITY: f32 = 4_000_000.0;

fn main() {
    let cli_world = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_world);

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        file_path: "examples/assets".to_owned(),
        ..default()
    }));

    let input = InputPlugin;
    let picking = PickingPlugin;
    let cursor = CursorPlugin;

    let vm = build_vm(app.world_mut(), &cli_world, &input, &picking, &cursor)
        .expect("load initial world");
    insert_vm_instance(&mut app, vm);
    app.add_vm_plugin(&input);
    app.add_vm_plugin(&picking);
    app.add_vm_plugin(&cursor);

    app.add_systems(Startup, setup_lighting);
    app.run();
}

fn default_world() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(WORLDS_DIR)
        .join("orbit")
}

fn build_vm(
    world: &mut World,
    path: &std::path::Path,
    input: &InputPlugin,
    picking: &PickingPlugin,
    cursor: &CursorPlugin,
) -> Result<VmInstance, bevy_vm::VmError> {
    VmInstanceBuilder::new()
        .add_plugin(input)?
        .add_plugin(picking)?
        .add_plugin(cursor)?
        .load(world, path)
}

fn setup_lighting(mut commands: Commands) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(1.0, 1.0, 1.0),
        brightness: 600.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            illuminance: 10_000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, -0.5, 0.0)),
    ));
    commands.spawn((
        PointLight {
            shadows_enabled: false,
            intensity: VIEWER_LIGHT_INTENSITY,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 8.0),
    ));
}

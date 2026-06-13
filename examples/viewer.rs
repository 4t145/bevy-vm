//! 图形 viewer：加载一个沙箱世界，每帧 tick 并渲染。
//!
//! 用法：`cargo run --example viewer --features render -- [world.ron]`
//! 省略路径时加载内置 demo 世界。
//!
//! 它把沙箱实体（带 `Position` + `Renderable`）渲染成 3D 图元，脚本驱动它们运动，
//! 验证「主世界渲染沙箱画面」的端到端能力。

use bevy::prelude::*;
use bevy_vm::VmWorld;
use bevy_vm::render::insert_vm_world;
use std::path::PathBuf;

fn main() {
    let world_path = std::env::args()
        .nth(1)
        .map_or_else(default_world, PathBuf::from);

    let vm = match VmWorld::load(&world_path) {
        Ok(vm) => vm,
        Err(error) => {
            eprintln!("加载世界失败 ({}): {error}", world_path.display());
            return;
        }
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        // demo 资产放在 examples/assets/ 下，便于随仓库分发。
        file_path: "examples/assets".to_owned(),
        ..default()
    }));
    app.add_systems(Startup, setup_scene);
    insert_vm_world(&mut app, vm);
    app.run();
}

/// 内置 demo 世界路径。
fn default_world() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/orbit.ron")
}

/// 布置相机与光照（沙箱实体由同步层动态 spawn）。
fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, 16.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 4_000_000.0,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 8.0),
    ));
}

//! Universal world viewer with a tiny corner-button + dropdown to switch worlds.
//!
//! Run: `cargo run --example viewer -- [path/to/world]`
//!
//! Behavior:
//! - The CLI arg picks the initial world; if omitted, the first directory
//!   under `examples/worlds/` is used.
//! - A small button in the top-right corner shows the current world name.
//!   Clicking it toggles a dropdown listing every directory under
//!   `examples/worlds/` containing a `world.ron`. Picking one swaps the
//!   active VM world without restarting the process.
//! - When the dropdown is closed nothing covers the 3D scene. The button
//!   itself is a small overlay in the corner only.
//! - [`bevy_vm::plugin::input::InputPlugin`] +
//!   [`bevy_vm::plugin::picking::PickingPlugin`] are wired automatically.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_vm::component::Position;
use bevy_vm::component::camera::{Camera3d as VmCamera3d, CameraProjection};
use bevy_vm::plugin::{
    AppVmPluginExt, BuilderVmPluginExt, cursor::CursorPlugin, input::InputPlugin,
    picking::PickingPlugin,
};
use bevy_vm::render::{VmEntityRef, insert_vm_world, reset_viewer_state};
use bevy_vm::{VmWorld, VmWorldBuilder};
use std::path::{Path, PathBuf};

/// Folder containing all worlds bundled with the repo.
const WORLDS_DIR: &str = "examples/worlds";

/// Fallback camera distance when the loaded world ships no camera.
const FALLBACK_CAMERA_DISTANCE: f32 = 12.0;
/// Fallback camera vertical FOV (degrees).
const FALLBACK_CAMERA_FOV: f32 = 60.0;

/// Point light intensity for the always-on viewer lamp.
const VIEWER_LIGHT_INTENSITY: f32 = 4_000_000.0;

/// Toggle button width (px).
const TOGGLE_BUTTON_WIDTH_PX: f32 = 160.0;
/// Toggle button height (px).
const TOGGLE_BUTTON_HEIGHT_PX: f32 = 24.0;
/// Distance from the top-right corner of the window (px).
const TOGGLE_BUTTON_INSET_PX: f32 = 8.0;
/// Width of the dropdown panel (px).
const DROPDOWN_WIDTH_PX: f32 = 220.0;

/// Resource: list of world directory paths (absolute) found under WORLDS_DIR.
#[derive(Resource, Default)]
struct WorldCatalog {
    entries: Vec<PathBuf>,
}

/// Resource: the world currently active in the VM. Used to highlight the
/// matching button in the dropdown and label the toggle button.
#[derive(Resource, Default)]
struct ActiveWorld {
    path: Option<PathBuf>,
}

/// Resource: a world swap requested by the UI but not yet applied. The
/// exclusive [`apply_pending_switch`] system consumes it.
#[derive(Resource, Default)]
struct PendingWorldSwitch {
    path: Option<PathBuf>,
}

/// Resource: dropdown open/closed state.
#[derive(Resource, Default)]
struct DropdownState {
    open: bool,
}

/// Marker on every UI entity owned by the viewer (toolbar host + FPS
/// overlay). Used by [`apply_pending_switch`] to skip viewer UI when
/// despawning leftover stray UI from a previous world.
#[derive(Component)]
struct ViewerUi;

/// Marker on the toggle button entity (so we can update its label as the
/// active world changes).
#[derive(Component)]
struct ToggleButton;

/// Marker on the dropdown root entity — toggling visibility shows/hides
/// the menu without rebuilding it.
#[derive(Component)]
struct DropdownRoot;

/// Marker on each world entry in the dropdown. Holds the path to load.
#[derive(Component)]
struct WorldButton {
    path: PathBuf,
}

/// Marker on the toggle button's `Text` child — refreshed when the active
/// world changes.
#[derive(Component)]
struct ToggleLabel;

/// Marker on the FPS overlay text node.
#[derive(Component)]
struct FpsOverlay;

fn main() {
    let cli_world = std::env::args().nth(1).map(PathBuf::from);

    let catalog = scan_world_catalog();
    let initial = cli_world
        .or_else(|| catalog.entries.first().cloned())
        .unwrap_or_else(|| PathBuf::from(WORLDS_DIR).join("geometry_bros"));

    let vm = match build_vm(&initial) {
        Ok(vm) => vm,
        Err(error) => {
            eprintln!("Failed to load world ({}): {error}", initial.display());
            return;
        }
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        file_path: "examples/assets".to_owned(),
        ..default()
    }));
    app.add_plugins(FrameTimeDiagnosticsPlugin::default());

    insert_vm_world(&mut app, vm);
    app.add_vm_plugin(&InputPlugin);
    app.add_vm_plugin(&PickingPlugin);
    app.add_vm_plugin(&CursorPlugin);

    app.insert_resource(catalog)
        .insert_resource(ActiveWorld {
            path: Some(initial),
        })
        .init_resource::<PendingWorldSwitch>()
        .init_resource::<DropdownState>()
        .add_systems(
            Startup,
            (setup_lighting, setup_dropdown_ui, setup_fps_overlay),
        )
        .add_systems(
            Update,
            (
                handle_toggle_click,
                handle_world_button_click,
                refresh_dropdown_visibility,
                refresh_world_button_styles,
                refresh_toggle_label,
                refresh_fps_overlay,
            ),
        )
        // 切换在 Last 跑：保证本帧所有 VM tick / sync 完成后再做大手术。
        .add_systems(Last, apply_pending_switch);
    app.run();
}

/// Scan WORLDS_DIR for any subdirectory containing a `world.ron`.
fn scan_world_catalog() -> WorldCatalog {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(WORLDS_DIR);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return WorldCatalog::default();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() && path.join("world.ron").is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    paths.sort();
    WorldCatalog { entries: paths }
}

fn build_vm(world_path: &Path) -> Result<VmWorld, bevy_vm::VmError> {
    let mut vm = VmWorldBuilder::new()
        .add_plugin(&InputPlugin)?
        .add_plugin(&PickingPlugin)?
        .add_plugin(&CursorPlugin)?
        .load(world_path)?;
    if !world_has_camera(&mut vm) {
        spawn_fallback_camera(&mut vm);
    }
    Ok(vm)
}

fn world_has_camera(vm: &mut VmWorld) -> bool {
    !vm.query("Camera3d").is_empty() || !vm.query("Camera2d").is_empty()
}

fn spawn_fallback_camera(vm: &mut VmWorld) {
    let world = vm.world_mut();
    world.spawn((
        Position {
            x: 0.0,
            y: 0.0,
            z: FALLBACK_CAMERA_DISTANCE,
        },
        VmCamera3d {
            projection: CameraProjection::Perspective {
                fov_degrees: FALLBACK_CAMERA_FOV,
                near: 0.1,
                far: 1000.0,
            },
            up: [0.0, 1.0, 0.0],
            target: [0.0, 0.0, 0.0],
            order: 0,
            active: true,
            clear_color: None,
        },
    ));
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

/// Spawn the toggle button + dropdown panel. Dropdown starts hidden.
fn setup_dropdown_ui(mut commands: Commands, catalog: Res<WorldCatalog>, active: Res<ActiveWorld>) {
    let initial_label = active
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("(none)")
        .to_owned();

    // 顶层容器——一个 100% 全屏的 Node，display none 也会丢 picking。这里用一个
    // 透明背景的常驻容器，把按钮/下拉钉在右上角；容器本身不接收 picking。
    let host = commands
        .spawn((
            ViewerUi,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                width: Val::Auto,
                height: Val::Auto,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                padding: UiRect::all(Val::Px(TOGGLE_BUTTON_INSET_PX)),
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(1000),
            // host 本身不参与点击——避免空白区域吞掉 VM picking 事件。
            bevy::picking::Pickable::IGNORE,
        ))
        .id();

    let toggle = commands
        .spawn((
            ToggleButton,
            Button,
            Node {
                width: Val::Px(TOGGLE_BUTTON_WIDTH_PX),
                height: Val::Px(TOGGLE_BUTTON_HEIGHT_PX),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(Val::Px(8.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.12, 0.16, 0.85)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.20)),
        ))
        .id();
    let toggle_text = commands
        .spawn((
            ToggleLabel,
            Text::new(format!("World: {initial_label}")),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(Color::srgb(0.92, 0.92, 0.96)),
            bevy::picking::Pickable::IGNORE,
        ))
        .id();
    commands.entity(toggle).add_child(toggle_text);
    commands.entity(host).add_child(toggle);

    // Dropdown root：默认 hidden。
    let dropdown = commands
        .spawn((
            DropdownRoot,
            Node {
                width: Val::Px(DROPDOWN_WIDTH_PX),
                display: Display::None,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.07, 0.10, 0.96)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.15)),
        ))
        .id();
    for path in &catalog.entries {
        let display = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_owned();
        let entry = commands
            .spawn((
                Button,
                WorldButton { path: path.clone() },
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(world_button_color(false, false)),
            ))
            .id();
        let entry_text = commands
            .spawn((
                Text::new(display),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.95, 0.97)),
                bevy::picking::Pickable::IGNORE,
            ))
            .id();
        commands.entity(entry).add_child(entry_text);
        commands.entity(dropdown).add_child(entry);
    }
    commands.entity(host).add_child(dropdown);
}

fn world_button_color(active: bool, hovered: bool) -> Color {
    match (active, hovered) {
        (true, _) => Color::srgba(0.20, 0.45, 0.85, 0.95),
        (false, true) => Color::srgba(0.30, 0.32, 0.40, 0.95),
        (false, false) => Color::srgba(0.18, 0.20, 0.26, 0.0),
    }
}

/// FPS overlay：左上角小数字。`FrameTimeDiagnosticsPlugin` 已经在 main 里
/// 注册——本 system 仅 spawn 文本节点，refresh_fps_overlay 每帧读 diagnostic
/// 更新内容。
fn setup_fps_overlay(mut commands: Commands) {
    commands.spawn((
        FpsOverlay,
        ViewerUi,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(4.0),
            left: Val::Px(8.0),
            ..default()
        },
        Text::new("FPS --"),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgba(0.85, 0.95, 0.85, 0.85)),
        GlobalZIndex(1000),
        bevy::picking::Pickable::IGNORE,
    ));
}

fn refresh_fps_overlay(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsOverlay>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let frame_time_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed());
    let label = match (fps, frame_time_ms) {
        (Some(fps), Some(ft)) => format!("FPS {fps:>5.1}  {ft:>5.2} ms"),
        (Some(fps), None) => format!("FPS {fps:>5.1}"),
        _ => "FPS --".to_owned(),
    };
    for mut text in &mut query {
        **text = label.clone();
    }
}

/// Toggle button click → flip dropdown state.
fn handle_toggle_click(
    query: Query<&Interaction, (Changed<Interaction>, With<ToggleButton>)>,
    mut state: ResMut<DropdownState>,
) {
    for interaction in &query {
        if *interaction == Interaction::Pressed {
            state.open = !state.open;
        }
    }
}

/// World entry click → queue swap + close dropdown.
fn handle_world_button_click(
    query: Query<(&Interaction, &WorldButton), Changed<Interaction>>,
    mut pending: ResMut<PendingWorldSwitch>,
    active: Res<ActiveWorld>,
    mut state: ResMut<DropdownState>,
) {
    for (interaction, button) in &query {
        if *interaction == Interaction::Pressed
            && active.path.as_ref() != Some(&button.path)
            && pending.path.is_none()
        {
            pending.path = Some(button.path.clone());
            state.open = false;
        }
    }
}

/// Apply [`DropdownState`] to the dropdown root's `Display`.
fn refresh_dropdown_visibility(
    state: Res<DropdownState>,
    mut query: Query<&mut Node, With<DropdownRoot>>,
) {
    if !state.is_changed() {
        return;
    }
    for mut node in &mut query {
        node.display = if state.open {
            Display::Flex
        } else {
            Display::None
        };
    }
}

fn refresh_world_button_styles(
    mut query: Query<(&Interaction, &WorldButton, &mut BackgroundColor)>,
    active: Res<ActiveWorld>,
) {
    for (interaction, button, mut color) in &mut query {
        let is_active = active.path.as_ref() == Some(&button.path);
        let hovered = matches!(interaction, Interaction::Hovered | Interaction::Pressed);
        color.0 = world_button_color(is_active, hovered);
    }
}

/// Update the toggle button's label whenever the active world changes.
fn refresh_toggle_label(active: Res<ActiveWorld>, mut query: Query<&mut Text, With<ToggleLabel>>) {
    if !active.is_changed() {
        return;
    }
    let name = active
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("(none)");
    for mut text in &mut query {
        **text = format!("World: {name}");
    }
}

/// Exclusive system — consumes [`PendingWorldSwitch`] and rebuilds the VM in
/// place. Touches the [`VmWorld`] NonSend resource and viewer-internal
/// bookkeeping resources, so it needs `&mut World`.
fn apply_pending_switch(world: &mut World) {
    let Some(target) = world
        .get_resource_mut::<PendingWorldSwitch>()
        .and_then(|mut p| p.path.take())
    else {
        return;
    };

    // Despawn every mirror entity (incl. cameras now that render.rs tags them).
    let mirrors: Vec<Entity> = world
        .query_filtered::<Entity, With<VmEntityRef>>()
        .iter(world)
        .collect();
    let mirror_count = mirrors.len();
    for entity in mirrors {
        if let Ok(entity_cmd) = world.get_entity_mut(entity) {
            entity_cmd.despawn();
        }
    }

    // 兜底：cascade 应该带走 UI 子树，但万一某个子节点 sync_hierarchy 还
    // 没把 ChildOf 关系建到主 World——它是顶层孤儿 UI Node，cascade 漏掉。
    // 把所有不属于 viewer 自身（ToolbarRoot / FpsOverlay 子树）的 UI Node
    // 一并 despawn。
    let stray_ui: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (
            With<bevy::ui::Node>,
            Without<bevy_ecs::hierarchy::ChildOf>,
            Without<ViewerUi>,
        )>();
        q.iter(world).collect()
    };
    let stray_count = stray_ui.len();
    for entity in stray_ui {
        if let Ok(entity_cmd) = world.get_entity_mut(entity) {
            entity_cmd.despawn();
        }
    }

    let surviving_nodes = world
        .query_filtered::<Entity, With<bevy::ui::Node>>()
        .iter(world)
        .count();
    info!(
        "world switch -> {}: despawned {mirror_count} mirrors + {stray_count} stray UI; remaining Nodes = {surviving_nodes}",
        target.display()
    );

    let new_vm = match build_vm(&target) {
        Ok(vm) => vm,
        Err(error) => {
            error!("Failed to load world {}: {error}", target.display());
            return;
        }
    };
    world.insert_non_send_resource(new_vm);
    reset_viewer_state(world);

    // 切换 world 时自动释放鼠标 grab——避免上一个 world 锁定的 cursor 留给
    // 不需要 FPS 视角的下一个 world（如 geometry_bros / minesweeper）。
    let mut q = world.query_filtered::<&mut CursorOptions, With<PrimaryWindow>>();
    if let Ok(mut cursor) = q.single_mut(world) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }

    if let Some(mut active) = world.get_resource_mut::<ActiveWorld>() {
        active.path = Some(target);
    }
}

//! World-switcher viewer.
//!
//! - 启动时加载 CLI 指定的 world（缺省 orbit）
//! - 顶栏右上 toggle button 显示当前 world，点击展开下拉
//! - 选其它 world：despawn 当前 VM 的所有 VmTag 实体 + 从 registry 移除 +
//!   load 新 world 进同一个主 World

use bevy::prelude::*;
use bevy_vm::plugin::input::InputPlugin;
use bevy_vm::plugin::picking::PickingPlugin;
use bevy_vm::plugin::{AppVmPluginExt, BuilderVmPluginExt};
use bevy_vm::{
    VmAppPlugin, VmId, VmInstance, VmInstanceBuilder, VmRegistry, despawn_tagged_entities,
};
use std::path::{Path, PathBuf};

const WORLDS_DIR: &str = "examples/worlds";
const VIEWER_LIGHT_INTENSITY: f32 = 4_000_000.0;
const TOGGLE_WIDTH_PX: f32 = 160.0;
const TOGGLE_HEIGHT_PX: f32 = 24.0;
const TOGGLE_INSET_PX: f32 = 8.0;
const DROPDOWN_WIDTH_PX: f32 = 220.0;

/// 列出 `examples/worlds/` 下所有含 `world.ron` 的子目录。
#[derive(Resource, Default)]
struct WorldCatalog {
    entries: Vec<PathBuf>,
}

/// 当前激活的 world 路径 + 它对应的 VmId。
#[derive(Resource, Default)]
struct ActiveWorld {
    path: Option<PathBuf>,
    vm_id: Option<VmId>,
}

/// 待切换：UI 点击塞值，[`apply_world_switch`] 在 Last 阶段消费。
#[derive(Resource, Default)]
struct PendingWorldSwitch {
    path: Option<PathBuf>,
}

#[derive(Resource, Default)]
struct DropdownState {
    open: bool,
}

/// 标记 viewer 自己的 UI——切换 world 时不会被误删（VmTag 实体清理只挑
/// 带 VmTag 的）。但 ViewerUi 仍能区分它们，避免未来的歧义。
#[derive(Component)]
struct ViewerUi;

#[derive(Component)]
struct ToggleButton;

#[derive(Component)]
struct ToggleLabel;

#[derive(Component)]
struct DropdownRoot;

#[derive(Component)]
struct WorldButton {
    path: PathBuf,
}

fn main() {
    let cli_world = std::env::args().nth(1).map(PathBuf::from);

    let catalog = scan_world_catalog();
    let initial = cli_world
        .or_else(|| catalog.entries.first().cloned())
        .unwrap_or_else(|| PathBuf::from(WORLDS_DIR).join("orbit"));

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        file_path: "examples/assets".to_owned(),
        ..default()
    }));

    let input = InputPlugin;
    let picking = PickingPlugin;
    app.add_vm_plugin(&input);
    app.add_vm_plugin(&picking);
    app.insert_non_send_resource(VmRegistry::new());
    app.add_plugins(VmAppPlugin);

    let vm = build_vm(app.world_mut(), &initial, &input, &picking).expect("load initial world");
    let vm_id = vm.id();
    app.world_mut()
        .non_send_resource_mut::<VmRegistry>()
        .insert(vm);

    app.insert_resource(catalog)
        .insert_resource(ActiveWorld {
            path: Some(initial),
            vm_id: Some(vm_id),
        })
        .init_resource::<PendingWorldSwitch>()
        .init_resource::<DropdownState>()
        .add_systems(Startup, (setup_lighting, setup_dropdown_ui))
        .add_systems(
            Update,
            (
                handle_toggle_click,
                handle_world_button_click,
                refresh_dropdown_visibility,
                refresh_world_button_styles,
                refresh_toggle_label,
            ),
        )
        // 切换在 Last 跑：本帧所有 VM tick / 命令都已 flush 后再做大手术。
        .add_systems(Last, apply_world_switch);
    app.run();
}

fn default_world_button_color(active: bool, hovered: bool) -> Color {
    match (active, hovered) {
        (true, _) => Color::srgba(0.20, 0.45, 0.85, 0.95),
        (false, true) => Color::srgba(0.30, 0.32, 0.40, 0.95),
        (false, false) => Color::srgba(0.18, 0.20, 0.26, 0.0),
    }
}

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

fn build_vm(
    world: &mut World,
    path: &Path,
    input: &InputPlugin,
    picking: &PickingPlugin,
) -> Result<VmInstance, bevy_vm::VmError> {
    VmInstanceBuilder::new()
        .add_plugin(input)?
        .add_plugin(picking)?
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

/// 顶栏 toggle button + dropdown panel。Dropdown 默认隐藏。
fn setup_dropdown_ui(mut commands: Commands, catalog: Res<WorldCatalog>, active: Res<ActiveWorld>) {
    let initial_label = world_label(active.path.as_deref());

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
                padding: UiRect::all(Val::Px(TOGGLE_INSET_PX)),
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(1000),
            // host 容器本身不接收 picking——避免空白区抢占下方 VM picking。
            bevy::picking::Pickable::IGNORE,
        ))
        .id();

    let toggle = commands
        .spawn((
            ToggleButton,
            Button,
            Node {
                width: Val::Px(TOGGLE_WIDTH_PX),
                height: Val::Px(TOGGLE_HEIGHT_PX),
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
                BackgroundColor(default_world_button_color(false, false)),
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

fn world_label(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("(none)")
        .to_owned()
}

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
        color.0 = default_world_button_color(is_active, hovered);
    }
}

fn refresh_toggle_label(active: Res<ActiveWorld>, mut query: Query<&mut Text, With<ToggleLabel>>) {
    if !active.is_changed() {
        return;
    }
    let name = world_label(active.path.as_deref());
    for mut text in &mut query {
        **text = format!("World: {name}");
    }
}

/// Exclusive system —— 消费 [`PendingWorldSwitch`]：清掉旧 VM 的所有 VmTag
/// 实体 + 从 registry 移除 + load 新 world。需要 `&mut World` 因为 VmInstance
/// 是 NonSend 资源、`load` 也吃 `&mut World`。
fn apply_world_switch(world: &mut World) {
    let Some(target) = world
        .get_resource_mut::<PendingWorldSwitch>()
        .and_then(|mut p| p.path.take())
    else {
        return;
    };

    // 1. 清掉旧 VM。
    let old_id = world.get_resource::<ActiveWorld>().and_then(|a| a.vm_id);
    if let Some(id) = old_id {
        if let Some(mut registry) = world.get_non_send_resource_mut::<VmRegistry>() {
            // 旧 VmInstance 直接 drop——entities 在主 World 里；下面按 VmTag 清。
            let _ = registry.remove(id);
        }
        despawn_tagged_entities(world, id);
    }

    // 2. 加载新 VM 进同一个 World。
    let input = InputPlugin;
    let picking = PickingPlugin;
    let new_vm = match build_vm(world, &target, &input, &picking) {
        Ok(vm) => vm,
        Err(error) => {
            error!("Failed to load world {}: {error}", target.display());
            return;
        }
    };
    let new_id = new_vm.id();
    if let Some(mut registry) = world.get_non_send_resource_mut::<VmRegistry>() {
        registry.insert(new_vm);
    }

    if let Some(mut active) = world.get_resource_mut::<ActiveWorld>() {
        active.path = Some(target);
        active.vm_id = Some(new_id);
    }
}

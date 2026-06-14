//! UI 桥测试。
//!
//! 头几个用例只验证 VM World 内"UI typed 组件被注册 + reflect 路径
//! 可读写字段"。Bevy 主 World 一侧的 sync_ui 行为由 `tests/ui_render.rs`
//! 里跑实际 App 的用例覆盖（如果有），这里 headless 即可。

#![cfg(feature = "bevy-bridge")]

use bevy::ui::widget::{Button, Text};
use bevy::ui::{Node, UiRect, Val};
use bevy_vm::VmWorld;
use std::path::PathBuf;

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/worlds")
        .join(name)
}

#[test]
fn ui_node_can_attach_as_component_in_vm_world() {
    // 任意已有的最小配置即可——我们要的只是 ComponentRegistry 已 with_builtins 过。
    let mut vm = VmWorld::load(world_path("movement.ron")).expect("load");
    let world = vm.world_mut();

    let entity = world.spawn_empty().id();
    world.entity_mut(entity).insert(Node {
        width: Val::Px(120.0),
        height: Val::Px(48.0),
        margin: UiRect::all(Val::Px(8.0)),
        ..Default::default()
    });
    let node = world.entity(entity).get::<Node>().expect("Node present");
    assert_eq!(node.width, Val::Px(120.0));
    assert_eq!(node.height, Val::Px(48.0));
}

#[test]
fn button_marker_works_as_component() {
    let mut vm = VmWorld::load(world_path("movement.ron")).expect("load");
    let world = vm.world_mut();

    let e = world.spawn((Node::default(), Button)).id();
    assert!(world.entity(e).get::<Button>().is_some());
    assert!(world.entity(e).get::<Node>().is_some());
}

#[test]
fn text_widget_components_co_exist() {
    let mut vm = VmWorld::load(world_path("movement.ron")).expect("load");
    let world = vm.world_mut();

    let e = world.spawn(Text::new("hello")).id();
    // Text 的 #[require(...)] 自动挂上 TextLayout/TextFont/TextColor 等
    let t = world.entity(e).get::<Text>().expect("Text present");
    assert_eq!(&t.0, "hello");
    assert!(
        world.entity(e).get::<bevy::text::TextFont>().is_some(),
        "Text 的 #[require] 应自动挂上 TextFont",
    );
}

/// 通过 world_access::set 走 reflect 路径写 Node 字段——这是脚本 set 的真正路径。
#[test]
fn set_node_via_world_access_reflect_path() {
    use bevy_vm::world_access;

    let path = world_path("movement.ron");
    let mut vm = VmWorld::load(&path).expect("load");

    // ComponentRegistry::with_builtins 已注册 Node。脚本入口走的就是这条路。
    let registry = vm.components();
    let world = vm.world_mut();
    let entity = world.spawn_empty().id();

    // 用脚本风格的写法：set Node.width 为 Px(200.0)。
    // reflect enum 形态：{"Px": 200.0}
    world_access::set(
        world,
        &registry,
        entity,
        "Node",
        "width",
        serde_json::json!({"Px": 200.0}),
    )
    .expect("set Node.width");

    let node = world.entity(entity).get::<Node>().expect("Node present");
    assert_eq!(node.width, Val::Px(200.0));
}

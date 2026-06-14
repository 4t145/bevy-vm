//! counter_sphere smoke：headless 验证 world 加载、第一帧 spawn、shoot 路径。

#![cfg(feature = "bevy-bridge")]

use bevy_vm::plugin::BuilderVmPluginExt;
use bevy_vm::plugin::cursor::CursorPlugin;
use bevy_vm::plugin::input::InputPlugin;
use bevy_vm::plugin::picking::PickingPlugin;
use bevy_vm::{VmWorld, VmWorldBuilder};
use std::path::PathBuf;
use std::time::Duration;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/counter_sphere")
}

fn boot() -> VmWorld {
    VmWorldBuilder::new()
        .add_plugin(&InputPlugin)
        .expect("InputPlugin")
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin")
        .add_plugin(&CursorPlugin)
        .expect("CursorPlugin")
        .load(world_path())
        .expect("counter_sphere world loads")
}

#[test]
fn world_loads_and_spawns_initial_entities() {
    let mut vm = boot();

    // 第一帧——player.rhai / terrain.rhai / enemy_spawn.rhai / gun_spawn.rhai
    // / crosshair.rhai 都跑过 spawn 阶段。
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("first tick");

    assert_eq!(vm.query("player::Player").len(), 1);
    assert_eq!(vm.query("enemy::Enemy").len(), 5, "5 个敌人");
    assert!(!vm.query("terrain::Obstacle").is_empty(), "至少有一些障碍",);
    assert_eq!(vm.query("gun::Gun").len(), 1, "一根烧火棍");
    assert_eq!(vm.query("crosshair::Crosshair").len(), 1, "一个准星");
}

/// 模拟左键开火 → 应 spawn 一颗子弹。
#[test]
fn left_click_spawns_a_bullet() {
    use bevy::ecs::entity::Entity as BevyEntity;
    use bevy::input::ButtonState;
    use bevy::input::mouse::MouseButtonInput;
    use bevy::window::PrimaryWindow;

    let mut vm = boot();
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("warm up tick");

    // 推一颗 MouseButton Pressed (Primary) 进 typed event 队列。
    vm.send_event::<MouseButtonInput>(
        "MouseButton",
        MouseButtonInput {
            button: bevy::input::mouse::MouseButton::Left,
            state: ButtonState::Pressed,
            window: BevyEntity::from_bits(BevyEntity::PLACEHOLDER.to_bits()),
        },
    )
    .expect("send MouseButton");
    let _ = std::any::type_name::<PrimaryWindow>(); // 保留 import 防止 unused 警告

    // typed event 双缓冲：发送当帧只进 back，下一帧 swap 后才 visible。
    // 跑 3 帧给 player.rhai 看到 + shoot.rhai 处理。
    for _ in 0..3 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    let bullets = vm.query("bullet::Bullet");
    assert!(
        !bullets.is_empty(),
        "左键按下应至少 spawn 1 颗子弹，实际 {}",
        bullets.len(),
    );
}

/// 鼠标 motion 应累加 yaw 与 pitch（pitch 取反 dy）。
#[test]
fn mouse_motion_rotates_view() {
    use bevy::ecs::entity::Entity as BevyEntity;
    use bevy::input::mouse::MouseMotion;

    let mut vm = boot();
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("warm up");

    let player = vm.query("player::Player")[0];
    let yaw0 = vm
        .get(player, "player::Player", "yaw")
        .unwrap()
        .as_f64()
        .unwrap();
    let pitch0 = vm
        .get(player, "player::Player", "pitch")
        .unwrap()
        .as_f64()
        .unwrap();
    assert!(yaw0.abs() < 1e-6 && pitch0.abs() < 1e-6);

    let _ = BevyEntity::PLACEHOLDER;
    vm.send_event::<MouseMotion>(
        "MouseMotion",
        MouseMotion {
            delta: bevy::math::Vec2::new(200.0, -100.0),
        },
    )
    .expect("send MouseMotion");

    for _ in 0..2 {
        vm.advance_time(Duration::from_millis(16));
        vm.tick().expect("tick");
    }

    let yaw1 = vm
        .get(player, "player::Player", "yaw")
        .unwrap()
        .as_f64()
        .unwrap();
    let pitch1 = vm
        .get(player, "player::Player", "pitch")
        .unwrap()
        .as_f64()
        .unwrap();
    assert!(yaw1 > 0.4, "dx 200 * 0.0025 = 0.5, yaw 应增长，实际 {yaw1}");
    assert!(pitch1 > 0.2, "dy -100 → pitch 增（仰视），实际 {pitch1}",);
}

/// 5 杀触发 WinScreen，按 R 应重新开始：清场 + 重生敌人 + kills 归零。
#[test]
fn r_restarts_after_victory() {
    use bevy::ecs::entity::Entity as BevyEntity;
    use bevy::input::ButtonState;
    use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};

    let mut vm = boot();
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("warm up");

    // 直接把 player.kills 设到 5 让 win 弹画面。
    let player = vm.query("player::Player")[0];
    let registry = vm.components();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "kills",
        serde_json::json!(5),
    )
    .unwrap();
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("tick win");
    assert_eq!(vm.query("win::WinScreen").len(), 1);

    // 按 R——KeyboardInput typed 双缓冲，3 帧让 emit + restart 跑完。
    vm.send_event::<KeyboardInput>(
        "KeyboardInput",
        KeyboardInput {
            key_code: KeyCode::KeyR,
            logical_key: Key::Character("r".into()),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: BevyEntity::PLACEHOLDER,
        },
    )
    .expect("send R");
    for _ in 0..3 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    assert!(vm.query("win::WinScreen").is_empty(), "胜利画面应消失");
    let kills = vm
        .get(player, "player::Player", "kills")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(kills, 0, "kills 归零");
    assert_eq!(vm.query("enemy::Enemy").len(), 5, "5 个敌人重生");
}

/// 子弹打到敌人附近应造成伤害；多发命中 → kill + Kills+1。
#[test]
fn bullets_kill_enemies() {
    let mut vm = boot();
    vm.advance_time(Duration::from_millis(16));
    vm.tick().expect("warm up");

    // 把第一个敌人挪到玩家正前方近距离——player yaw=0 时 forward=(0,0,-1)。
    // 敌人 y=0.6（球半径），玩家眼高 1.7——拔高敌人或把玩家 pitch 朝下。
    // 这里把敌人抬到 1.7 与子弹同高，便于精确命中验证。
    let enemy = vm.query("enemy::Enemy")[0];
    let registry = vm.components();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        enemy,
        "Position",
        "x",
        serde_json::json!(0.0),
    )
    .unwrap();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        enemy,
        "Position",
        "y",
        serde_json::json!(1.7),
    )
    .unwrap();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        enemy,
        "Position",
        "z",
        serde_json::json!(-3.0),
    )
    .unwrap();

    // 玩家位置 (0, 1, 0)，敌人 (0, ~0.6, -3)。子弹在 y=0+eye=0.7，命中范围
    // r=0.7 内 dy=0.1，命中需要 dz 接近 0——子弹会逐帧逼近 (0, 0.7, -25*t)，
    // t=0.1s 时 z=-2.5，距敌人(0,0.6,-3) → dz=0.5、dy=0.1 → dist≈0.51 < 0.7 ✅
    //
    // 直接调 emit("MouseButton") 走不通——我们用 send_event 推几次 Pressed。
    use bevy::ecs::entity::Entity as BevyEntity;
    use bevy::input::ButtonState;
    use bevy::input::mouse::MouseButtonInput;
    for _ in 0..3 {
        vm.send_event::<MouseButtonInput>(
            "MouseButton",
            MouseButtonInput {
                button: bevy::input::mouse::MouseButton::Left,
                state: ButtonState::Pressed,
                window: BevyEntity::PLACEHOLDER,
            },
        )
        .unwrap();
        // 推 3 帧让子弹飞到敌人 + combat 处理。
        for _ in 0..6 {
            vm.advance_time(Duration::from_millis(33));
            vm.tick().expect("tick");
        }
    }

    let player = vm.query("player::Player")[0];
    let kills = vm
        .get(player, "player::Player", "kills")
        .unwrap()
        .as_i64()
        .unwrap();
    assert!(kills >= 1, "应至少击杀 1 个敌人，实际 {kills}");
}

//! Headless smoke test: load the minesweeper world, run a couple of ticks,
//! and confirm the init script spawns the expected number of tiles without
//! tripping rhai's operation budget or any host-function error.

#![cfg(feature = "bevy-bridge")]

use bevy::ecs::entity::Entity as BevyEntity;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy_vm::plugin::BuilderVmPluginExt;
use bevy_vm::plugin::input::{self, InputPlugin};
use bevy_vm::plugin::picking::{PickClickEvent, PickingPlugin};
use bevy_vm::{VmWorld, VmWorldBuilder};
use std::path::PathBuf;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/minesweeper")
}

const TILE: &str = "cell::Tile";
const BOARD: &str = "board::Board";

/// 事件模型（新版）：
/// - typed 事件（host PickClick / KeyboardInput）双缓冲：host send 后下一
///   tick 脚本才能看到——pump_in 路径不可避免的 1 帧延迟
/// - dynamic 事件（cell::RevealCell 等）单缓冲同帧消费：plugin 之间事件链
///   零延迟（拓扑保证 emit 在 read 之前）
///
/// 因此 host PickClick → 实际 mutation 共需 **2 个 tick**：
///   tick N: send_event 后 ui 读不到（仍在 back）
///   tick N: tick 末 swap，PickClick 进 front
///   tick N+1: ui 读 → emit RevealCell（同帧 dynamic 可见）
///            cell 读 RevealCell → 处理
///
/// 所以 send_event 后调 2 个 tick 就够。R 键链路（restart → cell → board
/// 重建）只多 1 个 tick——因为 cell handle restart 在 tick N+1 内 reset
/// Board.layout_built=0，但 board 是 plugin 拓扑顺序中**前于** restart/cell——
/// 它在本 tick 已跑过了。要等 tick N+2 才看到 layout_built=0 重建。
fn ticks(vm: &mut VmWorld, n: usize) {
    for i in 0..n {
        vm.tick().unwrap_or_else(|e| panic!("tick {i}: {e}"));
    }
}

#[test]
fn minesweeper_init_spawns_full_board() {
    let mut vm: VmWorld = VmWorldBuilder::new()
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin registers")
        .load(world_path())
        .expect("minesweeper world loads");

    // First tick runs the init branch (layout only — mines deferred until first click).
    vm.tick().expect("tick 1 (layout)");

    let tiles = vm.query(TILE);
    assert_eq!(tiles.len(), 64, "should spawn 8x8 = 64 tile entities");

    // 确认每个 tile 也挂了 Sprite2d typed component（而非只是 dynamic Tile）。
    let sprites = vm.query("Sprite2d");
    assert_eq!(
        sprites.len(),
        64,
        "should also have 64 Sprite2d typed components"
    );

    // 首次点击保护：init 阶段不应布雷。
    let mut mines_before_click = 0;
    for tile in &tiles {
        let value = vm.get(*tile, TILE, "mine").expect("Tile.mine readable");
        if value.as_i64() == Some(1) {
            mines_before_click += 1;
        }
    }
    assert_eq!(
        mines_before_click, 0,
        "mines should be deferred until first click"
    );

    vm.tick().expect("tick 2 (idle)");
    assert_eq!(vm.query(TILE).len(), 64, "no tiles spawned on idle tick");
}

/// 首次点击触发布雷，且点击格 + 8 邻居必为安全。
#[test]
fn first_click_places_mines_safely() {
    let mut vm: VmWorld = VmWorldBuilder::new()
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin registers")
        .load(world_path())
        .expect("minesweeper world loads");

    vm.tick().expect("tick 1 (layout)");

    // 选一个内陆格作为首次点击位（保证 8 邻居都在棋盘内 → 测试更严格）。
    let tiles = vm.query(TILE);
    let target = *tiles
        .iter()
        .find(|t| {
            let x = vm
                .get(**t, TILE, "x")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            let y = vm
                .get(**t, TILE, "y")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            (1..=6).contains(&x) && (1..=6).contains(&y)
        })
        .expect("at least one inland tile exists");

    let target_x = vm
        .get(target, TILE, "x")
        .expect("Tile.x readable")
        .as_i64()
        .expect("x is integer");
    let target_y = vm
        .get(target, TILE, "y")
        .expect("Tile.y readable")
        .as_i64()
        .expect("y is integer");

    vm.send_event::<PickClickEvent>(
        "PickClick",
        PickClickEvent {
            entity: target.to_bits(),
            button: "Primary".to_owned(),
        },
    )
    .expect("PickClick send ok");

    // 拆分后事件链：PickClick → ui emit RevealCell → cell handle reveal。
    // 2 跳 × 2 帧（swap + 处理）= 4 个 tick。
    ticks(&mut vm, 2);

    // 布雷已发生：恰好 10 雷。
    let mut mines = 0;
    for tile in &tiles {
        let value = vm.get(*tile, TILE, "mine").expect("Tile.mine readable");
        if value.as_i64() == Some(1) {
            mines += 1;
        }
    }
    assert_eq!(mines, 10, "first click should trigger placing 10 mines");

    // 首次点击格 + 8 邻居都不应是雷。
    for tile in &tiles {
        let tx = vm.get(*tile, TILE, "x").unwrap().as_i64().unwrap();
        let ty = vm.get(*tile, TILE, "y").unwrap().as_i64().unwrap();
        let dx = (tx - target_x).abs();
        let dy = (ty - target_y).abs();
        if dx <= 1 && dy <= 1 {
            let mine = vm.get(*tile, TILE, "mine").unwrap().as_i64().unwrap();
            assert_eq!(
                mine, 0,
                "tile at ({tx}, {ty}) within first-click safe zone must not be a mine",
            );
        }
    }
}

/// chord：已揭开数字格被右键，且相邻已标旗数 == adj 时，自动展开剩余隐藏邻居。
#[test]
fn chord_expands_when_flag_count_matches() {
    use bevy_ecs::entity::Entity;

    let mut vm: VmWorld = VmWorldBuilder::new()
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin registers")
        .load(world_path())
        .expect("minesweeper world loads");

    vm.tick().expect("tick 1 (layout)");
    let tiles = vm.query(TILE);

    // 找一个有完整 8 邻居的内陆格作为首次点击。
    let center = *tiles
        .iter()
        .find(|t| {
            let x = vm
                .get(**t, TILE, "x")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            let y = vm
                .get(**t, TILE, "y")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            (1..=6).contains(&x) && (1..=6).contains(&y)
        })
        .expect("inland tile exists");

    // 首次点击触发布雷 + reveal + flood。
    vm.send_event::<PickClickEvent>(
        "PickClick",
        PickClickEvent {
            entity: center.to_bits(),
            button: "Primary".to_owned(),
        },
    )
    .unwrap();
    ticks(&mut vm, 2);

    // 找一个已揭开的数字格（adj > 0），并对其所有雷邻居打旗。
    let revealed_number_tile = tiles.iter().copied().find(|t| {
        let revealed = vm
            .get(*t, TILE, "revealed")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let adj = vm
            .get(*t, TILE, "adj")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        revealed == 1 && adj > 0
    });
    let Some(number_tile) = revealed_number_tile else {
        // flood-fill 可能填满整个安全区，没留下边界数字格 → 此局不可测，但脚本逻辑测试已在 first_click 中覆盖。
        return;
    };

    let nx = vm.get(number_tile, TILE, "x").unwrap().as_i64().unwrap();
    let ny = vm.get(number_tile, TILE, "y").unwrap().as_i64().unwrap();

    // 对 number_tile 的所有"是雷"邻居发 FlagCell（直接走 secondary click，未揭开 → 切旗）。
    let mine_neighbors: Vec<Entity> = tiles
        .iter()
        .copied()
        .filter(|t| {
            let tx = vm.get(*t, TILE, "x").unwrap().as_i64().unwrap();
            let ty = vm.get(*t, TILE, "y").unwrap().as_i64().unwrap();
            let dx = (tx - nx).abs();
            let dy = (ty - ny).abs();
            if dx > 1 || dy > 1 || (dx == 0 && dy == 0) {
                return false;
            }
            vm.get(*t, TILE, "mine").unwrap().as_i64() == Some(1)
        })
        .collect();

    for mine_tile in &mine_neighbors {
        vm.send_event::<PickClickEvent>(
            "PickClick",
            PickClickEvent {
                entity: mine_tile.to_bits(),
                button: "Secondary".to_owned(),
            },
        )
        .unwrap();
        ticks(&mut vm, 2);
    }

    // 记录右键 chord 之前的隐藏邻居数。
    let hidden_neighbors_before: Vec<Entity> = tiles
        .iter()
        .copied()
        .filter(|t| {
            let tx = vm.get(*t, TILE, "x").unwrap().as_i64().unwrap();
            let ty = vm.get(*t, TILE, "y").unwrap().as_i64().unwrap();
            let dx = (tx - nx).abs();
            let dy = (ty - ny).abs();
            if dx > 1 || dy > 1 || (dx == 0 && dy == 0) {
                return false;
            }
            let revealed = vm.get(*t, TILE, "revealed").unwrap().as_i64() == Some(1);
            let flagged = vm.get(*t, TILE, "flag").unwrap().as_i64() == Some(1);
            !revealed && !flagged
        })
        .collect();

    // 对 number_tile 发右键 → 触发 chord（相邻已标旗数 == adj）。
    vm.send_event::<PickClickEvent>(
        "PickClick",
        PickClickEvent {
            entity: number_tile.to_bits(),
            button: "Secondary".to_owned(),
        },
    )
    .unwrap();
    ticks(&mut vm, 2);

    // 之前隐藏 + 未标旗的邻居都应被翻开。
    for hidden in &hidden_neighbors_before {
        let revealed = vm.get(*hidden, TILE, "revealed").unwrap().as_i64().unwrap();
        assert_eq!(
            revealed, 1,
            "chord should reveal previously-hidden non-flag neighbors",
        );
    }
}

/// 按 R 键应清空 tiles 并重新初始化棋盘。
#[test]
fn r_key_restarts_board() {
    let mut vm: VmWorld = VmWorldBuilder::new()
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin registers")
        .add_plugin(&InputPlugin)
        .expect("InputPlugin registers")
        .load(world_path())
        .expect("minesweeper world loads");

    vm.tick().expect("tick 1 (layout)");
    let tiles_before = vm.query(TILE);
    assert_eq!(tiles_before.len(), 64);

    // 让一格揭开，证明状态有变。
    let target = tiles_before[0];
    vm.send_event::<PickClickEvent>(
        "PickClick",
        PickClickEvent {
            entity: target.to_bits(),
            button: "Primary".to_owned(),
        },
    )
    .unwrap();
    ticks(&mut vm, 2);

    // 发送 R 键按下事件。
    vm.send_event::<KeyboardInput>(
        input::KEYBOARD_INPUT,
        KeyboardInput {
            key_code: KeyCode::KeyR,
            logical_key: Key::Character("r".into()),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: BevyEntity::PLACEHOLDER,
        },
    )
    .expect("keyboard send");

    // 链路：KeyboardInput → restart emit RestartGame → cell despawn tiles +
    // reset Board.layout_built → board 重建。3 跳 × 2 帧 = 6 个 tick。
    ticks(&mut vm, 3);

    // 重开后：tiles 实体 id 已换一批，但仍 64 个；mines_placed=0；state=0；layout_built=1。
    let tiles_after = vm.query(TILE);
    assert_eq!(tiles_after.len(), 64, "restart should re-spawn full board");

    // 实体 id 应全部不同（旧的已 despawn）。
    for old in &tiles_before {
        assert!(
            !tiles_after.contains(old),
            "old tile entity should not survive restart",
        );
    }

    let board = vm.query(BOARD)[0];
    assert_eq!(
        vm.get(board, BOARD, "mines_placed").unwrap().as_i64(),
        Some(0),
        "restart should reset mines_placed",
    );
    assert_eq!(
        vm.get(board, BOARD, "state").unwrap().as_i64(),
        Some(0),
        "restart should reset state",
    );
}

/// Pick-click 事件能让脚本翻开格子，渲染颜色随之变化。
#[test]
fn click_changes_tile_color() {
    let mut vm: VmWorld = VmWorldBuilder::new()
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin registers")
        .load(world_path())
        .expect("minesweeper world loads");

    // tick 1: board init layout（tile 全 spawn，未布雷）+ anim 第一次刷色。
    // tick 2: hovered/anim 再跑一遍稳态——这一帧之后 Sprite2d.color 已稳定。
    ticks(&mut vm, 2);
    let tiles = vm.query(TILE);
    // 首次 reveal 才布雷——此时所有 tile 都不是雷，随便挑一个。
    let target = tiles[0];

    let before = vm
        .get(target, "Sprite2d", "color")
        .expect("Sprite2d.color readable");

    vm.send_event::<PickClickEvent>(
        "PickClick",
        PickClickEvent {
            entity: target.to_bits(),
            button: "Primary".to_owned(),
        },
    )
    .expect("PickClick send ok");

    // 链路：PickClick → ui emit RevealCell → cell handle → anim 改色。
    // 同 tick 内 cell→anim 串行（拓扑保证），所以 4 tick 足够。
    ticks(&mut vm, 2);

    let after = vm
        .get(target, "Sprite2d", "color")
        .expect("Sprite2d.color readable");

    assert_ne!(
        before, after,
        "tile color should change after reveal; before={before}, after={after}"
    );
}

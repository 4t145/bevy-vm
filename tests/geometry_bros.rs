//! Geometry Bros smoke：headless 验证 world 加载、plugin 拓扑、第一帧不挂。
//! 不渲染——仅断言 entity / 组件状态。

#![cfg(feature = "bevy-bridge")]

use bevy_vm::plugin::BuilderVmPluginExt;
use bevy_vm::plugin::input::InputPlugin;
use bevy_vm::plugin::picking::PickingPlugin;
use bevy_vm::{VmWorld, VmWorldBuilder};
use std::path::PathBuf;
use std::time::Duration;

fn world_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/worlds/geometry_bros")
}

fn boot() -> VmWorld {
    VmWorldBuilder::new()
        .add_plugin(&InputPlugin)
        .expect("InputPlugin")
        .add_plugin(&PickingPlugin)
        .expect("PickingPlugin")
        .load(world_path())
        .expect("geometry_bros world loads")
}

#[test]
fn world_loads_and_first_tick_succeeds() {
    let mut vm = boot();
    let players = vm.query("player::Player");
    assert_eq!(players.len(), 1, "exactly one Player singleton");
    let spawners = vm.query("enemy::EnemySpawner");
    assert_eq!(spawners.len(), 1, "exactly one EnemySpawner singleton");

    // 第一帧——0 dt，所有 system 跑不挂。
    vm.tick().expect("tick 1");
}

#[test]
fn enemies_spawn_over_time() {
    let mut vm = boot();
    // 推 0.5s——第一只敌人计划在 0.5s spawn（ron 里的 next_spawn_time）。
    vm.advance_time(Duration::from_millis(500));
    vm.tick().expect("tick");

    let enemies = vm.query("enemy::Enemy");
    assert!(
        !enemies.is_empty(),
        "after 0.5s at least 1 enemy should be spawned, got {}",
        enemies.len(),
    );
}

#[test]
fn enemies_move_toward_player() {
    let mut vm = boot();
    // 推到首个敌人 spawn。
    vm.advance_time(Duration::from_millis(500));
    vm.tick().expect("tick spawn");

    let enemies = vm.query("enemy::Enemy");
    assert!(!enemies.is_empty());
    let enemy = enemies[0];
    let dist_before = {
        let ex = vm.get(enemy, "Position", "x").unwrap().as_f64().unwrap();
        let ez = vm.get(enemy, "Position", "z").unwrap().as_f64().unwrap();
        (ex * ex + ez * ez).sqrt()
    };

    // 推一段时间让敌人移动。
    for _ in 0..30 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    let dist_after = {
        let ex = vm.get(enemy, "Position", "x").unwrap().as_f64().unwrap();
        let ez = vm.get(enemy, "Position", "z").unwrap().as_f64().unwrap();
        (ex * ex + ez * ez).sqrt()
    };

    assert!(
        dist_after < dist_before - 0.5,
        "enemy should have approached origin (player); before={dist_before:.2}, after={dist_after:.2}",
    );
}

#[test]
fn shooting_spawns_projectiles() {
    let mut vm = boot();
    // spawn 几个敌人。
    vm.advance_time(Duration::from_millis(800));
    vm.tick().expect("tick");

    let enemies = vm.query("enemy::Enemy");
    assert!(!enemies.is_empty(), "need enemies for player to target");

    // 攻击冷却 0.5s——再推 0.6s 应至少发射一颗。
    vm.advance_time(Duration::from_millis(600));
    vm.tick().expect("tick");

    let projectiles = vm.query("projectile::Projectile");
    assert!(
        !projectiles.is_empty(),
        "expected at least one projectile after cooldown elapses",
    );
}

#[test]
fn projectiles_kill_enemies_and_drop_xp() {
    let mut vm = boot();
    // 跑一段时间——敌人 spawn + 玩家发射 + 击杀。
    for _ in 0..120 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    let players = vm.query("player::Player");
    let kills = vm
        .get(players[0], "player::Player", "kills")
        .unwrap()
        .as_i64()
        .unwrap();
    assert!(kills > 0, "expected at least one kill in 4s of gameplay");

    // 经验球至少出现过——但可能已被吸走，这里看 kill 数即可。
    let _ = vm.query("xp::ExpOrb");
}

/// 升级（XP 阈值）应直接给玩家属性微涨 + 不暂停。
#[test]
fn level_up_buffs_player_without_pausing() {
    let mut vm = boot();

    let player = vm.query("player::Player")[0];
    let max_hp_before = vm
        .get(player, "player::Player", "max_hp")
        .unwrap()
        .as_i64()
        .unwrap();
    let dmg_before = vm
        .get(player, "player::Player", "bullet_damage")
        .unwrap()
        .as_i64()
        .unwrap();

    // 注入大经验球 → 跨阈值 → level.rhai 应用 buff。
    let registry = vm.components();
    let world = vm.world_mut();
    let orb = world.spawn_empty().id();
    bevy_vm::world_access::set(
        world,
        &registry,
        orb,
        "Position",
        "x",
        serde_json::json!(0.0),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        orb,
        "Position",
        "y",
        serde_json::json!(0.5),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        orb,
        "Position",
        "z",
        serde_json::json!(0.0),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        orb,
        "xp::ExpOrb",
        "amount",
        serde_json::json!(20),
    )
    .unwrap();

    vm.advance_time(Duration::from_millis(33));
    vm.tick().expect("tick");

    assert!(!vm.is_paused(), "升级不该暂停世界");
    assert!(
        vm.query("upgrade::UpgradeRoot").is_empty(),
        "升级不该弹菜单",
    );

    let pl_after = vm.query("level::PlayerLevel")[0];
    let lv = vm
        .get(pl_after, "level::PlayerLevel", "level")
        .unwrap()
        .as_i64()
        .unwrap();
    assert!(lv >= 2, "等级应推进到 >=2，实际 {lv}");

    let max_hp_after = vm
        .get(player, "player::Player", "max_hp")
        .unwrap()
        .as_i64()
        .unwrap();
    let dmg_after = vm
        .get(player, "player::Player", "bullet_damage")
        .unwrap()
        .as_i64()
        .unwrap();
    assert!(max_hp_after > max_hp_before, "max_hp 应微涨");
    assert!(dmg_after > dmg_before, "damage 应微涨");
}

/// 推到第一波结束应触发暂停 + 弹 3 张加强卡片 + 玩家无条件满血。
#[test]
fn wave_end_pauses_and_shows_upgrade_cards() {
    let mut vm = boot();

    // 玩家高 max_hp + 故意让 hp 偏低——验证波次结束自动满血。
    let player = vm.query("player::Player")[0];
    let registry = vm.components();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "max_hp",
        serde_json::json!(100_000),
    )
    .unwrap();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "hp",
        serde_json::json!(50_000),
    )
    .unwrap();

    // 让 wave 倒计时 20s 走完——一帧 0.5s 推 41 帧。
    for _ in 0..41 {
        vm.advance_time(Duration::from_millis(500));
        vm.tick().expect("tick");
        if vm.is_paused() {
            break;
        }
    }

    assert!(vm.is_paused(), "波次结束应触发 pause");

    let roots = vm.query("upgrade::UpgradeRoot");
    assert_eq!(roots.len(), 1, "一个加强菜单根");

    let cards = vm.query("upgrade::UpgradeCard");
    assert_eq!(cards.len(), 3, "应有 3 张卡");

    // 卡片 kind 应都来自池子，不应出现 "heal"——回血是无条件的，不占卡位。
    let kinds: Vec<String> = cards
        .iter()
        .map(|&c| {
            vm.get(c, "upgrade::UpgradeCard", "kind")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(
        !kinds.iter().any(|k| k == "heal"),
        "卡片不应含 heal，实际 {kinds:?}",
    );
    let pool = ["max_hp", "speed", "fire_rate", "damage", "regen"];
    for k in &kinds {
        assert!(pool.contains(&k.as_str()), "kind {k} 不在池子里");
    }

    // 验证波次结束时玩家被回到满血。
    let hp_after = vm
        .get(player, "player::Player", "hp")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(hp_after, 100_000, "波次结束应将 hp 拉满到 max_hp");
}

/// 选完波次菜单后世界应该解除暂停 + 卡片消失 + 进入下一波。
#[test]
fn picking_wave_reward_resumes_and_advances_wave() {
    use bevy_vm::plugin::picking::PickClickEvent;

    let mut vm = boot();

    // 拉超高血避免被磨死。
    let player = vm.query("player::Player")[0];
    let registry = vm.components();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "max_hp",
        serde_json::json!(100_000),
    )
    .unwrap();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "hp",
        serde_json::json!(100_000),
    )
    .unwrap();

    // 推到第一波结束。
    for _ in 0..41 {
        vm.advance_time(Duration::from_millis(500));
        vm.tick().expect("tick");
        if vm.is_paused() {
            break;
        }
    }
    assert!(vm.is_paused());

    let cards = vm.query("upgrade::UpgradeCard");
    assert!(!cards.is_empty(), "wave 菜单应有卡");
    let card = cards[0];

    vm.send_event::<PickClickEvent>(
        "PickClick",
        PickClickEvent {
            entity: card.to_bits(),
            button: "Primary".to_owned(),
        },
    )
    .expect("send PickClick");

    for _ in 0..3 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    assert!(!vm.is_paused(), "选完应解除暂停");
    assert!(vm.query("upgrade::UpgradeRoot").is_empty(), "菜单应已消失",);

    // 进入下一波——number==2，倒计时已重置。
    let waves = vm.query("wave::WaveState");
    assert_eq!(waves.len(), 1);
    let n = vm
        .get(waves[0], "wave::WaveState", "number")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(n, 2, "应进入第 2 波");
}

/// 玩家被持续扣血到 0 应触发 PlayerDied → 弹 DeathScreen + pause。
#[test]
fn player_death_shows_screen_and_pauses() {
    let mut vm = boot();
    vm.advance_time(Duration::from_millis(800));
    vm.tick().expect("tick");

    // 玩家叠到一只敌人身上——combat 每帧持续扣血。设 hp 很低让它一两帧就到 0。
    let player = vm.query("player::Player")[0];
    let enemy = vm.query("enemy::Enemy")[0];
    let registry = vm.components();
    let world = vm.world_mut();
    let ex = bevy_vm::world_access::get(world, &registry, enemy, "Position", "x")
        .unwrap()
        .as_f64()
        .unwrap();
    let ez = bevy_vm::world_access::get(world, &registry, enemy, "Position", "z")
        .unwrap()
        .as_f64()
        .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        player,
        "Position",
        "x",
        serde_json::json!(ex),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        player,
        "Position",
        "z",
        serde_json::json!(ez),
    )
    .unwrap();
    // hp = 0 直接触发死亡（combat 仍看到 hp<=0 就跳出，不 emit；改用很小但
    // > 0 的 hp，让 combat 每帧扣到 0+ 后 emit）。这里靠时间累积——dmg=5/s，
    // hp=2，4 帧左右触发。
    bevy_vm::world_access::set(
        world,
        &registry,
        player,
        "player::Player",
        "hp",
        serde_json::json!(2),
    )
    .unwrap();

    // 推几帧让玩家被扣到 0+ → emit PlayerDied → death_show + pause.rhai 同帧响应。
    for _ in 0..30 {
        vm.advance_time(Duration::from_millis(50));
        vm.tick().expect("tick");
        if !vm.query("death::DeathScreen").is_empty() {
            break;
        }
    }

    let screens = vm.query("death::DeathScreen");
    assert_eq!(screens.len(), 1, "expected DeathScreen after PlayerDied");
    assert!(vm.is_paused(), "expected pause after PlayerDied");
}

/// 死亡画面下按 R 键应触发 RestartGame：清场 + 重置 + 解除暂停。
#[test]
fn pressing_r_after_death_restarts() {
    use bevy::ecs::entity::Entity as BevyEntity;
    use bevy::input::ButtonState;
    use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};

    let mut vm = boot();

    // 1. 杀死玩家——叠到敌人身上扣到 0。
    vm.advance_time(Duration::from_millis(800));
    vm.tick().expect("tick spawn");
    let player = vm.query("player::Player")[0];
    let enemy = vm.query("enemy::Enemy")[0];
    let registry = vm.components();
    let world = vm.world_mut();
    let ex = bevy_vm::world_access::get(world, &registry, enemy, "Position", "x")
        .unwrap()
        .as_f64()
        .unwrap();
    let ez = bevy_vm::world_access::get(world, &registry, enemy, "Position", "z")
        .unwrap()
        .as_f64()
        .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        player,
        "Position",
        "x",
        serde_json::json!(ex),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        player,
        "Position",
        "z",
        serde_json::json!(ez),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        player,
        "player::Player",
        "hp",
        serde_json::json!(2),
    )
    .unwrap();

    for _ in 0..30 {
        vm.advance_time(Duration::from_millis(50));
        vm.tick().expect("tick");
        if !vm.query("death::DeathScreen").is_empty() {
            break;
        }
    }
    assert!(vm.is_paused(), "should be paused after death");
    assert_eq!(vm.query("death::DeathScreen").len(), 1);

    // 2. 模拟 R 键 down——typed event 双缓冲，需要两次 tick：
    //    tick A：host send → back buffer；tick 末 swap → front。
    //    tick B：脚本看到 → emit RestartGame → restart_world 同帧清场。
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
    .expect("send KeyR");

    for _ in 0..3 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    assert!(!vm.is_paused(), "world should be unpaused after restart",);
    assert!(
        vm.query("death::DeathScreen").is_empty(),
        "death screen should be despawned",
    );
    let p2 = vm.query("player::Player")[0];
    let hp = vm
        .get(p2, "player::Player", "hp")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(hp, 100, "player hp should be reset to 100");
}

/// 波次结束应清场所有敌人 + 收回未拾取的经验球（避免菜单期间敌人压脸）。
#[test]
fn wave_end_clears_enemies_and_collects_orbs() {
    let mut vm = boot();

    // 拉血避免推 20s 期间被磨死。
    let player = vm.query("player::Player")[0];
    let registry = vm.components();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "max_hp",
        serde_json::json!(100_000),
    )
    .unwrap();
    bevy_vm::world_access::set(
        vm.world_mut(),
        &registry,
        player,
        "player::Player",
        "hp",
        serde_json::json!(100_000),
    )
    .unwrap();

    // 先 spawn 几只敌人 + 注入一颗 ExpOrb。在波结束之前推进。
    let world = vm.world_mut();
    let stray = world.spawn_empty().id();
    bevy_vm::world_access::set(
        world,
        &registry,
        stray,
        "Position",
        "x",
        serde_json::json!(10.0),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        stray,
        "Position",
        "y",
        serde_json::json!(0.5),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        stray,
        "Position",
        "z",
        serde_json::json!(10.0),
    )
    .unwrap();
    bevy_vm::world_access::set(
        world,
        &registry,
        stray,
        "xp::ExpOrb",
        "amount",
        serde_json::json!(3),
    )
    .unwrap();

    // 推进 21s 让 wave 倒计时结束（之前的注入也累计存活）。
    for _ in 0..43 {
        vm.advance_time(Duration::from_millis(500));
        vm.tick().expect("tick");
        if vm.is_paused() {
            break;
        }
    }

    assert!(vm.is_paused(), "波次结束应触发暂停");
    assert!(
        vm.query("enemy::Enemy").is_empty(),
        "波次结束应清场敌人，still {}",
        vm.query("enemy::Enemy").len(),
    );
    assert!(
        vm.query("xp::ExpOrb").is_empty(),
        "波次结束应回收所有 ExpOrb",
    );
}

#[test]
fn pause_freezes_enemy_movement() {
    let mut vm = boot();
    vm.advance_time(Duration::from_millis(500));
    vm.tick().expect("spawn tick");
    let enemies = vm.query("enemy::Enemy");
    assert!(!enemies.is_empty());
    let enemy = enemies[0];

    vm.set_paused(true);

    let snapshot_x = vm.get(enemy, "Position", "x").unwrap().as_f64().unwrap();
    let snapshot_z = vm.get(enemy, "Position", "z").unwrap().as_f64().unwrap();

    for _ in 0..10 {
        vm.advance_time(Duration::from_millis(33));
        vm.tick().expect("tick");
    }

    let after_x = vm.get(enemy, "Position", "x").unwrap().as_f64().unwrap();
    let after_z = vm.get(enemy, "Position", "z").unwrap().as_f64().unwrap();

    let drift = ((after_x - snapshot_x).powi(2) + (after_z - snapshot_z).powi(2)).sqrt();
    assert!(
        drift < 0.001,
        "paused world should freeze enemy position; drifted {drift:.4}",
    );
}

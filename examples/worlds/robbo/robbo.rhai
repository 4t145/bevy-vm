// Robbo 玩家：第一帧从 board cell kind="R" 升级成 Player；后续按方向键移动。
//
// 移动判定（按优先级）：
// - 目标格是 Door 'D' + 持钥匙 → 消耗钥匙 + 走过去 + despawn 门
// - 目标格是 Capsule '!' + 已收齐螺丝 → game won；否则普通拾取
// - 目标格是 solid → 拒绝
// - 目标格是 pushable '~' + 再下一格空地 → 推
// - 目标格是 pickup → 走入 + Cell despawn + 计数
// - 目标格是空 → 直接走
//
// gameover 状态（Game.state != 0）下不响应输入。

import "sprites" as sp;
import "sounds" as snd;

let game = query_first("board::Game");
if game == () { return; }

// 死亡 / 胜利时不再吃输入。
if get(game, "board::Game", "state") != 0 { return; }

// 第一帧：扫 board::Cell with kind="R" → 创建 Player entity，despawn 那个 Cell。
let player = query_first("Player");
if player == () {
    let robbo_cell = ();
    for cell in query("board::Cell") {
        if get(cell, "board::Cell", "kind") == "R" {
            robbo_cell = cell;
            break;
        }
    }
    if robbo_cell == () { return; }

    let i = get(robbo_cell, "board::Cell", "i");
    let j = get(robbo_cell, "board::Cell", "j");
    despawn(robbo_cell);

    let p = spawn_entity();
    set(p, "Player", "i", i);
    set(p, "Player", "j", j);
    set(p, "Player", "spawned", 1);
    attach_sprite(p, #{
        image: "robbo/icons32.png",
        atlas: #{
            tile_size: [32, 32],
            columns: sp::atlas_columns(),
            rows: sp::atlas_rows(),
            index: sp::sprite_robbo(),
        },
        custom_size: [32.0, 32.0],
    });
    set_translation(p, [i.to_float() * 32.0, -j.to_float() * 32.0, 1.0]);
    snd::play(snd::snd_spawn());
    return;
}

let pi = get(player, "Player", "i");
let pj = get(player, "Player", "j");
let shift_held = get(player, "Player", "shift_held");

// 扫所有 KeyboardInput——同帧内可能有 Shift 按下 / 方向键按下，要保留
// shift 状态再处理方向键。
let dx = 0;
let dy = 0;
let direction_consumed = false;
for ev in events("KeyboardInput") {
    let key = ev.key_code;
    if key == "ShiftLeft" || key == "ShiftRight" {
        shift_held = if ev.state == "Pressed" { 1 } else { 0 };
        continue;
    }
    if direction_consumed { continue; }
    if ev.state != "Pressed" { continue; }
    if ev.repeat { continue; }
    if key == "ArrowUp"    { dy = -1; direction_consumed = true; }
    if key == "ArrowDown"  { dy =  1; direction_consumed = true; }
    if key == "ArrowLeft"  { dx = -1; direction_consumed = true; }
    if key == "ArrowRight" { dx =  1; direction_consumed = true; }
}
set(player, "Player", "shift_held", shift_held);

if dx == 0 && dy == 0 { return; }

// Shift+方向 = 射击（消耗 1 ammo）。子弹从 Robbo 隔壁格起步——如果隔壁就
// 已经是 solid 那子弹立即销毁也没关系，让 bullets.rhai 自己处理。
if shift_held == 1 {
    let ammo = get(game, "board::Game", "ammo");
    if ammo <= 0 { return; }
    set(game, "board::Game", "ammo", ammo - 1);

    let bi = pi + dx;
    let bj = pj + dy;
    let bullet = spawn_entity();
    set(bullet, "bullets::Bullet", "i", bi);
    set(bullet, "bullets::Bullet", "j", bj);
    set(bullet, "bullets::Bullet", "dx", dx);
    set(bullet, "bullets::Bullet", "dy", dy);
    let sprite_idx = if dx != 0 { sp::sprite_bullet_h() } else { sp::sprite_bullet_v() };
    attach_sprite(bullet, #{
        image: "robbo/icons32.png",
        atlas: #{
            tile_size: [32, 32],
            columns: sp::atlas_columns(),
            rows: sp::atlas_rows(),
            index: sprite_idx,
        },
        custom_size: [32.0, 32.0],
    });
    set_translation(bullet, [bi.to_float() * 32.0, -bj.to_float() * 32.0, 1.0]);
    snd::play(snd::snd_shot());
    return;
}

let ni = pi + dx;
let nj = pj + dy;

// 找目标格的 Cell（如果有）。
let target_cell = ();
let target_kind = ".";
for cell in query("board::Cell") {
    if get(cell, "board::Cell", "i") == ni && get(cell, "board::Cell", "j") == nj {
        target_cell = cell;
        target_kind = get(cell, "board::Cell", "kind");
        break;
    }
}

// 主动撞敌人 → Robbo 死。
if sp::is_enemy_char(target_kind) {
    set(game, "board::Game", "state", 2);
    return;
}

// Door：有钥匙才开。开门会消耗一把钥匙，然后走进去（despawn 门 + Robbo 移动）。
if target_kind == "D" {
    let keys = get(game, "board::Game", "keys");
    if keys <= 0 { return; }
    set(game, "board::Game", "keys", keys - 1);
    despawn(target_cell);
    set(player, "Player", "i", ni);
    set(player, "Player", "j", nj);
    set_translation(player, [ni.to_float() * 32.0, -nj.to_float() * 32.0, 1.0]);
    snd::play(snd::snd_door());
    return;
}

// Capsule：吃到时若所有螺丝都已收 → 胜利倒计时（让 transition module 切下一关）；
// 否则当 solid 撞不动。
if target_kind == "!" {
    let collected = get(game, "board::Game", "screws_collected");
    let total = get(game, "board::Game", "screws_total");
    if collected >= total {
        set(game, "board::Game", "state", 1);
        set(game, "board::Game", "won_timer", 60);   // ~1 秒（60 fps）
        despawn(target_cell);
        set(player, "Player", "i", ni);
        set(player, "Player", "j", nj);
        set_translation(player, [ni.to_float() * 32.0, -nj.to_float() * 32.0, 1.0]);
        snd::play(snd::snd_capsule());
        return;
    }
    return;
}

// solid → 撞死，不动。
if sp::is_solid_char(target_kind) {
    return;
}

// pushable → 推。检查再下一格必须空（也不能越界）。
if sp::is_pushable_char(target_kind) {
    let pi2 = ni + dx;
    let pj2 = nj + dy;
    if pi2 < 0 || pi2 >= 16 || pj2 < 0 {
        return;
    }
    let blocked = false;
    for cell in query("board::Cell") {
        if get(cell, "board::Cell", "i") == pi2 && get(cell, "board::Cell", "j") == pj2 {
            blocked = true;
            break;
        }
    }
    if blocked { return; }
    // 推：把 target_cell 移到 (pi2, pj2)，然后 Robbo 走到 (ni, nj)。
    set(target_cell, "board::Cell", "i", pi2);
    set(target_cell, "board::Cell", "j", pj2);
    set_translation(target_cell, [pi2.to_float() * 32.0, -pj2.to_float() * 32.0, 0.0]);
    set(player, "Player", "i", ni);
    set(player, "Player", "j", nj);
    set_translation(player, [ni.to_float() * 32.0, -nj.to_float() * 32.0, 1.0]);
    snd::play(snd::snd_walk());
    return;
}

// pickup → 走入 + 计数 + despawn。
let pickup_sound = ();
if sp::is_pickup_char(target_kind) {
    if target_kind == "T" {
        let n = get(game, "board::Game", "screws_collected") + 1;
        set(game, "board::Game", "screws_collected", n);
        pickup_sound = snd::snd_screw();
    } else if target_kind == "%" {
        let n = get(game, "board::Game", "keys") + 1;
        set(game, "board::Game", "keys", n);
        pickup_sound = snd::snd_key();
    } else if target_kind == "'" {
        let n = get(game, "board::Game", "ammo") + 9;   // 一弹夹 9 发
        set(game, "board::Game", "ammo", n);
        pickup_sound = snd::snd_ammo();
    }
    despawn(target_cell);
}

// 实际移动 Player。
set(player, "Player", "i", ni);
set(player, "Player", "j", nj);
set_translation(player, [ni.to_float() * 32.0, -nj.to_float() * 32.0, 1.0]);

if pickup_sound != () {
    snd::play(pickup_sound);
} else {
    // 走一格——忠实原版每步都响 walk。
    snd::play(snd::snd_walk());
}

// player：第一帧 spawn alien 球体；之后每帧消费方向键 + cake 碰撞。

import "consts" as c;

let games = query("board::Game");
if games.len() == 0 { return; }
let game = games[0];

if get(game, "board::Game", "state") != 0 {
    // game over：不响应输入，但保留 Player entity 在场上（方便 hud 显示）。
    return;
}

// 第一帧 spawn alien——以 query("Player") 为 once flag。
let players = query("Player");
let player;
if players.len() == 0 {
    player = spawn_player();
} else {
    player = players[0];
}

let dt = delta();
let cd = get(game, "board::Game", "move_cd");
cd -= dt;
if cd < 0.0 { cd = 0.0; }

// 消费 KeyboardInput：四向 → 走一格；冷却归零。
let i = get(player, "Player", "i");
let j = get(player, "Player", "j");
let rot_y = get(player, "Player", "rot_y");
let bw = c::board_w_int();
let bh = c::board_h_int();

let moved = false;
if cd <= 0.0 {
    for ev in events("KeyboardInput") {
        if ev.state != "Pressed" { continue; }
        let key = ev.key_code;
        if key == "ArrowUp" {
            if i < bw - 1 { i += 1; }
            rot_y = -90.0;
            moved = true;
        } else if key == "ArrowDown" {
            if i > 0 { i -= 1; }
            rot_y = 90.0;
            moved = true;
        } else if key == "ArrowRight" {
            if j < bh - 1 { j += 1; }
            rot_y = 180.0;
            moved = true;
        } else if key == "ArrowLeft" {
            if j > 0 { j -= 1; }
            rot_y = 0.0;
            moved = true;
        }
        if moved { break; }
    }
}

if moved {
    cd = c::move_cooldown();
    set(player, "Player", "i", i);
    set(player, "Player", "j", j);
    set(player, "Player", "rot_y", rot_y);
}

set(game, "board::Game", "move_cd", cd);

// 同步 player 视觉位置（每帧——cell 高度可能不同）。
let h = cell_height(i, j);
set_translation(player, [i.to_float(), h, j.to_float()]);
set(player, "Rotation", "y", rot_y);

// 吃 cake：与 bonus 同格则 +score、despawn bonus。
for bonus in query("bonus::Bonus") {
    let bi = get(bonus, "bonus::Bonus", "i");
    let bj = get(bonus, "bonus::Bonus", "j");
    if bi == i && bj == j {
        let score = get(game, "board::Game", "score") + c::score_eat();
        let eaten = get(game, "board::Game", "cake_eaten") + 1;
        set(game, "board::Game", "score", score);
        set(game, "board::Game", "cake_eaten", eaten);
        despawn(bonus);
    }
}

// ---- helpers ----

fn spawn_player() {
    let e = spawn_entity();
    let bw = c::board_w_int();
    let bh = c::board_h_int();
    let i = bw / 2;
    let j = bh / 2;
    set(e, "Player", "i", i);
    set(e, "Player", "j", j);
    set(e, "Player", "rot_y", -90.0);
    attach_scene(e, "models/AlienCake/alien.glb#Scene0");
    let h = cell_height(i, j);
    set_translation(e, [i.to_float(), h, j.to_float()]);
    e
}

fn cell_height(i, j) {
    for cell in query("board::Cell") {
        if get(cell, "board::Cell", "i") == i && get(cell, "board::Cell", "j") == j {
            return get(cell, "board::Cell", "height");
        }
    }
    0.0
}

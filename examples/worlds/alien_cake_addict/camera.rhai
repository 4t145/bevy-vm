// 相机平滑追随：focus 目标 = 玩家位置（有蛋糕则 + 蛋糕位置中点）。
//
// 用 board::Game 单例存 focus_target / focus_now（避免每帧 query 全部）。

import "consts" as c;

let games = query("board::Game");
if games.len() == 0 { return; }
let game = games[0];

let board_w = c::board_w();
let board_h = c::board_h();

// 算 focus_target：
let players = query("player::Player");
let bonuses = query("bonus::Bonus");

let target_x = board_w / 2.0;
let target_y = 0.0;
let target_z = board_h / 2.0 - 0.5;

if players.len() > 0 {
    let player = players[0];
    let px = get(player, "Position", "x");
    let py = get(player, "Position", "y");
    let pz = get(player, "Position", "z");

    if bonuses.len() > 0 {
        let bonus = bonuses[0];
        let bx = get(bonus, "Position", "x");
        let by = get(bonus, "Position", "y");
        let bz = get(bonus, "Position", "z");
        target_x = (px + bx) * 0.5;
        target_y = (py + by) * 0.5;
        target_z = (pz + bz) * 0.5;
    } else {
        target_x = px;
        target_y = py;
        target_z = pz;
    }
}

set(game, "board::Game", "focus_target_x", target_x);
set(game, "board::Game", "focus_target_y", target_y);
set(game, "board::Game", "focus_target_z", target_z);

// 平滑 focus_now → focus_target，速度按距离 + dt 缩放。
let now_x = get(game, "board::Game", "focus_now_x");
let now_y = get(game, "board::Game", "focus_now_y");
let now_z = get(game, "board::Game", "focus_now_z");

let dx = target_x - now_x;
let dy = target_y - now_y;
let dz = target_z - now_z;
let dist_sq = dx * dx + dy * dy + dz * dz;
let threshold = c::camera_focus_threshold();

if dist_sq > threshold * threshold {
    let speed = c::camera_focus_speed() * delta();
    now_x += dx * speed;
    now_y += dy * speed;
    now_z += dz * speed;
    set(game, "board::Game", "focus_now_x", now_x);
    set(game, "board::Game", "focus_now_y", now_y);
    set(game, "board::Game", "focus_now_z", now_z);
}

// 相机 transform.looking_at(focus_now)——位置不动，只改朝向。
for cam in query("MainCam") {
    look_at(cam, [now_x, now_y, now_z], [0.0, 1.0, 0.0]);
}

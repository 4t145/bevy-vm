// bonus：每 5 秒刷新一个金色 cuboid"蛋糕"；过期没吃 -3 分；旋转 + 呼吸。

import "consts" as c;

let games = query("board::Game");
if games.len() == 0 { return; }
let game = games[0];

if get(game, "board::Game", "state") != 0 { return; }

// rotate / scale 已存在的 bonus（每帧）。
for bonus in query("Bonus") {
    let cur_y = get(bonus, "Rotation", "y");
    cur_y = (cur_y + 60.0 * delta()) % 360.0;
    set(bonus, "Rotation", "y", cur_y);

    // scale 呼吸：1 + |score/10 * sin(time)|
    let score = get(game, "board::Game", "score");
    let s = 1.0 + ((score.to_float() / 10.0) * time().sin()).abs();
    let bi = get(bonus, "Bonus", "i");
    let bj = get(bonus, "Bonus", "j");
    let h = cell_height(bi, bj);
    set_transform(bonus, [bi.to_float(), h + 0.2, bj.to_float()], [0.0, cur_y, 0.0], [s, s, s]);
}

// 倒计时 spawn_cd。
let cd = get(game, "board::Game", "spawn_cd") - delta();
if cd > 0.0 {
    set(game, "board::Game", "spawn_cd", cd);
    return;
}

// 时间到——清旧蛋糕（如果有）扣分；分够低进 game over。
let despawned_old = false;
for bonus in query("Bonus") {
    despawn(bonus);
    despawned_old = true;
}
if despawned_old {
    let s = get(game, "board::Game", "score") - c::score_miss();
    set(game, "board::Game", "score", s);
    if s <= c::score_lose_threshold() {
        set(game, "board::Game", "state", 1);   // game over
        set(game, "board::Game", "spawn_cd", c::spawn_interval());
        return;
    }
}

// spawn 新蛋糕——落点随机但避开玩家当前格。
let players = query("player::Player");
let pi = -1;
let pj = -1;
if players.len() > 0 {
    pi = get(players[0], "player::Player", "i");
    pj = get(players[0], "player::Player", "j");
}

let bw = c::board_w_int();
let bh = c::board_h_int();
let bi = 0;
let bj = 0;
loop {
    bi = random_int(0, bw);
    bj = random_int(0, bh);
    if bi != pi || bj != pj { break; }
}

let bonus = spawn_entity();
set(bonus, "Bonus", "i", bi);
set(bonus, "Bonus", "j", bj);
attach_scene(bonus, "models/AlienCake/cakeBirthday.glb#Scene0");
let h = cell_height(bi, bj);
set_translation(bonus, [bi.to_float(), h + 0.2, bj.to_float()]);

// 蛋糕顶上挂个黄色暖光，模拟"生日蜡烛"——作为 bonus 的子节点跟着移动。
let candle = spawn_entity();
attach_point_light(candle, #{
    color: srgb(1.0, 1.0, 0.0),
    intensity: 500_000.0,
    range: 10.0,
});
set_translation(candle, [0.0, 2.0, 0.0]);
set_parent(candle, bonus);

set(game, "board::Game", "spawn_cd", c::spawn_interval());

fn cell_height(i, j) {
    for cell in query("board::Cell") {
        if get(cell, "board::Cell", "i") == i && get(cell, "board::Cell", "j") == j {
            return get(cell, "board::Cell", "height");
        }
    }
    0.0
}

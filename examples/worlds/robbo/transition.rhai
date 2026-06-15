// state==1 倒计时切关；超过最后关 → state=3 complete。

let game = query_first("board::Game");
if game == () { return; }

let state = get(game, "board::Game", "state");
if state != 1 { return; }

let timer = get(game, "board::Game", "won_timer");
if timer > 0 {
    set(game, "board::Game", "won_timer", timer - 1);
    return;
}

// 切关：先看是不是最后一关。
let levels = load_config("levels.json");
let total = config_get(levels, "count");
let next_idx = get(game, "board::Game", "level_idx") + 1;
if next_idx >= total {
    set(game, "board::Game", "state", 3);   // complete
    return;
}

// 清场（同 reset.rhai 但保留 level_idx + 推进它）。
for c in query("board::Cell")      { despawn(c); }
for p in query("robbo::Player")    { despawn(p); }
for b in query("bullets::Bullet")  { despawn(b); }
for r in query("board::BoardRoot") { despawn(r); }
for cam in query("board::MainCam") { despawn(cam); }
for root in query("GameRoot")      { despawn(root); }

set(game, "board::Game", "level_idx", next_idx);
set(game, "board::Game", "screws_collected", 0);
set(game, "board::Game", "screws_total", 0);
set(game, "board::Game", "state", 0);
set(game, "board::Game", "won_timer", 0);
set(game, "board::Game", "tick_count", 0);
set(game, "board::Game", "board_built", 0);
// keys / ammo 保留——原版是跨关累积。

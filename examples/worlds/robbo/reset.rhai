// 监听键盘：
//   R = 重置当前关（state=3 全通关时倒回第 1 关）
//   N = 调试用，跳到下一关（不清 keys/ammo）
//
// 触发后清掉 game-side entity 让 board.rhai 下一帧重建。

let game = query_first("board::Game");
if game == () { return; }

let trigger_r = false;
let trigger_n = false;
for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    if ev.state != "Pressed" { continue; }
    if ev.key_code == "KeyR" { trigger_r = true; break; }
    if ev.key_code == "KeyN" { trigger_n = true; break; }
}
if !trigger_r && !trigger_n { return; }

// 清场：所有 game-side entity。
// - Cell / Player / Bullet：平铺 entity，逐个 despawn。
// - GameRoot：HUD 子树挂在它上面，linked_spawn 级联。
// - MainCam：board.rhai 重建时会重新 spawn——不清掉就会"两个相机"。
for c in query("board::Cell")      { despawn(c); }
for p in query("robbo::Player")    { despawn(p); }
for b in query("bullets::Bullet")  { despawn(b); }
for r in query("board::BoardRoot") { despawn(r); }
for cam in query("board::MainCam") { despawn(cam); }
for root in query("GameRoot")      { despawn(root); }

// reset Game 状态：
// - R + state==3（已通关全部）→ 倒回 level 0
// - N → level_idx++（环回 0），keys/ammo 保留
// - 其它情况 → level_idx 保留（重玩当前关）
let cur_state = get(game, "board::Game", "state");
let cur_level = get(game, "board::Game", "level_idx");

let levels = load_config("levels.json");
let total = config_get(levels, "count");

if trigger_n {
    let next = cur_level + 1;
    if next >= total { next = 0; }
    set(game, "board::Game", "level_idx", next);
} else if cur_state == 3 {
    set(game, "board::Game", "level_idx", 0);
}

set(game, "board::Game", "screws_collected", 0);
set(game, "board::Game", "screws_total", 0);
if !trigger_n {
    // R 清掉累计；N 跳关保留 keys/ammo（贴近正常流程）。
    set(game, "board::Game", "keys", 0);
    set(game, "board::Game", "ammo", 0);
}
set(game, "board::Game", "state", 0);
set(game, "board::Game", "won_timer", 0);
set(game, "board::Game", "tick_count", 0);
set(game, "board::Game", "board_built", 0);

// 敌人 AI：蝴蝶 V、熊 @ \*、鸟 ^。每 ENEMY_TICK_RATE 帧推进一次。
//
// 蝴蝶（Eyes）：50% 随机方向 / 50% 朝 Robbo——优先 X 轴试，X 不行试 Y。
// 熊：左/右手迷宫规则（@ 左手 / * 右手）。每个熊存一个朝向（dir_idx 0-3 =
//   E S W N），**初始 0=East**（与原版默认对齐）。
// 鸟：直线移动，撞墙反向（来回跳）。每只鸟存方向；初始向右。
//
// 跟玩家相邻不直接死——但走到玩家格 → state=2。Robbo 主动撞敌也死（在 robbo.rhai）。
//
// 朝向 / 方向用 (dir_i, dir_j) 整数 (-1/0/1) 存——本来想用 dir_idx 索引到
// 4 方向表，但 board::Cell 只有 i/j/kind 三字段。给敌人单独 marker 组件存
// dir 太繁琐，关键的"熊朝向影响 AI"通过 bear_dir 字段记。

import "sounds" as snd;

const ENEMY_TICK_RATE = 12;
const RANDOM_MOVE_PROP_PERMIL = 500;   // 50% (千分制——random_int 整数好做)

let game = query_first("board::Game");
if game == () { return; }

if get(game, "board::Game", "state") != 0 { return; }

let tick = get(game, "board::Game", "tick_count");
if tick % ENEMY_TICK_RATE != 0 { return; }

let player = query_first("robbo::Player");
if player == () { return; }
let pi = get(player, "robbo::Player", "i");
let pj = get(player, "robbo::Player", "j");

// 棋盘 occupied 列表（除墙外的所有 cell）——蝴蝶 / 熊 / 鸟用同一 occupied 检查。
let occupied = [];
for cell in query("board::Cell") {
    occupied.push([get(cell, "board::Cell", "i"), get(cell, "board::Cell", "j")]);
}

let killed_player = false;

fn is_blocked(ni, nj, occupied) {
    for slot in occupied {
        if slot[0] == ni && slot[1] == nj { return true; }
    }
    false
}

fn move_enemy(cell, ni, nj, occupied) {
    set(cell, "board::Cell", "i", ni);
    set(cell, "board::Cell", "j", nj);
    set_translation(cell, [ni.to_float() * 32.0, -nj.to_float() * 32.0, 0.0]);
    occupied.push([ni, nj]);
}

// ---- 蝴蝶：50% 随机 / 50% 朝玩家 -------------------------------------
for cell in query("board::Cell") {
    let kind = get(cell, "board::Cell", "kind");
    if kind != "V" { continue; }

    let ei = get(cell, "board::Cell", "i");
    let ej = get(cell, "board::Cell", "j");

    let step_x = 0;
    let step_y = 0;
    let r = random_int(0, 1000);
    if r < RANDOM_MOVE_PROP_PERMIL {
        // 随机一个 4 方向。
        let d = random_int(0, 4);
        if d == 0 { step_x =  1; }
        else if d == 1 { step_y =  1; }
        else if d == 2 { step_x = -1; }
        else { step_y = -1; }
    } else {
        // 朝玩家。
        let dx_to = pi - ei;
        let dy_to = pj - ej;
        step_x = if dx_to == 0 { 0 } else if dx_to > 0 { 1 } else { -1 };
        step_y = if dy_to == 0 { 0 } else if dy_to > 0 { 1 } else { -1 };
    }

    // 优先 X 轴尝试，不行 Y 轴。
    let dst_x = [ei + step_x, ej];
    let dst_y = [ei, ej + step_y];
    let target = ();
    if step_x != 0 && (dst_x[0] == pi && dst_x[1] == pj || !is_blocked(dst_x[0], dst_x[1], occupied)) {
        target = dst_x;
    } else if step_y != 0 && (dst_y[0] == pi && dst_y[1] == pj || !is_blocked(dst_y[0], dst_y[1], occupied)) {
        target = dst_y;
    }
    if target == () { continue; }

    if target[0] == pi && target[1] == pj {
        killed_player = true;
        move_enemy(cell, target[0], target[1], occupied);
        continue;
    }
    move_enemy(cell, target[0], target[1], occupied);
}

// ---- 鸟 ^：直线移动，撞墙反向 -----------------------------------------
for cell in query("board::Cell") {
    let kind = get(cell, "board::Cell", "kind");
    if kind != "^" { continue; }

    let ei = get(cell, "board::Cell", "i");
    let ej = get(cell, "board::Cell", "j");
    let dx = get(cell, "board::Cell", "dir_i");
    // 默认朝右 (1, 0)——'dir_i' 字段值为 0 表示未初始化。
    if dx == 0 {
        dx = 1;
        set(cell, "board::Cell", "dir_i", 1);
    }
    let nx = ei + dx;
    if nx == pi && ej == pj {
        killed_player = true;
        move_enemy(cell, nx, ej, occupied);
        continue;
    }
    if is_blocked(nx, ej, occupied) || nx < 0 || nx >= 16 {
        // 反向。
        set(cell, "board::Cell", "dir_i", -dx);
    } else {
        move_enemy(cell, nx, ej, occupied);
    }
}

// ---- 熊 @ * ：左/右手迷宫 -------------------------------------------
//
// 算法：dir_idx 0=East 1=South 2=West 3=North。
// 顺时针四方向 lookup：dir → (dx, dy)。
// @ (bear.0=false) 走左手：r1=逆时针，r2=顺时针。
// * (bear.0=true)  走右手：r1=顺时针，r2=逆时针。
// 每帧：
//   1. 试 r1(dir) → 能走就走 + 转向 r1
//   2. 否则试 dir → 能走就走（保持方向）
//   3. 否则试 r2(dir) → 不动只转向 r2
//   4. 否则 r2(r2(dir))（即反方向）→ 不动只转向

fn dir_to_step(idx) {
    if idx == 0 { return [1, 0]; }
    if idx == 1 { return [0, 1]; }
    if idx == 2 { return [-1, 0]; }
    [0, -1]
}
fn rotate_cw(idx) { (idx + 1) % 4 }
fn rotate_ccw(idx) { (idx + 3) % 4 }

for cell in query("board::Cell") {
    let kind = get(cell, "board::Cell", "kind");
    if kind != "@" && kind != "*" { continue; }

    let ei = get(cell, "board::Cell", "i");
    let ej = get(cell, "board::Cell", "j");
    let dir_idx = get(cell, "board::Cell", "dir_i");   // 复用 dir_i 字段
    if dir_idx < 0 || dir_idx >= 4 { dir_idx = 0; }

    let r1 = if kind == "@" { rotate_ccw(dir_idx) } else { rotate_cw(dir_idx) };
    let r2 = if kind == "@" { rotate_cw(dir_idx) } else { rotate_ccw(dir_idx) };

    let s1 = dir_to_step(r1);
    let sf = dir_to_step(dir_idx);
    let s2 = dir_to_step(r2);

    let p1 = [ei + s1[0], ej + s1[1]];
    let pf = [ei + sf[0], ej + sf[1]];
    let p2 = [ei + s2[0], ej + s2[1]];

    let touch_p1 = p1[0] == pi && p1[1] == pj;
    let touch_pf = pf[0] == pi && pf[1] == pj;

    if touch_p1 || !is_blocked(p1[0], p1[1], occupied) {
        if touch_p1 { killed_player = true; }
        move_enemy(cell, p1[0], p1[1], occupied);
        set(cell, "board::Cell", "dir_i", r1);
    } else if touch_pf || !is_blocked(pf[0], pf[1], occupied) {
        if touch_pf { killed_player = true; }
        move_enemy(cell, pf[0], pf[1], occupied);
    } else if !is_blocked(p2[0], p2[1], occupied) {
        // 不走，只转向（与原版一致：试 r2 能走则只转向 r2）
        set(cell, "board::Cell", "dir_i", r2);
    } else {
        set(cell, "board::Cell", "dir_i", rotate_cw(rotate_cw(dir_idx)));
    }
}

// ---- Magnet：原版规则是"磁场前方一格 deadly"。我们简化为：Robbo 与
// 磁场相邻（4 方向）就死——保留体感"靠近磁场是危险的"。
for cell in query("board::Cell") {
    let kind = get(cell, "board::Cell", "kind");
    if kind != "M" { continue; }
    let mi = get(cell, "board::Cell", "i");
    let mj = get(cell, "board::Cell", "j");
    let dx = pi - mi;
    let dy = pj - mj;
    if (dx == 0 && dy.abs() == 1) || (dy == 0 && dx.abs() == 1) {
        killed_player = true;
        break;
    }
}

if killed_player {
    set(game, "board::Game", "state", 2);
    snd::play(snd::snd_burn());
}

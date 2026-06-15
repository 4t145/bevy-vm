// piece plugin 脚本：核心规则集中在此。
//
// 顺序：RestartGame → 输入 → gravity → lock 检查 → SpawnNext。
// 每帧内事件同帧消费——RestartGame 优先（清空才有意义重头来），其余按上方
// 顺序处理。

import "helpers" as h;

let board_q  = query("board::Board");
let piece_q  = query("Piece");
let score_q  = query("Score");
if board_q.len() == 0 || piece_q.len() == 0 || score_q.len() == 0 {
    return;
}
let board = board_q[0];
let piece = piece_q[0];
let score = score_q[0];

// ---- RestartGame ---------------------------------------------------------

let restart = false;
for _ev in events("RestartGame") {
    restart = true;
}
if restart {
    handle_restart(board, piece, score);
    // restart 后本帧不再处理其他事件——让下一帧的 SpawnNext 安静起步。
    return;
}

// ---- 第一帧：自动开局 ----------------------------------------------------

if get(piece, "Piece", "kind") == 0 && get(score, "Score", "state") == 0 {
    if get(score, "Score", "start_time") == 0.0 {
        set(score, "Score", "start_time", time());
    }
    spawn_next_piece(board, piece, score);
    return;
}

// ---- 等下一帧再 spawn ----------------------------------------------------
//
// 锁定时 emit SpawnNext，本帧不立即 spawn——避免 lock + spawn 同帧执行让
// 用户看到一帧"已经吐出新块但旧块还没落地"的错位。
let need_spawn = false;
for _ev in events("SpawnNext") {
    need_spawn = true;
}
if need_spawn {
    spawn_next_piece(board, piece, score);
    return;
}

// ---- 游戏结束就只等 RestartGame ------------------------------------------

if get(score, "Score", "state") != 0 {
    return;
}

// ---- 输入 ----------------------------------------------------------------
//
// 左 / 右：单次平移；↓：标记 soft_drop（按下=1 释放=0）；↑：顺时针旋转；
// Space：硬降（直接落到底）。
//
// 同帧多个事件按 ecs 顺序消费——多个方向键同帧很罕见，逐个处理即可。

let dx = 0;
let rotate = 0;
let hard_drop = false;
let soft_drop = get(piece, "Piece", "soft_drop");

for ev in events("KeyboardInput") {
    let key = ev.key_code;
    if key == "ArrowDown" {
        soft_drop = if ev.state == "Pressed" { 1 } else { 0 };
        continue;
    }
    if ev.state != "Pressed" { continue; }
    // 平移 / 旋转支持按住连发：参考实现是 DAS+ARR；这里简化为允许 repeat。
    if key == "ArrowLeft"  { dx -= 1; }
    if key == "ArrowRight" { dx += 1; }
    if key == "ArrowUp" && !ev.repeat { rotate += 1; }
    if key == "Space" && !ev.repeat   { hard_drop = true; }
}
set(piece, "Piece", "soft_drop", soft_drop);

// 平移
if dx != 0 {
    try_translate(piece, board, dx, 0);
}

// 旋转——撞墙 / 撞已落格则回滚（无 wall-kick）。
if rotate > 0 {
    let old_rot = get(piece, "Piece", "rotation");
    set(piece, "Piece", "rotation", (old_rot + 1) % 4);
    if !piece_fits(piece, board, 0, 0) {
        set(piece, "Piece", "rotation", old_rot);
    }
}

// 硬降：循环下落到不能动为止，立刻锁定。
if hard_drop {
    while try_translate(piece, board, 0, 1) { }
    lock_piece(piece, board, score);
    return;
}

// ---- gravity / lock-delay -----------------------------------------------

let dt = delta();
let level = get(score, "Score", "level");
let interval = gravity_interval(level, soft_drop == 1);

let gt = get(piece, "Piece", "gravity_timer") + dt;
while gt >= interval {
    gt -= interval;
    if !try_translate(piece, board, 0, 1) {
        // 触底——交给 lock-delay 流程。
        gt = 0.0;
        break;
    }
}
set(piece, "Piece", "gravity_timer", gt);

// 检查是否触底（向下 +1 装不下）。触底后 lock_timer 累加；任何成功平移 /
// 旋转都会让 piece 重新悬空，下一帧 lock_timer 自然清零。
if !piece_fits(piece, board, 0, 1) {
    let lt = get(piece, "Piece", "lock_timer") + dt;
    if lt >= h::lock_delay() {
        lock_piece(piece, board, score);
        return;
    }
    set(piece, "Piece", "lock_timer", lt);
} else {
    set(piece, "Piece", "lock_timer", 0.0);
}

// ===========================================================================
// helpers
// ===========================================================================

fn gravity_interval(level, soft) {
    let base = h::gravity_base();
    let lvl = level - 1;
    let factor = 1.0;
    let i = 0;
    while i < lvl { factor *= h::gravity_falloff(); i += 1; }
    let interval = base * factor;
    if interval < h::gravity_min() { interval = h::gravity_min(); }
    if soft { interval *= h::soft_drop_factor(); }
    interval
}

// 平移 (dx,dy)；成功 true，撞墙 / 落地 false。
fn try_translate(piece, board, dx, dy) {
    if !piece_fits(piece, board, dx, dy) { return false; }
    let nx = get(piece, "Piece", "x") + dx;
    let ny = get(piece, "Piece", "y") + dy;
    set(piece, "Piece", "x", nx);
    set(piece, "Piece", "y", ny);
    true
}

// 把 piece 的 4 格按 (px+dx, py+dy) 检查：是否越界 / 是否撞已落 Cell.value。
fn piece_fits(piece, board, dx, dy) {
    let kind = get(piece, "Piece", "kind");
    let rot  = get(piece, "Piece", "rotation");
    let px   = get(piece, "Piece", "x") + dx;
    let py   = get(piece, "Piece", "y") + dy;
    let w    = get(board, "board::Board", "width");
    let bh   = get(board, "board::Board", "height");

    for off in h::shape_offsets(kind, rot) {
        let cx = px + off[0];
        let cy = py + off[1];
        if cx < 0 || cx >= w { return false; }
        if cy >= bh           { return false; }
        // cy < 0 算合法（spawn 时 piece 顶部还没进盘）。
        if cy < 0 { continue; }
        let cell = cell_at(cx, cy);
        if cell == () { return false; }
        if get(cell, "board::Cell", "value") > 0 { return false; }
    }
    true
}

fn cell_at(x, y) {
    for c in query("board::Cell") {
        if get(c, "board::Cell", "x") == x && get(c, "board::Cell", "y") == y {
            return c;
        }
    }
    ()
}

// 锁定：把 4 格 value 写入 Cell；emit ClearRows 让 lines plugin 同帧扫；emit
// SpawnNext 让自己下一帧吐新块。如果 piece 顶部尚未完全进盘（任何 cy<0）
// → game over。
fn lock_piece(piece, board, score) {
    let kind = get(piece, "Piece", "kind");
    let rot  = get(piece, "Piece", "rotation");
    let px   = get(piece, "Piece", "x");
    let py   = get(piece, "Piece", "y");
    let any_above = false;

    for off in h::shape_offsets(kind, rot) {
        let cx = px + off[0];
        let cy = py + off[1];
        if cy < 0 { any_above = true; continue; }
        let cell = cell_at(cx, cy);
        if cell != () {
            set(cell, "board::Cell", "value", kind);
        }
    }

    set(piece, "Piece", "kind", 0);
    set(piece, "Piece", "gravity_timer", 0.0);
    set(piece, "Piece", "lock_timer", 0.0);

    if any_above {
        set(score, "Score", "state", 1);
        set(score, "Score", "end_time", time());
        return;
    }

    emit("lines::ClearRows", #{});
    emit("SpawnNext", #{});
}

// 抽 7-bag：bag_pos==0 时洗牌（kind 1..7），bag_pos++ 取出。
fn spawn_next_piece(board, piece, score) {
    let kind = draw_from_bag(piece);
    let w  = get(board, "board::Board", "width");

    set(piece, "Piece", "kind", kind);
    set(piece, "Piece", "rotation", 0);
    // 锚点：x 居中（按 width 折半），y=0（顶行）。I 块 4 格水平所以
    // 居中向左偏 1；其它 3x3 / 2x2 居中即可——这里简化用 width/2。
    set(piece, "Piece", "x", w / 2);
    set(piece, "Piece", "y", 0);
    set(piece, "Piece", "gravity_timer", 0.0);
    set(piece, "Piece", "lock_timer", 0.0);

    // 出生位置即被占——game over。
    if !piece_fits(piece, board, 0, 0) {
        set(score, "Score", "state", 1);
        set(score, "Score", "end_time", time());
        set(piece, "Piece", "kind", 0);
    }
}

fn draw_from_bag(piece) {
    let pos = get(piece, "Piece", "bag_pos");
    if pos == 0 {
        // 洗牌：先填 1..7，再 Fisher–Yates。
        let arr = [1, 2, 3, 4, 5, 6, 7];
        let n = 7;
        let i = n - 1;
        while i > 0 {
            let j = random_int(0, i + 1);
            let tmp = arr[i];
            arr[i] = arr[j];
            arr[j] = tmp;
            i -= 1;
        }
        set(piece, "Piece", "bag_0", arr[0]);
        set(piece, "Piece", "bag_1", arr[1]);
        set(piece, "Piece", "bag_2", arr[2]);
        set(piece, "Piece", "bag_3", arr[3]);
        set(piece, "Piece", "bag_4", arr[4]);
        set(piece, "Piece", "bag_5", arr[5]);
        set(piece, "Piece", "bag_6", arr[6]);
    }
    let kind = get(piece, "Piece", `bag_${pos}`);
    let next = (pos + 1) % 7;
    set(piece, "Piece", "bag_pos", next);
    kind
}

fn handle_restart(board, piece, score) {
    // 清空所有 Cell.value；不重建 Cell——尺寸固定。
    for cell in query("board::Cell") {
        set(cell, "board::Cell", "value", 0);
    }
    set(piece, "Piece", "kind", 0);
    set(piece, "Piece", "rotation", 0);
    set(piece, "Piece", "x", 0);
    set(piece, "Piece", "y", 0);
    set(piece, "Piece", "gravity_timer", 0.0);
    set(piece, "Piece", "lock_timer", 0.0);
    set(piece, "Piece", "soft_drop", 0);
    set(piece, "Piece", "bag_pos", 0);

    set(score, "Score", "score", 0);
    set(score, "Score", "lines", 0);
    set(score, "Score", "level", 1);
    set(score, "Score", "state", 0);
    set(score, "Score", "start_time", 0.0);
    set(score, "Score", "end_time", 0.0);
}

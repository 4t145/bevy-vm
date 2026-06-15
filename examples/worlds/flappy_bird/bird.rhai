// bird plugin 脚本：跳跃输入 + 物理积分 + 碰撞判定 + sprite 视觉刷新。
//
// 顺序：
//   1. RestartGame → 复位（bird 自己 + 不动管道，pipes 自己监听 RestartGame
//      做 despawn）
//   2. 第一次跑 → 挂 sprite child（PlayArea 必须已存在，否则跳过等下一帧）
//   3. 死亡时只刷新 sprite 位置（鸟从空中落到地面），不再吃输入
//   4. 输入：Space / ArrowUp 跳跃
//   5. 物理：vel_y += -gravity * dt（+Y 向上，gravity 拉它向下 = 减 vel_y）；
//      pos_y += vel_y * dt
//   6. 碰撞：触地 → state=1；超出顶部边界 → 钳住；与管道矩形 AABB 相交 → state=1

import "helpers" as h;

let game_q = query("game::Game");
let bird_q = query("Bird");
let play_q = query("PlayArea");
if game_q.len() == 0 || bird_q.len() == 0 { return; }
let game = game_q[0];
let bird = bird_q[0];

// ---- RestartGame ---------------------------------------------------------

let restart = false;
for _ev in events("RestartGame") {
    restart = true;
}
if restart {
    handle_restart(bird, game);
}

// ---- 第一次：挂 sprite ---------------------------------------------------

if get(bird, "Bird", "spawned") == 0 {
    if play_q.len() == 0 { return; }
    spawn_sprite(bird, play_q[0]);
    set(bird, "Bird", "spawned", 1);
    set(bird, "Bird", "pos_x", h::bird_x());
    set(bird, "Bird", "pos_y", h::bird_y_start());
    update_visual(bird);
    return;
}

// ---- 死亡：只继续把鸟落到地面（不吃输入、不再判碰撞） --------------------

if get(game, "game::Game", "state") != 0 {
    apply_gravity(bird);
    clamp_to_ground(bird, game);
    update_visual(bird);
    return;
}

// ---- 输入：跳跃 ----------------------------------------------------------

for ev in events("KeyboardInput") {
    if ev.state != "Pressed" { continue; }
    if ev.repeat { continue; }
    let key = ev.key_code;
    if key == "Space" || key == "ArrowUp" || key == "KeyW" {
        set(bird, "Bird", "vel_y", h::flap_velocity());
    }
}

// ---- 物理 + 碰撞 --------------------------------------------------------

apply_gravity(bird);

// 顶部钳：playfield 内 +Y 向上，pos_y 上限 = play_height/2 - bird_size/2
let top_limit = h::play_height() * 0.5 - h::bird_size() * 0.5;
let bottom_limit = -h::play_height() * 0.5 + h::ground_height() + h::bird_size() * 0.5;

let py = get(bird, "Bird", "pos_y");
if py > top_limit {
    py = top_limit;
    set(bird, "Bird", "vel_y", 0.0);
}
set(bird, "Bird", "pos_y", py);

// 触地 → death
if py <= bottom_limit {
    set(bird, "Bird", "pos_y", bottom_limit);
    set(bird, "Bird", "vel_y", 0.0);
    set(game, "game::Game", "state", 1);
}

// 与管道 AABB 相交 → death
if get(game, "game::Game", "state") == 0 {
    if hits_any_pipe(bird) {
        set(game, "game::Game", "state", 1);
    }
}

update_visual(bird);

// ===========================================================================
// helpers
// ===========================================================================

fn apply_gravity(bird) {
    let dt = delta();
    let v = get(bird, "Bird", "vel_y") - h::gravity() * dt;
    if v < -h::max_fall_speed() { v = -h::max_fall_speed(); }
    set(bird, "Bird", "vel_y", v);
    let py = get(bird, "Bird", "pos_y") + v * dt;
    set(bird, "Bird", "pos_y", py);
}

fn clamp_to_ground(bird, game) {
    let bottom_limit = -h::play_height() * 0.5 + h::ground_height() + h::bird_size() * 0.5;
    let py = get(bird, "Bird", "pos_y");
    if py < bottom_limit {
        set(bird, "Bird", "pos_y", bottom_limit);
        set(bird, "Bird", "vel_y", 0.0);
    }
}

// 鸟的 AABB（playfield 坐标）vs 每对管道的 top / bottom 矩形。
// 管道在 pipes module 里：每对管道是一个 Pipe 实体，gap 居中在 (pos_x, gap_y)。
fn hits_any_pipe(bird) {
    let bx = get(bird, "Bird", "pos_x");
    let by = get(bird, "Bird", "pos_y");
    let bs = h::bird_size() * 0.5;
    let bx0 = bx - bs;  let bx1 = bx + bs;
    let by0 = by - bs;  let by1 = by + bs;

    let pw = h::pipe_width();
    let gap = h::pipe_gap();
    let ph = h::play_height();

    for pipe in query("pipes::Pipe") {
        let px = get(pipe, "pipes::Pipe", "pos_x");
        let gy = get(pipe, "pipes::Pipe", "gap_y");
        let px0 = px - pw * 0.5;
        let px1 = px + pw * 0.5;
        // x 不重叠 → 跳过该对
        if bx1 < px0 || bx0 > px1 { continue; }

        // top 管道：从 playfield 顶到 gy + gap/2（playfield 坐标 +Y 向上）
        let top_low = gy + gap * 0.5;
        let top_high = ph * 0.5;
        // bottom 管道：从 -ph/2 + ground_height 到 gy - gap/2
        let bot_low = -ph * 0.5 + h::ground_height();
        let bot_high = gy - gap * 0.5;

        // 命中条件：bird Y 落在两段管道之外的"未被 gap 覆盖"区域。
        // 如果 by1 > top_low（撞上）或 by0 < bot_high（撞下）。
        if by1 > top_low || by0 < bot_high {
            return true;
        }
    }
    false
}

fn spawn_sprite(bird, play_area) {
    // 鸟身：圆角矩形（用 BorderRadius 营造视觉）。playfield 内 Absolute 定位。
    let body = spawn_entity();
    set(body, "Node", "", #{
        position_type: "Absolute",
        width: px(h::bird_size()),
        height: px(h::bird_size()),
        border: #{
            left: px(2.0), right: px(2.0),
            top: px(2.0),  bottom: px(2.0),
        },
    });
    set(body, "BackgroundColor", "", h::color_bird());
    let edge = h::color_bird_edge();
    set(body, "BorderColor", "", #{
        top: edge, right: edge, bottom: edge, left: edge,
    });
    set_parent(body, play_area);

    set(bird, "Bird", "sprite", body);
}

fn update_visual(bird) {
    let sprite = get(bird, "Bird", "sprite");
    if sprite == 0 { return; }
    // 把物理坐标 (pos_x, pos_y, +Y 向上) 翻译成 PlayArea 内的 (left, top)：
    // left = pos_x + play_width/2 - bird_size/2
    // top  = play_height/2 - pos_y - bird_size/2
    let bs = h::bird_size();
    let pw = h::play_width();
    let ph = h::play_height();
    let bird_px = get(bird, "Bird", "pos_x");
    let bird_py = get(bird, "Bird", "pos_y");
    let left = bird_px + pw * 0.5 - bs * 0.5;
    let top  = ph * 0.5 - bird_py - bs * 0.5;
    set(sprite, "Node", "left", px(left));
    set(sprite, "Node", "top",  px(top));
}

fn handle_restart(bird, game) {
    set(bird, "Bird", "pos_x", h::bird_x());
    set(bird, "Bird", "pos_y", h::bird_y_start());
    set(bird, "Bird", "vel_y", 0.0);
    set(game, "game::Game", "state", 0);
    set(game, "game::Game", "score", 0);
    set(game, "game::Game", "elapsed", 0.0);
}

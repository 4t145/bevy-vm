// player：键状态机 + WASD 沿 yaw 朝向移动 + 鼠标转 yaw/pitch + 攻击冷却倒计时。
//
// 视角：鼠标 dx → yaw（绕 Y），dy → pitch（绕 X）；俯仰 cap 在 ±85°。
// 移动：W = 朝向前，S = 后；A/D = 横向（基于水平 yaw，不受 pitch 影响）。
// forward = (sin(yaw), 0, -cos(yaw))；right = (cos(yaw), 0, sin(yaw))。

import "helpers" as h;

let players = query("Player");
if players.len() == 0 { return; }
let player = players[0];

if get(player, "Player", "hp") <= 0 { return; }

let dt = delta();

let attack_timer = get(player, "Player", "attack_timer") - dt;
if attack_timer < 0.0 { attack_timer = 0.0; }
set(player, "Player", "attack_timer", attack_timer);

// 键盘 edge：Pressed/Released 切 0/1。
for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    let down = ev.state == "Pressed";
    let v = if down { 1 } else { 0 };
    let key = ev.key_code;
    if      key == "KeyW" || key == "ArrowUp"    { set(player, "Player", "key_w", v); }
    else if key == "KeyS" || key == "ArrowDown"  { set(player, "Player", "key_s", v); }
    else if key == "KeyA" || key == "ArrowLeft"  { set(player, "Player", "key_a", v); }
    else if key == "KeyD" || key == "ArrowRight" { set(player, "Player", "key_d", v); }
}

// 鼠标左键 edge：Left Pressed → fire_pressed=1。shoot 消费后清回 0。
for ev in events("MouseButton") {
    if ev.state != "Pressed" { continue; }
    if ev.button == "Left" {
        set(player, "Player", "fire_pressed", 1);
    }
}

// 鼠标移动 → yaw / pitch。MouseMotion.delta 是 [dx, dy]（屏幕坐标，
// dy 向下为正）。dx 正值（鼠标右移）→ yaw 增大（向右转）；
// dy 正值（鼠标下移）→ pitch 减小（视线下压）。
let yaw = get(player, "Player", "yaw");
let pitch = get(player, "Player", "pitch");
let sens = get(player, "Player", "mouse_sensitivity");
for ev in events("MouseMotion") {
    yaw += ev.delta[0] * sens;
    pitch -= ev.delta[1] * sens;
}
let pi = 3.1415927;
let pitch_cap = pi * 0.47;     // ~85°
if pitch >  pitch_cap { pitch =  pitch_cap; }
if pitch < -pitch_cap { pitch = -pitch_cap; }
while yaw >  pi { yaw -= 2.0 * pi; }
while yaw < -pi { yaw += 2.0 * pi; }
set(player, "Player", "yaw", yaw);
set(player, "Player", "pitch", pitch);

// 朝向单位向量（水平面，pitch 不影响移动）。
let fx = yaw.sin();
let fz = -yaw.cos();
let rx = yaw.cos();
let rz = yaw.sin();

let in_f = (get(player, "Player", "key_w") - get(player, "Player", "key_s")).to_float();
let in_r = (get(player, "Player", "key_d") - get(player, "Player", "key_a")).to_float();

let mx = fx * in_f + rx * in_r;
let mz = fz * in_f + rz * in_r;
let mag = (mx * mx + mz * mz).sqrt();
if mag > 0.001 {
    mx /= mag;
    mz /= mag;
    let speed = get(player, "Player", "speed");
    let new_x = get(player, "Position", "x") + mx * speed * dt;
    let new_z = get(player, "Position", "z") + mz * speed * dt;
    let bound = h::arena_radius() - 1.0;
    if new_x >  bound { new_x =  bound; }
    if new_x < -bound { new_x = -bound; }
    if new_z >  bound { new_z =  bound; }
    if new_z < -bound { new_z = -bound; }
    set(player, "Position", "x", new_x);
    set(player, "Position", "z", new_z);
}

// 玩家长方体绕 Y 轴旋转——可视化朝向（即便第一人称看不到，调试有用）。
// Bevy +Y 旋转方向与脚本 yaw 约定相反，传负值。
set(player, "Rotation", "y", -yaw * 180.0 / pi);

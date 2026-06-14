// player plugin / player system：维护键盘状态 + WASD 移动 + 累计存活时间。
//
// 键盘状态机：Player 实体上挂 dynamic 字段 `key_w/a/s/d`（0/1），每帧根据
// KeyboardInput 事件更新 Pressed→1，Released→0。这样按住 W 持续生效。
//
// 移动方向约定：W = -Z（前），S = +Z（后），A = -X（左），D = +X（右）。
// 相机 up=[0,0,-1]，-Z 在屏幕上是"上"。

import "helpers" as h;

let players = query("Player");
if players.len() == 0 { return; }
let player = players[0];

let hp = get(player, "Player", "hp");
if hp <= 0 { return; }  // 死亡 → 不响应输入

// ---- 累计存活时间 + 攻击冷却 倒计 ---------------------------------------

let dt = delta();
let survived = get(player, "Player", "survived") + dt;
set(player, "Player", "survived", survived);

let attack_timer = get(player, "Player", "attack_timer") - dt;
if attack_timer < 0.0 { attack_timer = 0.0; }
set(player, "Player", "attack_timer", attack_timer);

// HP regen：按 dt 缓慢回血，cap 到 max_hp。
let regen = get(player, "Player", "regen");
if regen > 0.0 {
    let max_hp = get(player, "Player", "max_hp");
    let new_hp_f = hp.to_float() + regen * dt;
    if new_hp_f > max_hp.to_float() { new_hp_f = max_hp.to_float(); }
    set(player, "Player", "hp", new_hp_f.to_int());
}

// ---- 更新键盘状态（Pressed → 1，Released → 0） --------------------------

for ev in events("KeyboardInput") {
    if ev.repeat { continue; }  // OS repeat 不影响状态——只看 edge
    let down = ev.state == "Pressed";
    let val = if down { 1 } else { 0 };
    let key = ev.key_code;
    if      key == "KeyW" || key == "ArrowUp"    { set(player, "Player", "key_w", val); }
    else if key == "KeyS" || key == "ArrowDown"  { set(player, "Player", "key_s", val); }
    else if key == "KeyA" || key == "ArrowLeft"  { set(player, "Player", "key_a", val); }
    else if key == "KeyD" || key == "ArrowRight" { set(player, "Player", "key_d", val); }
}

// ---- 按状态计算移动方向 -------------------------------------------------

let input_x = (get(player, "Player", "key_d") - get(player, "Player", "key_a")).to_float();
let input_z = (get(player, "Player", "key_s") - get(player, "Player", "key_w")).to_float();

// 归一化
let mag = (input_x * input_x + input_z * input_z).sqrt();
if mag > 0.001 {
    input_x /= mag;
    input_z /= mag;
}

let speed = get(player, "Player", "speed");
let pos_x = get(player, "Position", "x") + input_x * speed * dt;
let pos_z = get(player, "Position", "z") + input_z * speed * dt;

// 限制在 arena 内
let bound = h::arena_radius() - 1.0;
if pos_x >  bound { pos_x =  bound; }
if pos_x < -bound { pos_x = -bound; }
if pos_z >  bound { pos_z =  bound; }
if pos_z < -bound { pos_z = -bound; }

set(player, "Position", "x", pos_x);
set(player, "Position", "z", pos_z);

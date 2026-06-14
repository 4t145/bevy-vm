// shoot：fire_pressed=1 且冷却归零 → spawn 子弹 + 重置冷却。
//
// 子弹起点：相机位置 + forward * 0.6（避开自身长方体）；
// 速度：forward * bullet_speed。
// fire_pressed 是 edge-trigger——一次按键发一发；shoot 处理后清回 0。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
if get(player, "player::Player", "hp") <= 0 { return; }

let pressed = get(player, "player::Player", "fire_pressed");
if pressed != 1 { return; }
set(player, "player::Player", "fire_pressed", 0);

let timer = get(player, "player::Player", "attack_timer");
if timer > 0.0 { return; }
set(player, "player::Player", "attack_timer", h::fire_cooldown());

let yaw = get(player, "player::Player", "yaw");
let pitch = get(player, "player::Player", "pitch");
let cp = pitch.cos();
let fx = yaw.sin() * cp;
let fy = pitch.sin();
let fz = -yaw.cos() * cp;

// 子弹起点：玩家眼高 + forward 0.6 避开自身长方体。
let ox = get(player, "Position", "x") + fx * 0.6;
let oy = get(player, "Position", "y") + h::eye_offset_y() + fy * 0.6;
let oz = get(player, "Position", "z") + fz * 0.6;

let speed = h::bullet_speed();

let b = spawn_entity();
set_translation(b, [ox, oy, oz]);
attach_mesh(b, #{ Sphere: h::bullet_radius() });
attach_pbr(b, #{
    base_color: h::color_bullet(),
    metallic: 0.5,
    roughness: 0.2,
    emissive: srgba(2.0, 1.5, 0.0, 1.0),
});
set(b, "bullet::Bullet", "vx", fx * speed);
set(b, "bullet::Bullet", "vy", fy * speed);
set(b, "bullet::Bullet", "vz", fz * speed);
set(b, "bullet::Bullet", "damage", h::bullet_damage());
set(b, "bullet::Bullet", "lifetime", h::bullet_lifetime());

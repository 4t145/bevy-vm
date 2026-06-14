// shooting plugin / shooting system：自动锁定最近敌人发射投射物。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];

if get(player, "player::Player", "hp") <= 0 { return; }
if get(player, "player::Player", "attack_timer") > 0.0 { return; }

let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");

// 找最近敌人——线性扫描。敌人数 60 上限内 ~600 ops，可以接受。
let enemies = query("enemy::Enemy");
if enemies.len() == 0 { return; }

let nearest = enemies[0];
let nearest_dist_sq = 1.0e9;
for enemy in enemies {
    let ex = get(enemy, "Position", "x");
    let ez = get(enemy, "Position", "z");
    let dx = ex - px;
    let dz = ez - pz;
    let dist_sq = dx * dx + dz * dz;
    if dist_sq < nearest_dist_sq {
        nearest_dist_sq = dist_sq;
        nearest = enemy;
    }
}

// 计算朝向单位向量。
let tx = get(nearest, "Position", "x") - px;
let tz = get(nearest, "Position", "z") - pz;
let dist = (tx * tx + tz * tz).sqrt();
if dist < 0.001 { return; }
let dir_x = tx / dist;
let dir_z = tz / dist;

let speed = get(player, "player::Player", "bullet_speed");
let damage = get(player, "player::Player", "bullet_damage");
let lifetime = get(player, "player::Player", "bullet_lifetime");

// Spawn projectile。
let bullet = spawn_entity();
set_translation(bullet, [px, h::ground_y(), pz]);
attach_mesh(bullet, #{ Sphere: 0.15 });
attach_pbr(bullet, #{
    base_color: h::color_projectile(),
    metallic: 0.6,
    roughness: 0.25,
    emissive: srgba(2.0, 1.5, 0.0, 1.0),
});
set(bullet, "projectile::Projectile", "vx", dir_x * speed);
set(bullet, "projectile::Projectile", "vz", dir_z * speed);
set(bullet, "projectile::Projectile", "damage", damage);
set(bullet, "projectile::Projectile", "lifetime", lifetime);

// 重置攻击冷却。
let cooldown = get(player, "player::Player", "attack_cooldown");
set(player, "player::Player", "attack_timer", cooldown);

// combat plugin / combat system：双重碰撞检测。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];

if get(player, "player::Player", "hp") <= 0 { return; }

let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");

let projectiles = query("projectile::Projectile");
let enemies = query("enemy::Enemy");

// ---- 投射物 ↔ 敌人 -------------------------------------------------------
//
// O(n²)：proj × enemy。命中标志记在数组里——同一帧 despawn 不在循环中
// 立即做（避免 query 返回的 entity id 失效）。

let proj_threshold_sq = h::collide_proj_enemy() * h::collide_proj_enemy();
let proj_to_despawn = [];
let enemy_kills = [];   // [(entity, x, z, xp_reward)]

for proj in projectiles {
    let dmg = get(proj, "projectile::Projectile", "damage");
    let pjx = get(proj, "Position", "x");
    let pjz = get(proj, "Position", "z");
    let hit = false;
    for enemy in enemies {
        // 已被本帧别的弹打死的敌人 hp 已经是 0+ —— 跳过避免重复 emit。
        let ehp = get(enemy, "enemy::Enemy", "hp");
        if ehp <= 0 { continue; }
        let ex = get(enemy, "Position", "x");
        let ez = get(enemy, "Position", "z");
        let dx = ex - pjx;
        let dz = ez - pjz;
        if dx * dx + dz * dz <= proj_threshold_sq {
            // 命中。
            let new_hp = ehp - dmg;
            set(enemy, "enemy::Enemy", "hp", new_hp);
            if new_hp <= 0 {
                let xp = get(enemy, "enemy::Enemy", "xp_reward");
                enemy_kills.push([enemy, ex, ez, xp]);
            }
            hit = true;
            break;
        }
    }
    if hit { proj_to_despawn.push(proj); }
}

for proj in proj_to_despawn { despawn(proj); }

// 击杀计数 + emit
for kill in enemy_kills {
    let entity = kill[0];
    let x = kill[1];
    let z = kill[2];
    let xp = kill[3];
    emit("EnemyDied", #{ entity: entity, x: x, z: z, xp_reward: xp });
    despawn(entity);
    let kc = get(player, "player::Player", "kills") + 1;
    set(player, "player::Player", "kills", kc);
}

// ---- 玩家 ↔ 敌人 ---------------------------------------------------------
//
// 玩家碰到敌人按帧扣血——没有"无敌帧"机制，但这只是 MVP 可接受。
// 玩家 hp ≤ 0 → emit PlayerDied。
let pe_threshold_sq = h::collide_player_enemy() * h::collide_player_enemy();
let dt = delta();
let alive_enemies = query("enemy::Enemy"); // 上面 despawn 后重 query
for enemy in alive_enemies {
    let ex = get(enemy, "Position", "x");
    let ez = get(enemy, "Position", "z");
    let dx = ex - px;
    let dz = ez - pz;
    if dx * dx + dz * dz <= pe_threshold_sq {
        let dmg_per_sec = get(enemy, "enemy::Enemy", "damage");
        // 按 dt 比例扣血——避免帧率影响伤害。
        let hp = get(player, "player::Player", "hp") - (dmg_per_sec.to_float() * dt);
        if hp < 0.0 { hp = 0.0; }
        set(player, "player::Player", "hp", hp.to_int());
        if hp <= 0.0 {
            emit("PlayerDied", #{});
            return;
        }
    }
}

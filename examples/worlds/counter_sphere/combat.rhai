// combat：O(n²) 子弹 × 敌人。命中阈值见 helpers::collide_bullet_enemy。
// 命中后子弹与敌人 hp 在收尾循环里处理——避免 query 中 despawn 让 id 失效。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];

let bullets = query("bullet::Bullet");
let enemies = query("enemy::Enemy");
if bullets.len() == 0 || enemies.len() == 0 { return; }

let thr = h::collide_bullet_enemy();
let thr_sq = thr * thr;
let bullets_to_despawn = [];
let kills = [];   // 死亡的敌人

for b in bullets {
    let bx = get(b, "Position", "x");
    let by = get(b, "Position", "y");
    let bz = get(b, "Position", "z");
    let dmg = get(b, "bullet::Bullet", "damage");
    let hit = false;
    for e in enemies {
        let ehp = get(e, "enemy::Enemy", "hp");
        if ehp <= 0 { continue; }
        let ex = get(e, "Position", "x");
        let ey = get(e, "Position", "y");
        let ez = get(e, "Position", "z");
        let dx = ex - bx;
        let dy = ey - by;
        let dz = ez - bz;
        if dx*dx + dy*dy + dz*dz <= thr_sq {
            let new_hp = ehp - dmg;
            set(e, "enemy::Enemy", "hp", new_hp);
            if new_hp <= 0 { kills.push(e); }
            hit = true;
            break;
        }
    }
    if hit { bullets_to_despawn.push(b); }
}

for b in bullets_to_despawn { despawn(b); }
for e in kills {
    despawn(e);
    emit("EnemyDied", #{});
    let kc = get(player, "player::Player", "kills") + 1;
    set(player, "player::Player", "kills", kc);
}

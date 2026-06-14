// enemy_move：每帧让敌人朝玩家走，距离 < 2.0 时停步。
// 不做避障——障碍物只是视觉装饰；敌人会穿过 obstacle，先简化。

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
if get(player, "player::Player", "hp") <= 0 { return; }

let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");
let dt = delta();

const STOP_DIST = 2.0;
let stop_sq = STOP_DIST * STOP_DIST;

for e in query("Enemy") {
    let hp = get(e, "Enemy", "hp");
    if hp <= 0 { continue; }
    let ex = get(e, "Position", "x");
    let ez = get(e, "Position", "z");
    let dx = px - ex;
    let dz = pz - ez;
    let dist_sq = dx * dx + dz * dz;
    if dist_sq < stop_sq { continue; }
    let dist = dist_sq.sqrt();
    let speed = get(e, "Enemy", "speed");
    let step = speed * dt;
    set(e, "Position", "x", ex + dx / dist * step);
    set(e, "Position", "z", ez + dz / dist * step);
}

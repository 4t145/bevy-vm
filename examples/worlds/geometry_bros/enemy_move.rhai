// enemy plugin / enemy_move system：每帧让所有敌人朝玩家移动。
//
// after: ["player"] 保证读到玩家本帧的最新位置——朝着"看到的"位置追。

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");

let dt = delta();

for enemy in query("Enemy") {
    let ex = get(enemy, "Position", "x");
    let ez = get(enemy, "Position", "z");
    let dx = px - ex;
    let dz = pz - ez;
    let dist = (dx * dx + dz * dz).sqrt();
    if dist < 0.001 { continue; }
    let speed = get(enemy, "Enemy", "speed");
    let step = speed * dt;
    let new_x = ex + dx / dist * step;
    let new_z = ez + dz / dist * step;
    set(enemy, "Position", "x", new_x);
    set(enemy, "Position", "z", new_z);
}

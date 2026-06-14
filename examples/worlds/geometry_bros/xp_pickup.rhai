// xp plugin / xp_pickup system：玩家走近经验球时吸附 + 拾取。
//
// 拾取范围内 → despawn orb + emit XpGained。
// （未做"远距吸附"动画——参考土豆兄弟有"在某半径内被拉过来"，先省。）

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");

let radius_sq = h::pickup_radius() * h::pickup_radius();
let to_despawn = [];
let total_xp = 0;

for orb in query("ExpOrb") {
    let ox = get(orb, "Position", "x");
    let oz = get(orb, "Position", "z");
    let dx = ox - px;
    let dz = oz - pz;
    if dx * dx + dz * dz <= radius_sq {
        total_xp += get(orb, "ExpOrb", "amount");
        to_despawn.push(orb);
    }
}

for orb in to_despawn { despawn(orb); }

if total_xp > 0 {
    emit("XpGained", #{ amount: total_xp });
}

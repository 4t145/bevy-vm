// wave_clear：监听 WaveCompleted，清场 + 无条件满血。
//
// 敌人 / 弹幕直接 despawn；未拾取的经验球折算价值直接累加给玩家
// （cap 到下一阈值-1 防连级——但 level 已无 menu，所以这里其实可以放开，
// 但保留 cap 让 XP bar 视觉上更稳）。
// 玩家 hp 直接拉到 max_hp——回血是波次的固定奖励，不占菜单卡位。

let triggered = false;
for _ev in events("WaveCompleted") {
    triggered = true;
}
if !triggered { return; }

for e in query("enemy::Enemy") { despawn(e); }
for p in query("projectile::Projectile") { despawn(p); }

let salvaged = 0;
for orb in query("xp::ExpOrb") {
    salvaged += get(orb, "xp::ExpOrb", "amount");
    despawn(orb);
}

if salvaged > 0 {
    let pl_q = query("level::PlayerLevel");
    if pl_q.len() > 0 {
        let pl = pl_q[0];
        let cap = get(pl, "level::PlayerLevel", "next_threshold") - 1;
        let new_xp = get(pl, "level::PlayerLevel", "current_xp") + salvaged;
        if new_xp > cap { new_xp = cap; }
        set(pl, "level::PlayerLevel", "current_xp", new_xp);
    }
}

let players = query("player::Player");
if players.len() > 0 {
    let player = players[0];
    let max_hp = get(player, "player::Player", "max_hp");
    set(player, "player::Player", "hp", max_hp);
}

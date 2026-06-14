// level plugin：消费 XpGained，累加 current_xp；越过阈值给玩家属性微涨。
//
// 升级**不暂停**——只是均匀加强属性条；显著加强留给波次菜单（wave plugin）。
// 仍然 emit LevelUp 事件——HUD / 音效之类只想"知道升级了"的消费方依然可用。

let pl_q = query("PlayerLevel");
if pl_q.len() == 0 { return; }
let pl = pl_q[0];

let gained = 0;
for ev in events("xp::XpGained") {
    gained += ev.amount;
}

if gained == 0 { return; }

let current = get(pl, "PlayerLevel", "current_xp") + gained;
let mut_level = get(pl, "PlayerLevel", "level");
let mut_threshold = get(pl, "PlayerLevel", "next_threshold");

let players = query("player::Player");
let player = if players.len() > 0 { players[0] } else { () };

// 一次 XpGained 可能跨多级——循环消化。
while current >= mut_threshold {
    current -= mut_threshold;
    mut_level += 1;
    // 阈值 1.6 倍递增。10 → 16 → 25 → 41 → 65 → ...
    mut_threshold = (mut_threshold.to_float() * 1.6).to_int();
    emit("LevelUp", #{ level: mut_level });

    // 每升一级，玩家属性条均匀微涨。
    if player != () {
        let max_hp = get(player, "player::Player", "max_hp") + 5;
        set(player, "player::Player", "max_hp", max_hp);
        let hp = get(player, "player::Player", "hp") + 5;   // 顺带回点血
        if hp > max_hp { hp = max_hp; }
        set(player, "player::Player", "hp", hp);

        let speed = get(player, "player::Player", "speed") + 0.1;
        set(player, "player::Player", "speed", speed);

        let dmg = get(player, "player::Player", "bullet_damage") + 1;
        set(player, "player::Player", "bullet_damage", dmg);

        let cd = get(player, "player::Player", "attack_cooldown") * 0.98;
        set(player, "player::Player", "attack_cooldown", cd);
    }
}

set(pl, "PlayerLevel", "current_xp", current);
set(pl, "PlayerLevel", "level", mut_level);
set(pl, "PlayerLevel", "next_threshold", mut_threshold);

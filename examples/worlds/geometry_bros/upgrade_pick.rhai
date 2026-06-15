// upgrade_pick：扫本帧 picking click 缓冲，若点中 UpgradeCard 应用属性 + 关闭面板。

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];

let picked = ();
for ev in events("PickClick") {
    if ev.button != "Primary" { continue; }
    let kind = get(ev.entity, "UpgradeCard", "kind");
    if kind != "" {
        picked = kind;
        break;
    }
}

if picked == () { return; }

if picked == "max_hp" {
    let cur = get(player, "player::Player", "max_hp");
    set(player, "player::Player", "max_hp", cur + 20);
    let hp = get(player, "player::Player", "hp") + 20;
    set(player, "player::Player", "hp", hp);
} else if picked == "speed" {
    let cur = get(player, "player::Player", "speed");
    set(player, "player::Player", "speed", cur * 1.2);
} else if picked == "fire_rate" {
    let cur = get(player, "player::Player", "attack_cooldown");
    set(player, "player::Player", "attack_cooldown", cur * 0.8);
} else if picked == "damage" {
    let cur = get(player, "player::Player", "bullet_damage");
    set(player, "player::Player", "bullet_damage", (cur.to_float() * 1.3).to_int());
} else if picked == "regen" {
    let cur = get(player, "player::Player", "regen");
    set(player, "player::Player", "regen", cur + 1.0);
}

// 清面板：despawn UpgradeRoot 整棵子树（linked_spawn 自动级联）。
for root in query("UpgradeRoot") {
    despawn(root);
}

emit("pause::UpgradePicked", #{ kind: picked });

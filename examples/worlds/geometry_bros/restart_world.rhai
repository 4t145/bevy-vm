// restart_world：监听 RestartGame，重置玩家 / 等级 / 清场。
//
// 不 despawn 单例 entity（Player / EnemySpawner / PlayerLevel）——只 reset
// 字段。动态生成的（Enemy / Projectile / ExpOrb / DeathScreen / UpgradeRoot）
// 全部 despawn。

let triggered = false;
for _ev in events("RestartGame") {
    triggered = true;
}
if !triggered { return; }

// 清场。
for e in query("enemy::Enemy") { despawn(e); }
for e in query("projectile::Projectile") { despawn(e); }
for e in query("xp::ExpOrb") { despawn(e); }
for e in query("DeathScreen") { despawn(e); }
for e in query("upgrade::UpgradeRoot") { despawn(e); }

// 重置波次。
let waves = query("wave::WaveState");
if waves.len() > 0 {
    let w = waves[0];
    set(w, "wave::WaveState", "number", 1);
    set(w, "wave::WaveState", "remaining", get(w, "wave::WaveState", "duration"));
    set(w, "wave::WaveState", "in_intermission", 0);
}

// 重置玩家。
let players = query("player::Player");
if players.len() > 0 {
    let player = players[0];
    set(player, "player::Player", "hp", 100);
    set(player, "player::Player", "max_hp", 100);
    set(player, "player::Player", "speed", 5.0);
    set(player, "player::Player", "attack_cooldown", 0.5);
    set(player, "player::Player", "attack_timer", 0.0);
    set(player, "player::Player", "bullet_damage", 10);
    set(player, "player::Player", "bullet_speed", 8.0);
    set(player, "player::Player", "bullet_lifetime", 2.0);
    set(player, "player::Player", "regen", 0.0);
    set(player, "player::Player", "kills", 0);
    set(player, "player::Player", "survived", 0.0);
    set(player, "player::Player", "key_w", 0);
    set(player, "player::Player", "key_a", 0);
    set(player, "player::Player", "key_s", 0);
    set(player, "player::Player", "key_d", 0);
    set(player, "Position", "x", 0.0);
    set(player, "Position", "z", 0.0);
}

// 重置等级。
let pl_q = query("level::PlayerLevel");
if pl_q.len() > 0 {
    let pl = pl_q[0];
    set(pl, "level::PlayerLevel", "level", 1);
    set(pl, "level::PlayerLevel", "current_xp", 0);
    set(pl, "level::PlayerLevel", "next_threshold", 10);
}

// 重置 spawner。
let spawner_q = query("enemy::EnemySpawner");
if spawner_q.len() > 0 {
    let spawner = spawner_q[0];
    set(spawner, "enemy::EnemySpawner", "next_spawn_time", time() + 0.5);
    set(spawner, "enemy::EnemySpawner", "spawn_interval", 1.0);
}

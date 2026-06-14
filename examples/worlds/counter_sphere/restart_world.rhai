// restart_world：监听 RestartGame，重置玩家 + despawn 所有动态实体 +
// 让 spawn flag 归零让 enemy / terrain 重新铺设（新种子应用）。

let triggered = false;
for _ev in events("RestartGame") {
    triggered = true;
}
if !triggered { return; }

// 清场（dynamic 实体——保留单例 Player + Camera + Plane + state markers）。
for e in query("enemy::Enemy") { despawn(e); }
for e in query("bullet::Bullet") { despawn(e); }
for e in query("terrain::Obstacle") { despawn(e); }
for e in query("WinScreen") { despawn(e); }

// 重置玩家。
let players = query("player::Player");
if players.len() > 0 {
    let p = players[0];
    set(p, "player::Player", "hp", 100);
    set(p, "player::Player", "max_hp", 100);
    set(p, "player::Player", "kills", 0);
    set(p, "player::Player", "yaw", 0.0);
    set(p, "player::Player", "pitch", 0.0);
    set(p, "player::Player", "attack_timer", 0.0);
    set(p, "player::Player", "fire_pressed", 0);
    set(p, "player::Player", "key_w", 0);
    set(p, "player::Player", "key_a", 0);
    set(p, "player::Player", "key_s", 0);
    set(p, "player::Player", "key_d", 0);
    set(p, "Position", "x", 0.0);
    set(p, "Position", "y", 1.0);
    set(p, "Position", "z", 0.0);
    set(p, "Rotation", "y", 0.0);
}

// 让 spawn flag 归零，下一帧 terrain / enemy_spawn 重新生成。
let ts = query("terrain::TerrainState");
if ts.len() > 0 { set(ts[0], "terrain::TerrainState", "generated", 0); }
let es = query("enemy::EnemySpawnState");
if es.len() > 0 { set(es[0], "enemy::EnemySpawnState", "spawned", 0); }
// 重置 cursor.grabbed 让下一帧自动 emit CursorGrab 重新进入 FPS。
let cs = query("cursor::CursorState");
if cs.len() > 0 { set(cs[0], "cursor::CursorState", "grabbed", 0); }

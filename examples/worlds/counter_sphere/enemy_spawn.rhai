// enemy_spawn：第一帧 spawn 5 个球体敌人。位置随机散布在 arena 远端。

import "helpers" as h;

let states = query("EnemySpawnState");
if states.len() == 0 { return; }
let s = states[0];
if get(s, "EnemySpawnState", "spawned") == 1 { return; }
set(s, "EnemySpawnState", "spawned", 1);

const COUNT = 5;
const MIN_DIST_FROM_SPAWN = 10.0;

for _i in 0..COUNT {
    let r = random_range(MIN_DIST_FROM_SPAWN, h::arena_radius() - 2.0);
    let angle = random_range(0.0, 6.2831855);
    let x = angle.cos() * r;
    let z = angle.sin() * r;
    let e = spawn_entity();
    set(e, "Position", "x", x);
    set(e, "Position", "y", h::enemy_radius());
    set(e, "Position", "z", z);
    set(e, "Mesh3dRender", "", #{
        mesh: #{ "Sphere": #{ radius: h::enemy_radius() } },
        material: #{ "Pbr": #{
            base_color: h::color_enemy(),
            metallic: 0.2,
            roughness: 0.4,
        } },
    });
    set(e, "Enemy", "hp", 50);
    set(e, "Enemy", "speed", random_range(1.0, 2.0));
}

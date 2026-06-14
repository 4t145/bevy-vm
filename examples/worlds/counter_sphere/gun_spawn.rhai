// gun_spawn：第一帧 spawn 一根细长方体当烧火棍。位置由 gun_follow 维护。

import "helpers" as h;

let states = query("GunSpawnState");
if states.len() == 0 { return; }
let s = states[0];
if get(s, "GunSpawnState", "spawned") == 1 { return; }
set(s, "GunSpawnState", "spawned", 1);

let g = spawn_entity();
set(g, "Position", "x", 0.0);
set(g, "Position", "y", 0.5);
set(g, "Position", "z", 0.0);
set(g, "Mesh3dRender", "", #{
    mesh: #{ "Cube": #{ size: [0.08, 0.08, 1.2] } },   // 细长棍
    material: #{ "Pbr": #{
        base_color: h::color_gun(),
        metallic: 0.0,
        roughness: 0.85,
    } },
});
set(g, "Gun", "", #{});

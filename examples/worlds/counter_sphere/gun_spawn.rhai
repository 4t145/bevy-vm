// gun_spawn：第一帧 spawn 一根细长方体当烧火棍。位置由 gun_follow 维护。

import "helpers" as h;

let states = query("GunSpawnState");
if states.len() == 0 { return; }
let s = states[0];
if get(s, "GunSpawnState", "spawned") == 1 { return; }
set(s, "GunSpawnState", "spawned", 1);

let g = spawn_entity();
set_translation(g, [0.0, 0.5, 0.0]);
attach_mesh(g, #{ Cuboid: [0.08, 0.08, 1.2] });   // 细长棍
attach_pbr(g, #{
    base_color: h::color_gun(),
    metallic: 0.0,
    roughness: 0.85,
});
set(g, "Gun", "", #{});

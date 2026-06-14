// terrain：第一帧生成 ~12 个随机长方体障碍。
//
// 每个 obstacle 是 Cuboid，xz 平面随机摆放，y 半高，避开 (0,0,0) 玩家
// 出生点 4 单位。世界种子由 world.ron 的 seed 字段决定——random_range
// 走 VM 的 VmRng，确定性回放。

import "helpers" as h;

let states = query("TerrainState");
if states.len() == 0 { return; }
let state = states[0];
if get(state, "TerrainState", "generated") == 1 { return; }
set(state, "TerrainState", "generated", 1);

const COUNT = 12;
const MIN_FROM_SPAWN = 4.0;

for _i in 0..COUNT {
    let attempts = 0;
    let placed = false;
    while attempts < 6 && !placed {
        let x = random_range(-h::arena_radius() + 2.0, h::arena_radius() - 2.0);
        let z = random_range(-h::arena_radius() + 2.0, h::arena_radius() - 2.0);
        if (x * x + z * z).sqrt() >= MIN_FROM_SPAWN {
            let w = random_range(0.8, 2.4);
            let d = random_range(0.8, 2.4);
            let height = random_range(1.0, 3.0);
            let e = spawn_entity();
            set(e, "Position", "x", x);
            set(e, "Position", "y", height * 0.5);
            set(e, "Position", "z", z);
            set(e, "Mesh3dRender", "", #{
                mesh: #{ "Cube": #{ size: [w, height, d] } },
                material: #{ "Pbr": #{
                    base_color: h::color_obstacle(),
                    metallic: 0.05,
                    roughness: 0.85,
                } },
            });
            set(e, "Obstacle", "", #{});
            placed = true;
        }
        attempts += 1;
    }
}

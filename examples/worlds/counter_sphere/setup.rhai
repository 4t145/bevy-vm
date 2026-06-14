// player::setup —— 第一帧给 Player / Ground attach 视觉，相机在 camera
// module 里 attach（每帧重 attach 因为相机要跟玩家头部）。

import "helpers" as h;

let players = query("Player");
if players.len() == 0 { return; }
let player = players[0];

if get(player, "Player", "setup_done") == 1 { return; }
set(player, "Player", "setup_done", 1);

attach_mesh(player, #{ Cuboid: [0.6, 1.8, 0.6] });
attach_pbr(player, #{
    base_color: srgba(0.30, 0.70, 1.00, 1.0),
    metallic: 0.2,
    roughness: 0.45,
});
set_translation(player, [0.0, 1.0, 0.0]);

for ground in query("Ground") {
    attach_mesh(ground, #{ Plane: [60.0, 60.0] });
    attach_pbr(ground, #{
        base_color: srgba(0.18, 0.20, 0.16, 1.0),
        roughness: 0.95,
    });
    set_translation(ground, [0.0, 0.0, 0.0]);
}

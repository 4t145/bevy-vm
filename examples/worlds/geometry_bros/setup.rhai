// player::setup —— 第一帧给 Player 挂视觉 + 给地面 attach mesh + 给相机
// attach orthographic 3D。

import "helpers" as h;

let players = query("Player");
if players.len() == 0 { return; }
let player = players[0];

if get(player, "Player", "setup_done") == 1 { return; }
set(player, "Player", "setup_done", 1);

attach_mesh(player, #{ Sphere: 0.5 });
attach_pbr(player, #{
    base_color: srgba(0.35, 0.65, 1.0, 1.0),
    metallic: 0.3,
    roughness: 0.35,
});
set_translation(player, [0.0, 0.5, 0.0]);

// 地面（Ground 标记单例）。
for ground in query("Ground") {
    attach_mesh(ground, #{ Plane: [40.0, 40.0] });
    attach_pbr(ground, #{
        base_color: srgba(0.18, 0.18, 0.20, 1.0),
        roughness: 0.95,
    });
    set_translation(ground, [0.0, 0.0, 0.0]);
}

// 主相机：3D 正交「轻倾斜俯视」。
for cam in query("MainCamera") {
    attach_camera_3d(cam, #{
        projection: "orthographic",
        viewport_height: 22.0,
        scale: 1.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 14.0, 8.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        clear_color: srgba(0.06, 0.07, 0.10, 1.0),
    });
}

// camera：第一人称相机——位置贴在玩家头部 + target 沿 (yaw, pitch) 视线。
//
// 第一帧 attach Camera3d；每帧 set_translation + look_at 更新 transform。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
let yaw = get(player, "player::Player", "yaw");
let pitch = get(player, "player::Player", "pitch");

let px = get(player, "Position", "x");
let py = get(player, "Position", "y") + h::eye_offset_y();
let pz = get(player, "Position", "z");

let cp = pitch.cos();
let dx = yaw.sin() * cp;
let dy = pitch.sin();
let dz = -yaw.cos() * cp;

for cam in query("MainCamera") {
    if get(cam, "MainCamera", "attached") == 0 {
        attach_camera_3d(cam, #{
            projection: "perspective",
            fov_degrees: 75.0,
            near: 0.1,
            far: 200.0,
            up: [0.0, 1.0, 0.0],
            clear_color: srgba(0.45, 0.55, 0.70, 1.0),
        });
        set(cam, "MainCamera", "attached", 1);
    }
    set_translation(cam, [px, py, pz]);
    look_at(cam, [px + dx, py + dy, pz + dz], [0.0, 1.0, 0.0]);
}

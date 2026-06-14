// camera：第一人称——位置贴在玩家头部 + target 沿 (yaw, pitch) 视线 1 单位。
//
// 视线方向：
//   dir.x = sin(yaw) * cos(pitch)
//   dir.y = sin(pitch)
//   dir.z = -cos(yaw) * cos(pitch)
// pitch=0 时退化成水平 (sin(yaw), 0, -cos(yaw))。

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

for cam in query("Camera3d") {
    set(cam, "Position", "x", px);
    set(cam, "Position", "y", py);
    set(cam, "Position", "z", pz);
    set(cam, "Camera3d", "target", [px + dx, py + dy, pz + dz]);
}

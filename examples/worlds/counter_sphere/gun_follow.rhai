// gun_follow：把枪贴在相机右下前方，朝向跟相机视线一致。
//
// Rotation 用 YXZ 顺序（render 端配置）：先绕 Y(yaw) 再绕变换后的 X(pitch)。
// 这与相机的视线构造一致——枪管始终指向相机前方。
//
// 位置基底：
//   forward = (sin(yaw)cos(pitch), sin(pitch), -cos(yaw)cos(pitch))
//   right   = (cos(yaw), 0, sin(yaw))                ← pitch 不影响 right
//   down    = right × forward                         ← 仰视时 down 也跟着抬

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
let yaw = get(player, "player::Player", "yaw");
let pitch = get(player, "player::Player", "pitch");

let cp = pitch.cos();
let sp = pitch.sin();

let fx = yaw.sin() * cp;
let fy = sp;
let fz = -yaw.cos() * cp;

let rx = yaw.cos();
let ry = 0.0;
let rz = yaw.sin();

// down = right × forward（左手系下 = -Y 大致方向，仰视时随之抬）。
let dx = ry * fz - rz * fy;
let dy = rz * fx - rx * fz;
let dz = rx * fy - ry * fx;
// 翻号让 down 大致指向 -Y。
dx = -dx; dy = -dy; dz = -dz;

let px = get(player, "Position", "x");
let py = get(player, "Position", "y") + h::eye_offset_y();
let pz = get(player, "Position", "z");

const RIGHT = 0.30;
const DOWN = 0.18;
const FORWARD = 0.55;

let pi = 3.1415927;
let yaw_deg = yaw * 180.0 / pi;
let pitch_deg = pitch * 180.0 / pi;

for g in query("Gun") {
    set(g, "Position", "x", px + rx * RIGHT + dx * DOWN + fx * FORWARD);
    set(g, "Position", "y", py + ry * RIGHT + dy * DOWN + fy * FORWARD);
    set(g, "Position", "z", pz + rz * RIGHT + dz * DOWN + fz * FORWARD);
    // Bevy +Y 右手旋转 +θ 让 -Z → -X，但我们脚本里 yaw 的约定是
    // forward=(sin(yaw),0,-cos(yaw))，yaw=90° 朝 +X——刚好相反。
    // 所以传 -yaw_deg 让 Bevy 那边的转向与我们的 yaw 一致。
    set(g, "Rotation", "x", pitch_deg);
    set(g, "Rotation", "y", -yaw_deg);
}

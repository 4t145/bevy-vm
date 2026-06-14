// camera plugin / camera system：相机以斜俯视角度跟随玩家。
//
// 偏移量与 world.ron 中初始位置一致：相机在玩家"斜后上方"——y +14、z +8。
// target 设为玩家脚下，looking_at 自动算出正确朝向。
// 直接 snap（不平滑）避免增加视觉延迟感。

const CAM_OFFSET_Y = 14.0;
const CAM_OFFSET_Z = 8.0;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");

for cam in query("Camera3d") {
    set(cam, "Position", "x", px);
    set(cam, "Position", "y", CAM_OFFSET_Y);
    set(cam, "Position", "z", pz + CAM_OFFSET_Z);
    set(cam, "Camera3d", "target", [px, 0.0, pz]);
}

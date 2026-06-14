// camera module / camera system：相机以斜俯视角度跟随玩家。
//
// 偏移量与 setup 中初始位置一致：相机在玩家"斜后上方"——y +14、z +8。
// target 设为玩家脚下；look_at 内部用 Transform::looking_at 算朝向。

const CAM_OFFSET_Y = 14.0;
const CAM_OFFSET_Z = 8.0;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");

for cam in query("player::MainCamera") {
    set_translation(cam, [px, CAM_OFFSET_Y, pz + CAM_OFFSET_Z]);
    look_at(cam, [px, 0.0, pz], [0.0, 1.0, 0.0]);
}

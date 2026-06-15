// board::setup —— 第一帧 spawn 相机 + 14x21 网格 tile。
//
// tile 用 cuboid（1x0.2x1）+ PBR 浅灰色，高度在 ±0.1 抖动模拟原版起伏。
// 相机放在 (-7, 14, 10) 看向棋盘中心，与原版坐标一致。

import "consts" as c;

let games = query("Game");
if games.len() == 0 { return; }
let game = games[0];

if get(game, "Game", "board_built") == 1 { return; }
set(game, "Game", "board_built", 1);

// 主相机——MainCam marker 让 camera.rhai 知道哪个 entity 是主相机。
let cam = spawn_entity();
set(cam, "MainCam", "", #{});
attach_camera_3d(cam, #{
    projection: "perspective",
    // 与原版 `Camera3d::default()` 的 PerspectiveProjection 默认 fov = π/4 一致。
    fov_degrees: 45.0,
    near: 0.1,
    far: 1000.0,
    // 原版 setup_cameras 写死 (-7, 14, 10)；这里用 BOARD_SIZE 公式表达
    // 一致结果：(-board_w/2, 2*board_h/3, board_h/2 - 0.5) = (-7, 14, 10)。
    position: [-(c::board_w() / 2.0), 2.0 * c::board_h() / 3.0, c::board_h() / 2.0 - 0.5],
    target: [c::board_w() / 2.0, 0.0, c::board_h() / 2.0 - 0.5],
    up: [0.0, 1.0, 0.0],
    clear_color: srgb(0.05, 0.05, 0.08),
});

// 全局点光源：固定在棋盘上空，给场景打基础亮度（与原版 setup 里那盏
// PointLight 对齐）。
let key_light = spawn_entity();
attach_point_light(key_light, #{
    intensity: 2_000_000.0,
    range: 30.0,
    shadows: true,
});
set_translation(key_light, [4.0, 10.0, 4.0]);

// 棋盘：14 列（i / x） × 21 行（j / z）。tile 用 GLB Scene。
let bw = c::board_w_int();
let bh = c::board_h_int();
for j in 0..bh {
    for i in 0..bw {
        let height = random_range(-0.1, 0.1);
        let cell = spawn_entity();
        set(cell, "Cell", "i", i);
        set(cell, "Cell", "j", j);
        set(cell, "Cell", "height", height);

        attach_scene(cell, "models/AlienCake/tile.glb#Scene0");
        set_translation(cell, [i.to_float(), height - 0.2, j.to_float()]);
    }
}

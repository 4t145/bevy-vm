// drag::setup —— 第一帧 attach 立方体 + 相机。

let states = query("DragSetup");
if states.len() == 0 { return; }
let state = states[0];
if get(state, "DragSetup", "done") == 1 { return; }
set(state, "DragSetup", "done", 1);

for cube in query("DragState") {
    attach_mesh(cube, #{ Cuboid: [1.0, 1.0, 1.0] });
    attach_pbr(cube, #{
        base_color: srgb(0.4, 0.7, 0.9),
        metallic: 0.2,
        roughness: 0.4,
    });
    set_translation(cube, [0.0, 0.0, 0.0]);
}

for cam in query("DragCam") {
    attach_camera_3d(cam, #{
        projection: "perspective",
        fov_degrees: 60.0,
        position: [0.0, 0.0, 6.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
    });
}

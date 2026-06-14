// object_viewer_3d::setup —— 第一帧 attach subject mesh + orbit camera。

let states = query("OrbitSetup");
if states.len() == 0 { return; }
let state = states[0];
if get(state, "OrbitSetup", "done") == 1 { return; }
set(state, "OrbitSetup", "done", 1);

for s in query("Subject") {
    attach_mesh(s, #{ Cuboid: [1.5, 1.5, 1.5] });
    attach_pbr(s, #{
        base_color: srgb(0.6, 0.7, 0.85),
        metallic: 0.4,
        roughness: 0.35,
    });
    set_translation(s, [0.0, 0.0, 0.0]);
}

for cam in query("OrbitCam") {
    attach_camera_3d(cam, #{
        projection: "perspective",
        fov_degrees: 60.0,
        position: [0.0, 0.0, 6.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
    });
}

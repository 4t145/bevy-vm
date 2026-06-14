// orbit::setup —— 第一帧给 Mover / OrbitCamera 挂上 Bevy 端的视觉组件。
// done flag 防止重复 attach。

let states = query("SetupState");
if states.len() == 0 { return; }
let state = states[0];
if get(state, "SetupState", "done") == 1 { return; }
set(state, "SetupState", "done", 1);

for entity in query("Mover") {
    let r = get(entity, "Mover", "color_r");
    let g = get(entity, "Mover", "color_g");
    let b = get(entity, "Mover", "color_b");
    let shape = get(entity, "Mover", "shape");
    let size = get(entity, "Mover", "size");

    let mesh_spec = if shape == "Sphere" {
        #{ Sphere: size }
    } else {
        #{ Cuboid: [size, size, size] }
    };
    attach_mesh(entity, mesh_spec);
    attach_pbr(entity, #{
        base_color: srgb(r, g, b),
        metallic: 0.1,
        roughness: 0.6,
    });

    let px = get(entity, "Pos", "x");
    let py = get(entity, "Pos", "y");
    let pz = get(entity, "Pos", "z");
    set_translation(entity, [px, py, pz]);
}

// 相机：放在 +Z 方向看向原点。
for cam in query("OrbitCamera") {
    attach_camera_3d(cam, #{
        projection: "perspective",
        fov_degrees: 60.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 4.0, 12.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        clear_color: srgb(0.05, 0.06, 0.08),
    });
}

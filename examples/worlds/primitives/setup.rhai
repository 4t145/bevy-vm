// primitives::setup —— 第一帧给 Spin / Ground / Camera 挂 Bevy 视觉组件。

let states = query("PrimitivesSetup");
if states.len() == 0 { return; }
let state = states[0];
if get(state, "PrimitivesSetup", "done") == 1 { return; }
set(state, "PrimitivesSetup", "done", 1);

for entity in query("Spin") {
    let r = get(entity, "Spin", "color_r");
    let g = get(entity, "Spin", "color_g");
    let b = get(entity, "Spin", "color_b");
    let shape = get(entity, "Spin", "shape");
    let metallic = get(entity, "Spin", "metallic");
    let roughness = get(entity, "Spin", "roughness");

    let mesh_spec = if shape == "Sphere" {
        #{ Sphere: 0.8 }
    } else if shape == "Cylinder" {
        #{ Cylinder: [0.7, 1.6] }
    } else if shape == "Capsule" {
        #{ Capsule: [0.5, 1.2] }
    } else if shape == "Cone" {
        #{ Cone: [0.8, 1.6] }
    } else if shape == "ConicalFrustum" {
        #{ ConicalFrustum: [0.3, 0.8, 1.6] }
    } else if shape == "Torus" {
        #{ Torus: [0.3, 0.8] }
    } else if shape == "Tetrahedron" {
        #{ Tetrahedron: 1.6 }
    } else {
        #{ Cuboid: [1.4, 1.4, 1.4] }
    };
    attach_mesh(entity, mesh_spec);
    attach_pbr(entity, #{
        base_color: srgb(r, g, b),
        metallic: metallic,
        roughness: roughness,
    });

    let x = get(entity, "Spin", "x");
    set_translation(entity, [x, 0.0, 0.0]);
}

for ground in query("Ground") {
    attach_mesh(ground, #{ Plane: [12.0, 4.0] });
    attach_pbr(ground, #{
        base_color: srgb(0.25, 0.25, 0.28),
        roughness: 0.9,
    });
    set_translation(ground, [0.0, -1.5, 0.0]);
}

for cam in query("PrimitivesCamera") {
    attach_camera_3d(cam, #{
        projection: "perspective",
        fov_degrees: 55.0,
        position: [0.0, 4.0, 12.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        clear_color: srgb(0.05, 0.06, 0.08),
    });
}

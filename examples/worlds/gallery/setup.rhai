// gallery::setup —— glTF / 程序图元混搭。

let states = query("GallerySetup");
if states.len() == 0 { return; }
let state = states[0];
if get(state, "GallerySetup", "done") == 1 { return; }
set(state, "GallerySetup", "done", 1);

for entity in query("Spin") {
    let asset = get(entity, "Spin", "asset");
    let shape = get(entity, "Spin", "shape");
    let x = get(entity, "Spin", "x");
    let y = get(entity, "Spin", "y");
    let z = get(entity, "Spin", "z");
    let s = get(entity, "Spin", "scale");

    if asset != "" {
        attach_scene(entity, asset);
    } else if shape == "Sphere" {
        let r = get(entity, "Spin", "radius");
        attach_mesh(entity, #{ Sphere: r });
        attach_pbr(entity, #{
            base_color: srgba(
                get(entity, "Spin", "color_r"),
                get(entity, "Spin", "color_g"),
                get(entity, "Spin", "color_b"),
                get(entity, "Spin", "color_a"),
            ),
            metallic: get(entity, "Spin", "metallic"),
            roughness: get(entity, "Spin", "roughness"),
            emissive: srgba(
                get(entity, "Spin", "emissive_r"),
                get(entity, "Spin", "emissive_g"),
                get(entity, "Spin", "emissive_b"),
                1.0,
            ),
        });
    } else if shape == "Cuboid" {
        attach_mesh(entity, #{ Cuboid: [1.5, 1.5, 1.5] });
        attach_pbr(entity, #{
            base_color: srgba(
                get(entity, "Spin", "color_r"),
                get(entity, "Spin", "color_g"),
                get(entity, "Spin", "color_b"),
                get(entity, "Spin", "color_a"),
            ),
            metallic: get(entity, "Spin", "metallic"),
            roughness: get(entity, "Spin", "roughness"),
        });
    }
    set_transform(entity, [x, y, z], [0.0, 0.0, 0.0], [s, s, s]);
}

for cam in query("GalleryCamera") {
    attach_camera_3d(cam, #{
        projection: "perspective",
        fov_degrees: 55.0,
        position: [0.0, 4.0, 12.0],
        target: [0.0, 1.5, 0.0],
        up: [0.0, 1.0, 0.0],
        clear_color: srgb(0.05, 0.06, 0.08),
    });
}

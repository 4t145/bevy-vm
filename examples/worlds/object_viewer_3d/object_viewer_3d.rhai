// 轨道相机：右键拖动环视 + 滚轮 zoom。
//
// 状态：Zoom.distance / Orbit.yaw / Orbit.pitch / Orbit.active。
// 每帧把球坐标 -> Bevy 端 Camera3d 的 Transform。

const PITCH_LIMIT = 1.5607963; // π/2 - 0.01

for e in query("Orbit") {
    for ev in events("MouseButton") {
        if ev.button == "Right" {
            let next = if ev.state == "Pressed" { 1 } else { 0 };
            set(e, "Orbit", "active", next);
        }
    }

    let active = get(e, "Orbit", "active");
    if active == 1 {
        let sensitivity = get(e, "Orbit", "sensitivity");
        let yaw = get(e, "Orbit", "yaw");
        let pitch = get(e, "Orbit", "pitch");
        for ev in events("CursorMoved") {
            if ev.delta != () {
                yaw -= ev.delta[0] * sensitivity;
                pitch += ev.delta[1] * sensitivity;
            }
        }
        if pitch > PITCH_LIMIT { pitch = PITCH_LIMIT; }
        if pitch < -PITCH_LIMIT { pitch = -PITCH_LIMIT; }
        set(e, "Orbit", "yaw", yaw);
        set(e, "Orbit", "pitch", pitch);
    }

    let distance = get(e, "Zoom", "distance");
    let min_d = get(e, "Zoom", "min_distance");
    let max_d = get(e, "Zoom", "max_distance");
    let speed = get(e, "Zoom", "speed");
    let total_dy = 0.0;
    for ev in events("MouseWheel") { total_dy += ev.y; }
    if total_dy != 0.0 {
        distance -= total_dy * speed;
        if distance < min_d { distance = min_d; }
        if distance > max_d { distance = max_d; }
        set(e, "Zoom", "distance", distance);
    }

    // 球坐标 -> 相机位置 + lookAt 原点。
    // 这里通过 attach_camera_3d 重新挂相机参数（含 Transform），等价于一个
    // 每帧"重设相机"的轻量做法。视觉上完全 OK。
    let yaw_now = get(e, "Orbit", "yaw");
    let pitch_now = get(e, "Orbit", "pitch");
    let dist_now = get(e, "Zoom", "distance");
    let cp = pitch_now.cos();
    let x = dist_now * cp * yaw_now.sin();
    let y = dist_now * pitch_now.sin();
    let z = dist_now * cp * yaw_now.cos();
    attach_camera_3d(e, #{
        projection: "perspective",
        fov_degrees: 60.0,
        position: [x, y, z],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
    });
}

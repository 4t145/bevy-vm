// 轨道相机：右键拖动环视 + 滚轮 zoom。
//
// 状态：
//   Zoom.distance ∈ [min_distance, max_distance]
//   Orbit.yaw / Orbit.pitch（弧度）
//   Orbit.active：右键是否按住（用 0/1 而非 bool 简化算术）。
//
// 每帧：
//   1. 处理 MouseButton 事件，更新 Orbit.active。
//   2. 仅在 active 时累加 CursorMoved.delta 到 yaw/pitch；pitch 钳到 ±π/2-ε。
//   3. 处理 MouseWheel 累加到 distance；钳到 [min, max]。
//   4. 由 (yaw, pitch, distance) 算球坐标，写回 Position。

// pitch 接近 ±π/2 时 lookAt 退化（与 up 共线）；留 0.01 弧度 (~0.6°) 缓冲区。
const PITCH_LIMIT = 1.5607963; // π/2 - 0.01

for e in query("Orbit") {
    // ---- 1. 右键按住状态 ----
    for ev in events("MouseButton") {
        if ev.button == "Right" {
            let next = if ev.state == "Pressed" { 1 } else { 0 };
            set(e, "Orbit", "active", next);
        }
    }

    // ---- 2. 拖动累加 yaw/pitch ----
    let active = get(e, "Orbit", "active");
    if active == 1 {
        let sensitivity = get(e, "Orbit", "sensitivity");
        let yaw = get(e, "Orbit", "yaw");
        let pitch = get(e, "Orbit", "pitch");
        for ev in events("CursorMoved") {
            if ev.delta != () {
                // 屏幕 x 向右增→ yaw 减小（相机绕物体逆时针看去），y 向下增→ pitch 减小。
                yaw -= ev.delta[0] * sensitivity;
                pitch += ev.delta[1] * sensitivity;
            }
        }
        // clamp pitch
        if pitch > PITCH_LIMIT {
            pitch = PITCH_LIMIT;
        }
        if pitch < -PITCH_LIMIT {
            pitch = -PITCH_LIMIT;
        }
        set(e, "Orbit", "yaw", yaw);
        set(e, "Orbit", "pitch", pitch);
    }

    // ---- 3. 滚轮 zoom ----
    let distance = get(e, "Zoom", "distance");
    let min_d = get(e, "Zoom", "min_distance");
    let max_d = get(e, "Zoom", "max_distance");
    let speed = get(e, "Zoom", "speed");
    let total_dy = 0.0;
    for ev in events("MouseWheel") {
        total_dy += ev.y;
    }
    if total_dy != 0.0 {
        distance -= total_dy * speed;
        if distance < min_d {
            distance = min_d;
        }
        if distance > max_d {
            distance = max_d;
        }
        set(e, "Zoom", "distance", distance);
    }

    // ---- 4. 球坐标 → Position ----
    // yaw 绕 Y，pitch 绕 X。target 假定是 [0,0,0]。
    let yaw_now = get(e, "Orbit", "yaw");
    let pitch_now = get(e, "Orbit", "pitch");
    let dist_now = get(e, "Zoom", "distance");
    let cp = cos(pitch_now);
    let x = dist_now * cp * sin(yaw_now);
    let y = dist_now * sin(pitch_now);
    let z = dist_now * cp * cos(yaw_now);
    set(e, "Position", "x", x);
    set(e, "Position", "y", y);
    set(e, "Position", "z", z);
}

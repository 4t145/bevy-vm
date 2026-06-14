// primitives::spin —— 让每个 Spin 实体绕 Y 轴持续自转。
//
// 在 Spin.yaw_deg 累加角度，再换算成弧度调 set_yaw 写到 Bevy Transform。

let dt = delta();

for e in query("Spin") {
    let speed = get(e, "Spin", "speed");
    let yaw = get(e, "Spin", "yaw_deg") + speed * 60.0 * dt;
    if yaw >= 360.0 { yaw -= 360.0; }
    set(e, "Spin", "yaw_deg", yaw);
    set_yaw(e, yaw * 3.1415927 / 180.0);
}

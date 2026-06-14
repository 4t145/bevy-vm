// gallery::spin —— 让带 Spin.speed > 0 的实体绕 Y 轴自转。
//
// set_yaw 仅改 Transform.rotation，保留 set_transform 设的 translation/scale。
// 对于 glTF 场景，set_yaw 也作用在场景根 Transform 上（子节点跟随）。

let dt = delta();

for e in query("Spin") {
    let speed = get(e, "Spin", "speed");
    if speed == 0.0 { continue; }
    let yaw = get(e, "Spin", "yaw_deg") + speed * 60.0 * dt;
    if yaw >= 360.0 { yaw -= 360.0; }
    set(e, "Spin", "yaw_deg", yaw);
    set_yaw(e, yaw * 3.1415927 / 180.0);
}

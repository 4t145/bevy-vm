// 让每个带 Spin 的实体绕 Y 轴持续自转，角度累加并对 360 取模。
for e in query("Spin") {
    let speed = get(e, "Spin", "speed");
    let angle = get(e, "Rotation", "y") + speed;
    if angle >= 360.0 {
        angle -= 360.0;
    }
    set(e, "Rotation", "y", angle);
}

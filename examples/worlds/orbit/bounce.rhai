// orbit::bounce —— Pos += Vel * dt；越界翻转 Vel；同步到 Bevy Transform。

let dt = delta();

fn bounce_axis(e, axis, bound) {
    let pos = get(e, "Pos", axis);
    let vel = get(e, "Vel", axis);
    if (pos > bound && vel > 0.0) || (pos < -bound && vel < 0.0) {
        set(e, "Vel", axis, -vel);
    }
}

for e in query("Mover") {
    let bound = get(e, "Mover", "bound");

    // 积分位置（dt 为秒；这里 vel 单位是"每帧"，所以这里其实当作"按 60fps
    // 标定的速度"——保留旧 demo 行为。乘以 60*dt 让它对帧率不敏感。
    let scale = 60.0 * dt;
    let nx = get(e, "Pos", "x") + get(e, "Vel", "x") * scale;
    let ny = get(e, "Pos", "y") + get(e, "Vel", "y") * scale;
    let nz = get(e, "Pos", "z") + get(e, "Vel", "z") * scale;
    set(e, "Pos", "x", nx);
    set(e, "Pos", "y", ny);
    set(e, "Pos", "z", nz);

    bounce_axis(e, "x", bound);
    bounce_axis(e, "y", bound);
    bounce_axis(e, "z", bound);

    set_translation(e, [nx, ny, nz]);
}

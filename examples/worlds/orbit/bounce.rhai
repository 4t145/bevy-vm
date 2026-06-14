// 让每个 Mover 实体在 [-bound, bound] 区间内来回弹跳。
// 引擎在脚本之后把 Velocity 积分进 Position，这里只需在越界时翻转速度方向。
fn bounce_axis(e, axis, bound) {
    let pos = get(e, "Position", axis);
    let vel = get(e, "Velocity", axis);
    if (pos > bound && vel > 0.0) || (pos < -bound && vel < 0.0) {
        set(e, "Velocity", axis, -vel);
    }
}

for e in query("Mover") {
    let bound = get(e, "Mover", "bound");
    bounce_axis(e, "x", bound);
    bounce_axis(e, "y", bound);
    bounce_axis(e, "z", bound);
}

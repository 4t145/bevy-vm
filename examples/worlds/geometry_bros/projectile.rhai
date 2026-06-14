// projectile plugin / projectile system：每帧推进位置 + 倒计寿命。

let dt = delta();

for proj in query("Projectile") {
    let lifetime = get(proj, "Projectile", "lifetime") - dt;
    if lifetime <= 0.0 {
        despawn(proj);
        continue;
    }
    set(proj, "Projectile", "lifetime", lifetime);
    let vx = get(proj, "Projectile", "vx");
    let vz = get(proj, "Projectile", "vz");
    let x = get(proj, "Position", "x") + vx * dt;
    let z = get(proj, "Position", "z") + vz * dt;
    set(proj, "Position", "x", x);
    set(proj, "Position", "z", z);
}

// bullet：每帧推进 + 寿命到期 despawn。

let dt = delta();

for b in query("Bullet") {
    let lifetime = get(b, "Bullet", "lifetime") - dt;
    if lifetime <= 0.0 {
        despawn(b);
        continue;
    }
    set(b, "Bullet", "lifetime", lifetime);
    let x = get(b, "Position", "x") + get(b, "Bullet", "vx") * dt;
    let y = get(b, "Position", "y") + get(b, "Bullet", "vy") * dt;
    let z = get(b, "Position", "z") + get(b, "Bullet", "vz") * dt;
    set(b, "Position", "x", x);
    set(b, "Position", "y", y);
    set(b, "Position", "z", z);
}

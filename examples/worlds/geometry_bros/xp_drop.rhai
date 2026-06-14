// xp module / xp_drop system：监听 EnemyDied，在死亡位置 spawn 经验球。

import "helpers" as h;

for ev in events("combat::EnemyDied") {
    let amount = ev.xp_reward;
    let orb = spawn_entity();
    set_translation(orb, [ev.x, h::ground_y(), ev.z]);
    attach_mesh(orb, #{ Sphere: 0.25 });
    attach_pbr(orb, #{
        base_color: h::color_xp_orb(),
        metallic: 0.5,
        roughness: 0.3,
        emissive: srgba(0.0, 1.5, 2.0, 1.0),
    });
    set(orb, "ExpOrb", "amount", amount);
}

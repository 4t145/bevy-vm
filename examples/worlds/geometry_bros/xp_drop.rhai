// xp plugin / xp_drop system：监听 EnemyDied，在死亡位置 spawn 经验球。

import "helpers" as h;

for ev in events("combat::EnemyDied") {
    let amount = ev.xp_reward;
    let orb = spawn_entity();
    set(orb, "Position", "x", ev.x);
    set(orb, "Position", "y", h::ground_y());
    set(orb, "Position", "z", ev.z);
    set(orb, "Mesh3dRender", "", #{
        mesh: #{ "Sphere": #{ radius: 0.25 } },
        material: #{ "Pbr": #{
            base_color: h::color_xp_orb(),
            metallic: 0.5,
            roughness: 0.3,
            emissive: #{ red: 0.0, green: 1.5, blue: 2.0, alpha: 1.0 },
        } },
    });
    set(orb, "ExpOrb", "amount", amount);
}

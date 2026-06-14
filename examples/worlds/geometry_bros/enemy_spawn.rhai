// enemy plugin / enemy_spawn system：按 spawn_interval 递增难度生成敌人。
//
// 玩家死亡 → 不再 spawn（看 player.hp）。
// 满员（query Enemy >= max_alive）跳过。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];
if get(player, "player::Player", "hp") <= 0 { return; }

let spawners = query("EnemySpawner");
if spawners.len() == 0 { return; }
let spawner = spawners[0];

let now = time();
let next = get(spawner, "EnemySpawner", "next_spawn_time");
if now < next { return; }

// 数量上限
let alive = query("Enemy").len();
let max_alive = get(spawner, "EnemySpawner", "max_alive");
if alive >= max_alive { return; }

// 难度梯度：survived 越久 spawn 间隔越短，且更倾向大型敌人。
let survived = get(player, "player::Player", "survived");
let interval = 1.0 - (survived * 0.01);
if interval < 0.15 { interval = 0.15; }
set(spawner, "EnemySpawner", "spawn_interval", interval);
set(spawner, "EnemySpawner", "next_spawn_time", now + interval);

// 选 kind——random_int 选权重。
let roll = random_int(0, 100);
let kind = "";
let hp = 0;
let damage = 0;
let speed = 0.0;
let xp_reward = 0;
let mesh = #{};
let color = h::srgba(0.0,0.0,0.0,1.0);

// 难度越高大型越多。
let large_threshold = 90 - (survived * 0.3).to_int();
let medium_threshold = 70 - (survived * 0.2).to_int();
if large_threshold < 50 { large_threshold = 50; }
if medium_threshold < 30 { medium_threshold = 30; }

// 三种敌人——都是多面体（与主角的球体形成对比）：
// small  = 四面体（最少面、最尖锐，符合"快/小"的语感）
// medium = 立方体
// large  = 八面体（用 Cuboid 拉伸略扁，立体且醒目；rhai mesh 没有 Octahedron
//          所以用 Cylinder 顶/底六边形——视觉上像多面体棱柱）
if roll >= large_threshold {
    kind = "large";
    hp = 40; damage = 20; speed = 1.5; xp_reward = 5;
    mesh = #{ "Cylinder": #{ radius: 0.55, height: 1.0 } };
    color = h::color_enemy_large();
} else if roll >= medium_threshold {
    kind = "medium";
    hp = 20; damage = 10; speed = 2.0; xp_reward = 2;
    mesh = #{ "Cube": #{ size: [0.8, 0.8, 0.8] } };
    color = h::color_enemy_medium();
} else {
    kind = "small";
    hp = 10; damage = 5; speed = 2.5; xp_reward = 1;
    mesh = #{ "Tetrahedron": #{ size: 0.7 } };
    color = h::color_enemy_small();
}

// 在玩家四周圆周外 spawn。
let px = get(player, "Position", "x");
let pz = get(player, "Position", "z");
let angle = random_range(0.0, 6.2831855);
let radius = h::arena_radius();
let ex = px + angle.cos() * radius;
let ez = pz + angle.sin() * radius;

// arena 边界 clamp——别 spawn 出地面。
let bound = h::arena_radius() - 0.5;
if ex >  bound { ex =  bound; }
if ex < -bound { ex = -bound; }
if ez >  bound { ez =  bound; }
if ez < -bound { ez = -bound; }

let e = spawn_entity();
set(e, "Position", "x", ex);
set(e, "Position", "y", h::ground_y());
set(e, "Position", "z", ez);
set(e, "Mesh3dRender", "", #{
    mesh: mesh,
    material: #{ "Pbr": #{
        base_color: color,
        metallic: 0.1,
        roughness: 0.5,
    } },
});
set(e, "Enemy", "kind", kind);
set(e, "Enemy", "hp", hp);
set(e, "Enemy", "damage", damage);
set(e, "Enemy", "speed", speed);
set(e, "Enemy", "xp_reward", xp_reward);

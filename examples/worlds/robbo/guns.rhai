// Gun 系统：每个 } 每 GUN_TICK_RATE 帧有 SHOOT_PROP_PERMIL/1000 概率发射
// 一发子弹。子弹方向由 Cell.dir_i 决定（默认朝南 1）。
//
// 子弹与玩家子弹用同一个 bullets::Bullet 组件 + bullets.rhai 推进。

import "sprites" as sp;
import "sounds" as snd;

const GUN_TICK_RATE = 6;
const SHOOT_PROP_PERMIL = 50;   // 5%

let game = query_first("board::Game");
if game == () { return; }

if get(game, "board::Game", "state") != 0 { return; }

let tick = get(game, "board::Game", "tick_count");
if tick % GUN_TICK_RATE != 0 { return; }

fn dir_to_step(idx) {
    if idx == 0 { return [1, 0]; }
    if idx == 1 { return [0, 1]; }
    if idx == 2 { return [-1, 0]; }
    [0, -1]
}

for cell in query("board::Cell") {
    let kind = get(cell, "board::Cell", "kind");
    if kind != "}" { continue; }

    if random_int(0, 1000) >= SHOOT_PROP_PERMIL { continue; }

    let gi = get(cell, "board::Cell", "i");
    let gj = get(cell, "board::Cell", "j");
    let dir_idx = get(cell, "board::Cell", "dir_i");
    if dir_idx < 0 || dir_idx >= 4 { dir_idx = 1; }   // 默认 South

    let step = dir_to_step(dir_idx);
    let bi = gi + step[0];
    let bj = gj + step[1];

    // 出口被挡（墙 / 其它 cell）就不发——避免立即原地爆炸。
    let blocked = false;
    for c in query("board::Cell") {
        if get(c, "board::Cell", "i") == bi && get(c, "board::Cell", "j") == bj {
            blocked = true;
            break;
        }
    }
    if blocked { continue; }

    let bullet = spawn_entity();
    set(bullet, "bullets::Bullet", "i", bi);
    set(bullet, "bullets::Bullet", "j", bj);
    set(bullet, "bullets::Bullet", "dx", step[0]);
    set(bullet, "bullets::Bullet", "dy", step[1]);
    let sprite_idx = if step[0] != 0 { sp::sprite_bullet_h() } else { sp::sprite_bullet_v() };
    attach_sprite(bullet, #{
        image: "robbo/icons32.png",
        atlas: #{
            tile_size: [32, 32],
            columns: sp::atlas_columns(),
            rows: sp::atlas_rows(),
            index: sprite_idx,
        },
        custom_size: [32.0, 32.0],
    });
    set_translation(bullet, [bi.to_float() * 32.0, -bj.to_float() * 32.0, 1.0]);
    snd::play(snd::snd_shot());
}

// 子弹推进 + 爆炸链反应。
//
// tick 每帧 +1（这里维护，enemies.rhai 只读）。每 BULLET_TICK_RATE 帧推进
// 子弹一格。命中规则：
//   - 越界 → 销毁
//   - 命中 Robbo → 玩家死亡 + 子弹销毁
//   - 命中 destroyable 的 cell：
//       - 'b' Bomb → 触发爆炸链 + 子弹销毁
//       - '?' QuestionMark → 销毁 + 随机出物品 + 子弹销毁
//       - 其它（敌人 / PushBox）→ 销毁该 cell + 子弹销毁
//   - 命中其它 cell（墙/Door/Gun/StaticBox 等不可破坏物） → 子弹销毁
//   - 空地 → 走过去
//
// 炸弹爆炸：销毁 9 格内的 destroyable cells（中心 + 8 邻居，原版用对角+直角
// 两批；简化为一次性销毁 3x3 区域并对其中的 'b' 递归引爆）。Robbo 在范围内
// → 死亡。墙类不被影响。

import "sprites" as sp;
import "sounds" as snd;

const BULLET_TICK_RATE = 3;

let game = query_first("board::Game");
if game == () { return; }

let tick = get(game, "board::Game", "tick_count") + 1;
set(game, "board::Game", "tick_count", tick);

if tick % BULLET_TICK_RATE != 0 { return; }
if get(game, "board::Game", "state") != 0 { return; }

// 棋盘尺寸——别越界。
let levels = load_config("levels.json");
let level_idx = get(game, "board::Game", "level_idx");
let board_h = config_len(levels, `levels.${level_idx}.rows`);
let board_w = 16;

let bullets = query("Bullet");
let player = query_first("robbo::Player");
let player_pos = if player != () {
    [get(player, "robbo::Player", "i"), get(player, "robbo::Player", "j")]
} else { () };

for b in bullets {
    let bi = get(b, "Bullet", "i");
    let bj = get(b, "Bullet", "j");
    let dx = get(b, "Bullet", "dx");
    let dy = get(b, "Bullet", "dy");
    let ni = bi + dx;
    let nj = bj + dy;

    if ni < 0 || ni >= board_w || nj < 0 || nj >= board_h {
        despawn(b);
        continue;
    }

    if player_pos != () && player_pos[0] == ni && player_pos[1] == nj {
        set(game, "board::Game", "state", 2);
        despawn(b);
        continue;
    }

    let hit_cell = find_cell_at(ni, nj);
    if hit_cell != () {
        let kind = get(hit_cell, "board::Cell", "kind");
        if sp::is_destroyable_char(kind) {
            // 子弹直击——非爆炸路径，让 destroy_cell 决定是否响 burn。
            destroy_cell(hit_cell, kind, game, board_w, board_h, false);
        }
        // 不论是否 destroyable，子弹此刻撞到都销毁。
        despawn(b);
        continue;
    }

    // 空地 → 走过去。
    set(b, "Bullet", "i", ni);
    set(b, "Bullet", "j", nj);
    set_translation(b, [ni.to_float() * 32.0, -nj.to_float() * 32.0, 1.0]);
}

// ---- helpers ------------------------------------------------------------

fn find_cell_at(i, j) {
    for cell in query("board::Cell") {
        if get(cell, "board::Cell", "i") == i && get(cell, "board::Cell", "j") == j {
            return cell;
        }
    }
    ()
}

// 销毁单个 cell。'b' → 触发爆炸链；'?' → 销毁 + 随机产生新物品；其它直接 despawn。
//
// `from_explosion`：true 表示销毁是被爆炸波及（链反应），不再各自响 burn——
// 一次爆炸中心已经响过 bomb 音效。bomb 链反应中的 b 仍各自响 bomb。
fn destroy_cell(cell, kind, game, w, h, from_explosion) {
    if kind == "b" {
        let i = get(cell, "board::Cell", "i");
        let j = get(cell, "board::Cell", "j");
        despawn(cell);
        snd::play(snd::snd_bomb());
        explode_at(i, j, game, w, h);
    } else if kind == "?" {
        let i = get(cell, "board::Cell", "i");
        let j = get(cell, "board::Cell", "j");
        despawn(cell);
        snd::play(snd::snd_spawn());
        spawn_random_at(i, j);
    } else {
        if !from_explosion {
            snd::play(snd::snd_burn());
        }
        despawn(cell);
    }
}

// 9 格爆炸：中心 + 8 邻居。
//
// 原版 process_damage 用 `Without<Undestroyable>` 过滤 + bomb damage 标记
// 让"非墙的所有物体"被销毁（包括 # Box、~ PushBox 等 Moveable）。我们
// 这里用 `is_undestroyable_char` 取反——墙保留，其它销毁。
//
// 范围内的 'b' 递归引爆（DFS 链反应）。Robbo 也被算"非墙"——爆炸碰到他
// 就死亡（这里实现走 state=2，不挪到 victims 里）。
fn explode_at(cx, cy, game, w, h) {
    // 玩家在范围内 → 死亡。
    let player = query_first("robbo::Player");
    if player != () {
        let pi = get(player, "robbo::Player", "i");
        let pj = get(player, "robbo::Player", "j");
        if (pi - cx).abs() <= 1 && (pj - cy).abs() <= 1 {
            set(game, "board::Game", "state", 2);
        }
    }

    // 收集范围内所有非 Undestroyable cell——一次性 query 完，避免迭代时
    // 增删。
    let victims = [];
    for cell in query("board::Cell") {
        let ci = get(cell, "board::Cell", "i");
        let cj = get(cell, "board::Cell", "j");
        if (ci - cx).abs() > 1 || (cj - cy).abs() > 1 { continue; }
        let kind = get(cell, "board::Cell", "kind");
        if sp::is_undestroyable_char(kind) { continue; }
        victims.push([cell, kind]);
    }
    for v in victims {
        // entity 可能已被前一次 destroy_cell 拆掉（如同帧多个爆炸覆盖同格）——
        // is_alive 守护一下。
        if !is_alive(v[0]) { continue; }
        destroy_cell(v[0], v[1], game, w, h, true);
    }
}

// QuestionMark 被打 → 在原位随机 spawn 一个物品。原版列表：empty / push_box /
// screw / bullet(ammo) / key / bomb / ground / butterfly / gun / question。
// 这里 weighted-uniform，10 种均概率；'.' 表示什么都不出（空地）。
fn spawn_random_at(i, j) {
    let pool = ["~", "T", "'", "%", "b", "H", "V", "}", "?", "."];
    let idx = random_int(0, pool.len());
    let kind = pool[idx];
    if kind == "." { return; }
    let tile_idx = sp::tile_for_char(kind);
    if tile_idx < 0 { return; }

    let cell = spawn_entity();
    set(cell, "board::Cell", "i", i);
    set(cell, "board::Cell", "j", j);
    set(cell, "board::Cell", "kind", kind);
    attach_sprite(cell, #{
        image: "robbo/icons32.png",
        atlas: #{
            tile_size: [32, 32],
            columns: sp::atlas_columns(),
            rows: sp::atlas_rows(),
            index: tile_idx,
        },
        custom_size: [32.0, 32.0],
    });
    set_translation(cell, [i.to_float() * 32.0, -j.to_float() * 32.0, 0.0]);
}

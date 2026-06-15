// 简化版传送：Robbo 走到 '&' 时被弹到下一个 '&' 的输出格（沿同方向相邻空地）。
//
// 走法：robbo.rhai 把 '&' 当作"可走的空地"（既不阻挡也不拾取）——这部分由
// is_solid_char 不含 '&' + 不是 pushable / pickup 实现，自然走过去。本系统
// 在 robbo 跑完后查 Robbo 是否站在 '&' 上，若是则触发瞬移。
//
// 关 1 的 '&' 数量：3。同 group 多次循环找空地，避免吐到墙里。

import "sounds" as snd;

let game = query_first("board::Game");
if game == () { return; }

if get(game, "board::Game", "state") != 0 { return; }

let player = query_first("robbo::Player");
if player == () { return; }
let pi = get(player, "robbo::Player", "i");
let pj = get(player, "robbo::Player", "j");

// 找当前格是不是 '&'。
let on_teleport = false;
let entered_id = -1;
let teleports = [];
for cell in query("board::Cell") {
    if get(cell, "board::Cell", "kind") == "&" {
        let ti = get(cell, "board::Cell", "i");
        let tj = get(cell, "board::Cell", "j");
        teleports.push([ti, tj]);
        if ti == pi && tj == pj {
            on_teleport = true;
            entered_id = teleports.len() - 1;
        }
    }
}
if !on_teleport { return; }
if teleports.len() < 2 { return; }

// "刚瞬移到这里" 的格不再二次触发——避免在两个 '&' 之间反复弹。Robbo 必须
// 走出去再进来才能再次传。
let last_i = get(player, "robbo::Player", "last_tp_i");
let last_j = get(player, "robbo::Player", "last_tp_j");
if last_i == pi && last_j == pj { return; }

// 收集所有被占格，找输出空地用。
let occupied = [];
for cell in query("board::Cell") {
    let ci = get(cell, "board::Cell", "i");
    let cj = get(cell, "board::Cell", "j");
    let kind = get(cell, "board::Cell", "kind");
    // '&' 自己不算阻挡（玩家可以踩在上面继续传送）。
    if kind != "&" {
        occupied.push([ci, cj]);
    }
}

// 沿四个方向找第一个空地，作为出口。
fn find_exit(ti, tj, occupied) {
    let dirs = [[1, 0], [-1, 0], [0, 1], [0, -1]];
    for d in dirs {
        let ni = ti + d[0];
        let nj = tj + d[1];
        if ni < 0 || ni >= 16 || nj < 0 { continue; }
        let blocked = false;
        for slot in occupied {
            if slot[0] == ni && slot[1] == nj { blocked = true; break; }
        }
        if !blocked {
            return [ni, nj];
        }
    }
    ()
}

// 从 entered_id+1 开始环形找下一个 '&'，直到能找到合法出口。
let n = teleports.len();
let exit = ();
for k in 1..n {
    let idx = (entered_id + k) % n;
    let t = teleports[idx];
    let candidate = find_exit(t[0], t[1], occupied);
    if candidate != () {
        exit = candidate;
        break;
    }
}
if exit == () { return; }

// 把 Robbo 移到 exit；记 last_tp 防止 exit 同样是 '&'-相邻区域被误触。
set(player, "robbo::Player", "i", exit[0]);
set(player, "robbo::Player", "j", exit[1]);
set(player, "robbo::Player", "last_tp_i", exit[0]);
set(player, "robbo::Player", "last_tp_j", exit[1]);
set_translation(player, [exit[0].to_float() * 32.0, -exit[1].to_float() * 32.0, 1.0]);
snd::play(snd::snd_teleport());

// lines plugin 脚本：消费 ClearRows，扫满行清掉、上方下落、加分。
//
// 行的 y 越小越靠上；自下而上扫并保留删除行 y 列表，处理时从下往上"塌"
// 上方所有 Cell 的 value——因为同一时刻可能消除多行，需要重新建立 (x,y) →
// cell 的索引，避免在 query 上反复线性扫。

import "helpers" as h;

let triggered = false;
for _ev in events("ClearRows") {
    triggered = true;
}
if !triggered { return; }

let board_q = query("board::Board");
let score_q = query("piece::Score");
if board_q.len() == 0 || score_q.len() == 0 { return; }
let board = board_q[0];
let score = score_q[0];

let w = get(board, "board::Board", "width");
let bh = get(board, "board::Board", "height");

// 索引：(x,y) → cell entity。一次 query 转 dict 比循环 query 多次便宜。
let by_pos = #{};
for cell in query("board::Cell") {
    let cx = get(cell, "board::Cell", "x");
    let cy = get(cell, "board::Cell", "y");
    by_pos[`${cx},${cy}`] = cell;
}

// 1) 收集满行
let full_rows = [];
for y in 0..bh {
    let full = true;
    for x in 0..w {
        let cell = by_pos[`${x},${y}`];
        if get(cell, "board::Cell", "value") == 0 {
            full = false;
            break;
        }
    }
    if full { full_rows.push(y); }
}

let cleared = full_rows.len();
if cleared == 0 { return; }

// 2) 塌行：每消除 1 行，把它上面所有行往下挪一格。从最小的 y（最高的满行）
// 开始处理，依次把"该行 + 上方"全部下移；注意我们读写 value 而非 entity，
// 所以外层循环每处理一行后，下一行索引在网格上仍稳定不变。
for cleared_y in full_rows {
    // 从 cleared_y 开始向上"拉"
    let y = cleared_y;
    while y > 0 {
        for x in 0..w {
            let dst = by_pos[`${x},${y}`];
            let src = by_pos[`${x},${y - 1}`];
            set(dst, "board::Cell", "value", get(src, "board::Cell", "value"));
        }
        y -= 1;
    }
    // 顶行清空
    for x in 0..w {
        let top = by_pos[`${x},0`];
        set(top, "board::Cell", "value", 0);
    }
}

// 3) 加分 / 升级
let level = get(score, "piece::Score", "level");
let gained = h::score_for_lines(cleared, level);
let total_lines = get(score, "piece::Score", "lines") + cleared;
let new_level = total_lines / h::lines_per_level() + 1;

set(score, "piece::Score", "score", get(score, "piece::Score", "score") + gained);
set(score, "piece::Score", "lines", total_lines);
set(score, "piece::Score", "level", new_level);

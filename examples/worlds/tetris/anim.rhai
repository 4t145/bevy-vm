// anim module 脚本：每帧扫所有 Cell，按 (value, active piece, ghost) 算
// 目标 BackgroundColor 写回去。
//
// 渲染优先级：active piece 4 格 > ghost piece 4 格 > Cell.value > 空。
// active 始终覆盖 ghost——active 触底时 ghost 与 active 重合，颜色用 active 即可。

import "helpers" as h;

let board_q = query("board::Board");
let piece_q = query("piece::Piece");
if board_q.len() == 0 || piece_q.len() == 0 { return; }
let board = board_q[0];
let piece = piece_q[0];

let w  = get(board, "board::Board", "width");
let bh = get(board, "board::Board", "height");

// 索引 cell：(x,y) → entity。
let by_pos = #{};
for cell in query("board::Cell") {
    let cx = get(cell, "board::Cell", "x");
    let cy = get(cell, "board::Cell", "y");
    by_pos[`${cx},${cy}`] = cell;
}

// 计算 active 4 格 + ghost 4 格的 (x,y)。
let kind = get(piece, "piece::Piece", "kind");
let active_cells = [];
let ghost_cells = [];

if kind > 0 {
    let rot = get(piece, "piece::Piece", "rotation");
    let px = get(piece, "piece::Piece", "x");
    let py = get(piece, "piece::Piece", "y");
    for off in h::shape_offsets(kind, rot) {
        let cx = px + off[0];
        let cy = py + off[1];
        active_cells.push([cx, cy]);
    }
    // ghost：从当前 piece 一直下落到不能落，记录最终落点。
    let drop = 0;
    while piece_drop_fits(kind, rot, px, py + drop + 1, board, by_pos) {
        drop += 1;
    }
    if drop > 0 {
        for off in h::shape_offsets(kind, rot) {
            ghost_cells.push([px + off[0], py + drop + off[1]]);
        }
    }
}

// 第一遍：默认按 Cell.value 涂。
for cell in query("board::Cell") {
    let v = get(cell, "board::Cell", "value");
    let bg = if v == 0 { h::color_empty() } else { h::piece_color(v) };
    set(cell, "BackgroundColor", "", bg);
}

// 第二遍：覆盖 ghost。
for pos in ghost_cells {
    if pos[0] < 0 || pos[0] >= w || pos[1] < 0 || pos[1] >= bh { continue; }
    let cell = by_pos[`${pos[0]},${pos[1]}`];
    if cell != () && get(cell, "board::Cell", "value") == 0 {
        set(cell, "BackgroundColor", "", h::color_ghost());
    }
}

// 第三遍：覆盖 active（最高优先级）。
for pos in active_cells {
    if pos[0] < 0 || pos[0] >= w || pos[1] < 0 || pos[1] >= bh { continue; }
    let cell = by_pos[`${pos[0]},${pos[1]}`];
    if cell != () {
        set(cell, "BackgroundColor", "", h::piece_color(kind));
    }
}

// ghost 投影 fit 检查的本地版本——anim 不能写 piece，所以直接根据偏移
// 表 + Cell.value 判断目标行能否站住。
fn piece_drop_fits(kind, rot, px, py, board, by_pos) {
    let w  = get(board, "board::Board", "width");
    let bh = get(board, "board::Board", "height");
    for off in h::shape_offsets(kind, rot) {
        let cx = px + off[0];
        let cy = py + off[1];
        if cx < 0 || cx >= w { return false; }
        if cy >= bh           { return false; }
        if cy < 0 { continue; }
        let cell = by_pos[`${cx},${cy}`];
        if cell == () { return false; }
        if get(cell, "board::Cell", "value") > 0 { return false; }
    }
    true
}

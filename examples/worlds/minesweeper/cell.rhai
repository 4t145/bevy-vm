// cell plugin 脚本：监听 4 个领域事件。
//
// 顺序很关键——RestartGame 优先（清空才有意义重头来）；其它三个事件按
// 与参考项目相同的语义：Flag → Chord → Reveal。chord 直接调 reveal helper
// 同帧完成展开，避免 chord 的"再 emit 一波 RevealCell" 引入的额外帧延迟。

import "helpers" as h;

let board_q = query("board::Board");
if board_q.len() == 0 {
    return;
}
let board = board_q[0];

// ---- RestartGame ---------------------------------------------------------

let restart = false;
for _ev in events("RestartGame") {
    restart = true;
}
if restart {
    handle_restart(board);
    // restart 后本帧不再处理其他 cell 事件——它们针对的是上一局的实体。
    return;
}

// ---- 游戏进行中才处理交互 ------------------------------------------------

if get(board, "board::Board", "state") != 0 {
    return;
}

// flag 先于 reveal——chord 看 flag 数。
for ev in events("FlagCell") {
    handle_flag(ev.entity);
}
for ev in events("ChordCell") {
    handle_chord(ev.entity, board);
}
for ev in events("RevealCell") {
    handle_reveal(ev.entity, board);
}

// ---- handlers ------------------------------------------------------------

fn handle_flag(entity) {
    if get(entity, "Tile", "revealed") == 1 { return; }
    let flagged = get(entity, "Tile", "flag");
    set(entity, "Tile", "flag", if flagged == 1 { 0 } else { 1 });
}

fn handle_chord(entity, board) {
    let adj = get(entity, "Tile", "adj");
    if adj == 0 { return; }
    if get(entity, "Tile", "revealed") != 1 { return; }
    let cx = get(entity, "Tile", "x");
    let cy = get(entity, "Tile", "y");
    let neighbors = neighbors_of(cx, cy);
    let flagged = 0;
    for n in neighbors {
        if get(n, "Tile", "flag") == 1 { flagged += 1; }
    }
    if flagged != adj { return; }
    for n in neighbors {
        if get(n, "Tile", "revealed") == 0 && get(n, "Tile", "flag") == 0 {
            handle_reveal(n, board);
        }
    }
}

fn handle_reveal(entity, board) {
    if get(entity, "Tile", "revealed") == 1 { return; }
    if get(entity, "Tile", "flag") == 1 { return; }
    if get(board, "board::Board", "mines_placed") == 0 {
        let sx = get(entity, "Tile", "x");
        let sy = get(entity, "Tile", "y");
        populate_mines(board, sx, sy);
        set(board, "board::Board", "mines_placed", 1);
        set(board, "board::Board", "start_time", time());
    }
    reveal_cascade(entity, board);
    check_win_or_loss(board);
}

fn reveal_cascade(start_entity, board) {
    let frontier = [start_entity];
    while frontier.len() > 0 {
        let e = frontier.pop();
        if get(e, "Tile", "revealed") == 1 { continue; }
        if get(e, "Tile", "flag") == 1 { continue; }
        set(e, "Tile", "revealed", 1);
        if get(e, "Tile", "mine") == 1 {
            set(board, "board::Board", "state", 2);
            return;
        }
        if get(e, "Tile", "adj") != 0 { continue; }
        let cx = get(e, "Tile", "x");
        let cy = get(e, "Tile", "y");
        for n in neighbors_of(cx, cy) {
            if get(n, "Tile", "revealed") == 0 && get(n, "Tile", "flag") == 0 {
                frontier.push(n);
            }
        }
    }
}

fn neighbors_of(x, y) {
    let out = [];
    for tile in query("Tile") {
        let tx = get(tile, "Tile", "x");
        let ty = get(tile, "Tile", "y");
        let dx = tx - x;
        let dy = ty - y;
        if dx >= -1 && dx <= 1 && dy >= -1 && dy <= 1 && !(dx == 0 && dy == 0) {
            out.push(tile);
        }
    }
    out
}

fn check_win_or_loss(board) {
    let unrevealed_safe = 0;
    for tile in query("Tile") {
        if get(tile, "Tile", "mine") == 0 && get(tile, "Tile", "revealed") == 0 {
            unrevealed_safe += 1;
        }
    }
    if unrevealed_safe == 0 {
        set(board, "board::Board", "state", 1);
    }
}

fn populate_mines(board, safe_x, safe_y) {
    let mine_count = get(board, "board::Board", "mine_count");
    let candidates = [];
    for tile in query("Tile") {
        let tx = get(tile, "Tile", "x");
        let ty = get(tile, "Tile", "y");
        let dx = tx - safe_x;
        let dy = ty - safe_y;
        if dx >= -1 && dx <= 1 && dy >= -1 && dy <= 1 {
            continue;
        }
        candidates.push(tile);
    }
    let cap = candidates.len();
    let want = if mine_count > cap { cap } else { mine_count };
    let placed = 0;
    while placed < want {
        let idx = random_int(0, cap);
        let tile_e = candidates[idx];
        if get(tile_e, "Tile", "mine") == 0 {
            set(tile_e, "Tile", "mine", 1);
            placed += 1;
        }
    }
    compute_adjacency(board);
}

fn compute_adjacency(board) {
    let w = get(board, "board::Board", "width");
    let h = get(board, "board::Board", "height");
    let by_pos = #{};
    for tile in query("Tile") {
        let tx = get(tile, "Tile", "x");
        let ty = get(tile, "Tile", "y");
        by_pos[`${tx},${ty}`] = tile;
    }
    for y in 0..h {
        for x in 0..w {
            let e = by_pos[`${x},${y}`];
            if get(e, "Tile", "mine") == 1 { continue; }
            let n = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 { continue; }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || nx >= w || ny < 0 || ny >= h { continue; }
                    let neighbor = by_pos[`${nx},${ny}`];
                    if get(neighbor, "Tile", "mine") == 1 {
                        n += 1;
                    }
                }
            }
            set(e, "Tile", "adj", n);
        }
    }
}

fn handle_restart(board) {
    for tile in query("Tile") {
        despawn(tile);
    }
    set(board, "board::Board", "layout_built", 0);
    set(board, "board::Board", "mines_placed", 0);
    set(board, "board::Board", "state", 0);
    set(board, "board::Board", "start_time", 0.0);
}

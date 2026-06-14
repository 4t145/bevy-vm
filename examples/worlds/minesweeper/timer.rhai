// timer plugin 脚本：每帧根据 Board 状态推进 GameTimer.elapsed。
//
// - mines_placed=0 时 elapsed=0（还没开局）
// - state==0（playing）时 elapsed = time() - start_time
// - state!=0（won/lost）时 elapsed 冻结在最后一次更新值

let board_q = query("board::Board");
let timer_q = query("GameTimer");
if board_q.len() == 0 || timer_q.len() == 0 {
    return;
}
let board = board_q[0];
let timer = timer_q[0];

let mines_placed = get(board, "board::Board", "mines_placed");
let state = get(board, "board::Board", "state");

if mines_placed == 0 {
    set(timer, "GameTimer", "elapsed", 0.0);
} else if state == 0 {
    let start = get(board, "board::Board", "start_time");
    set(timer, "GameTimer", "elapsed", time() - start);
}
// won / lost 时不写——保持上一帧的值。

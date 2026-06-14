// difficulty plugin 脚本：消费 ChangeDifficulty，更新 Board 参数，再
// emit cell::RestartGame 让 cell plugin 在下一 tick 清空 + 让 board
// plugin 在再下一 tick 用新参数重建。
//
// 三档对齐 bevy-lab/difficulty.rs：
//   Easy  9x9   / 10
//   Med   16x16 / 40
//   Hard  16x16 / 99（参考是 30x16，VM 端尚无相机自适应，先用更紧凑尺寸）

let board_q = query("board::Board");
if board_q.len() == 0 {
    return;
}
let board = board_q[0];

for ev in events("ChangeDifficulty") {
    if ev.which == "Easy" {
        set(board, "board::Board", "width", 9);
        set(board, "board::Board", "height", 9);
        set(board, "board::Board", "mine_count", 10);
    } else if ev.which == "Med" {
        set(board, "board::Board", "width", 16);
        set(board, "board::Board", "height", 16);
        set(board, "board::Board", "mine_count", 40);
    } else if ev.which == "Hard" {
        set(board, "board::Board", "width", 16);
        set(board, "board::Board", "height", 16);
        set(board, "board::Board", "mine_count", 99);
    } else {
        continue;
    }
    emit("cell::RestartGame", #{});
}

// 跨 module 共用常量。`import "consts" as c;` 引入。

fn board_w_int() { 14 }
fn board_h_int() { 21 }
fn board_w() { 14.0 }
fn board_h() { 21.0 }
fn move_cooldown() { 0.3 }   // 玩家移动间隔（秒）
fn spawn_interval() { 5.0 }  // bonus 刷新间隔
fn camera_focus_speed() { 2.0 }
fn camera_focus_threshold() { 0.2 }

// score 调整。
fn score_eat() { 2 }
fn score_miss() { 3 }
fn score_lose_threshold() { -5 }

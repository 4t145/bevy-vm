// pipes plugin 脚本：spawn / 滚动 / 计分 / despawn。
//
// 顺序：RestartGame → spawn 新管道 → 推进所有管道 + 计分 + 离屏 despawn。
// dead 时不动管道（让玩家看到死亡那一刻的画面）。

import "helpers" as h;

let game_q = query("game::Game");
let spawner_q = query("PipeSpawner");
let play_q = query("PlayArea");
if game_q.len() == 0 || spawner_q.len() == 0 { return; }
let game = game_q[0];
let spawner = spawner_q[0];

// ---- RestartGame ---------------------------------------------------------

let restart = false;
for _ev in events("bird::RestartGame") {
    restart = true;
}
if restart {
    handle_restart(spawner);
    return;
}

// ---- dead 时静止 --------------------------------------------------------

if get(game, "game::Game", "state") != 0 { return; }
if play_q.len() == 0 { return; }
let play = play_q[0];

// ---- spawn 节奏 ---------------------------------------------------------

let elapsed = get(game, "game::Game", "elapsed");
let last = get(spawner, "PipeSpawner", "last_spawn");
let due = if last == 0.0 {
    elapsed >= h::pipe_initial_delay()
} else {
    elapsed - last >= h::pipe_spawn_period()
};

if due {
    spawn_pipe_pair(play, elapsed);
    set(spawner, "PipeSpawner", "last_spawn", elapsed);
}

// ---- 推进所有 pipe + 计分 + 离屏 despawn --------------------------------

let dt = delta();
let speed = h::pipe_speed();
let bx = h::bird_x();
let pw = h::pipe_width();
let half_play = h::play_width() * 0.5;

let dead_pipes = [];
let gained = 0;

for pipe in query("Pipe") {
    let new_x = get(pipe, "Pipe", "pos_x") - speed * dt;
    set(pipe, "Pipe", "pos_x", new_x);
    update_pipe_sprites(pipe);

    // 计分：管道**右侧**越过 bird_x 后第一帧 += 1。
    if get(pipe, "Pipe", "passed") == 0 && new_x + pw * 0.5 < bx {
        set(pipe, "Pipe", "passed", 1);
        gained += 1;
    }

    // 离屏：管道完全在 PlayArea 左侧之外。
    if new_x + pw * 0.5 < -half_play {
        dead_pipes.push(pipe);
    }
}

if gained > 0 {
    set(game, "game::Game", "score", get(game, "game::Game", "score") + gained);
}

for p in dead_pipes {
    despawn_pipe(p);
}

// ===========================================================================
// helpers
// ===========================================================================

fn spawn_pipe_pair(play, elapsed) {
    let pipe = spawn_entity();
    let start_x = h::play_width() * 0.5 + h::pipe_width() * 0.5 + 4.0;
    let gap_y = pick_gap_y(elapsed);
    set(pipe, "Pipe", "pos_x", start_x);
    set(pipe, "Pipe", "gap_y", gap_y);
    set(pipe, "Pipe", "passed", 0);

    let pw = h::pipe_width();
    let gap = h::pipe_gap();
    let ph = h::play_height();
    let gh = h::ground_height();

    // top 管道：从 playfield 顶 (top=0) 一直到 gap 上沿。
    // PlayArea 内坐标：top=0 是 playfield 顶；gap 上沿在 playfield 物理坐标
    // y = gap_y + gap/2，对应 top = ph/2 - (gap_y + gap/2)。
    let top_height = ph * 0.5 - (gap_y + gap * 0.5);
    let top = spawn_entity();
    set(top, "Node", "", #{
        position_type: "Absolute",
        left: px(start_x + h::play_width() * 0.5 - pw * 0.5),
        top: px(0.0),
        width: px(pw),
        height: px(top_height),
        border: #{
            left: px(2.0), right: px(2.0),
            top: px(2.0),  bottom: px(2.0),
        },
    });
    set(top, "BackgroundColor", "", h::color_pipe());
    let edge = h::color_pipe_edge();
    set(top, "BorderColor", "", #{
        top: edge, right: edge, bottom: edge, left: edge,
    });
    set_parent(top, play);

    // bottom 管道：从 gap 下沿一直到地面顶。
    // gap 下沿物理 y = gap_y - gap/2，对应 top = ph/2 - (gap_y - gap/2)。
    // height 直到地面顶 = ph - gh - 该 top 值。
    let bot_top = ph * 0.5 - (gap_y - gap * 0.5);
    let bot_height = ph - gh - bot_top;
    let bot = spawn_entity();
    set(bot, "Node", "", #{
        position_type: "Absolute",
        left: px(start_x + h::play_width() * 0.5 - pw * 0.5),
        top: px(bot_top),
        width: px(pw),
        height: px(bot_height),
        border: #{
            left: px(2.0), right: px(2.0),
            top: px(2.0),  bottom: px(2.0),
        },
    });
    set(bot, "BackgroundColor", "", h::color_pipe());
    set(bot, "BorderColor", "", #{
        top: edge, right: edge, bottom: edge, left: edge,
    });
    set_parent(bot, play);

    set(pipe, "Pipe", "top_sprite", top);
    set(pipe, "Pipe", "bottom_sprite", bot);
}

// 让 gap_y 在区间内随机；用 elapsed 让 random_int 每次都换种子（避免连续
// 局完全相同的图案）。random_int 是 [low, high)。
fn pick_gap_y(elapsed) {
    let lo = h::pipe_min_center_y().to_int();
    let hi = h::pipe_max_center_y().to_int();
    let r = random_int(lo, hi + 1);
    r.to_float()
}

fn update_pipe_sprites(pipe) {
    let pw = h::pipe_width();
    let pos_x = get(pipe, "Pipe", "pos_x");
    // PlayArea 内 left = pos_x + play_width/2 - pw/2
    let left = pos_x + h::play_width() * 0.5 - pw * 0.5;

    let top = get(pipe, "Pipe", "top_sprite");
    if top != 0 {
        set(top, "Node", "left", px(left));
    }
    let bot = get(pipe, "Pipe", "bottom_sprite");
    if bot != 0 {
        set(bot, "Node", "left", px(left));
    }
}

fn despawn_pipe(pipe) {
    let top = get(pipe, "Pipe", "top_sprite");
    if top != 0 { despawn(top); }
    let bot = get(pipe, "Pipe", "bottom_sprite");
    if bot != 0 { despawn(bot); }
    despawn(pipe);
}

fn handle_restart(spawner) {
    for pipe in query("Pipe") {
        despawn_pipe(pipe);
    }
    set(spawner, "PipeSpawner", "last_spawn", 0.0);
}

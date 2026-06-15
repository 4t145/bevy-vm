// hud：score 文字 + game over 屏 + space 重启。

import "consts" as c;

let games = query("board::Game");
if games.len() == 0 { return; }
let game = games[0];

if get(game, "board::Game", "hud_built") == 0 {
    init_hud();
    set(game, "board::Game", "hud_built", 1);
}

let score = get(game, "board::Game", "score");
let state = get(game, "board::Game", "state");
let cake_eaten = get(game, "board::Game", "cake_eaten");

// 实时刷新 score 文字。
for txt in query("ScoreText") {
    set(txt, "Text", "", `Sugar Rush: ${score}`);
}

// game over：显示结算屏（一次性 spawn）+ 监听空格重启。
if state == 1 {
    if query("GameOverRoot").len() == 0 {
        spawn_game_over_screen(cake_eaten);
    }
    for ev in events("KeyboardInput") {
        if ev.state == "Pressed" && ev.key_code == "Space" {
            restart_game(game);
            return;
        }
    }
} else {
    // 切回 playing 时清掉 game over 屏（如果还在）。
    for root in query("GameOverRoot") {
        despawn(root);
    }
}

// ---- helpers ----

fn init_hud() {
    let root = spawn_entity();
    set(root, "Node", "", #{
        position_type: "Absolute",
        top: px(5.0),
        left: px(5.0),
    });

    let txt = spawn_entity();
    set(txt, "ScoreText", "", #{});
    set(txt, "Text", "", "Sugar Rush: 0");
    set(txt, "TextFont", "font_size", 33.0);
    set(txt, "TextColor", "", srgb(0.5, 0.5, 1.0));
    set_parent(txt, root);
}

fn spawn_game_over_screen(cake_eaten) {
    let root = spawn_entity();
    set(root, "GameOverRoot", "", #{});
    set(root, "Node", "", #{
        position_type: "Absolute",
        top: px(0.0),
        left: px(0.0),
        width: percent(100.0),
        height: percent(100.0),
        align_items: "Center",
        justify_content: "Center",
        flex_direction: "Column",
    });
    set(root, "BackgroundColor", "", srgba(0.0, 0.0, 0.0, 0.7));

    let big = spawn_entity();
    set(big, "GameOverText", "", #{});
    set(big, "Text", "", `Cake eaten: ${cake_eaten}`);
    set(big, "TextFont", "font_size", 67.0);
    set(big, "TextColor", "", srgb(0.5, 0.5, 1.0));
    set_parent(big, root);

    let hint = spawn_entity();
    set(hint, "Text", "", "Press Space to restart");
    set(hint, "TextFont", "font_size", 24.0);
    set(hint, "TextColor", "", srgb(0.85, 0.85, 0.95));
    set_parent(hint, root);
}

fn restart_game(game) {
    set(game, "board::Game", "score", 0);
    set(game, "board::Game", "cake_eaten", 0);
    set(game, "board::Game", "state", 0);
    set(game, "board::Game", "spawn_cd", c::spawn_interval());
    set(game, "board::Game", "move_cd", 0.0);

    // 玩家 reset 到中心格——让 player module 下一帧用单例 query 看到不存在
    // 就重建。这里直接 despawn 然后让 player.rhai 重新 spawn。
    for p in query("player::Player") {
        despawn(p);
    }
    for b in query("bonus::Bonus") {
        despawn(b);
    }
    for r in query("GameOverRoot") {
        despawn(r);
    }
}

// hud module 脚本：第一帧 spawn 文字节点；每帧刷新分数 + game over panel。

import "helpers" as h;

let game_q = query("game::Game");
let play_q = query("PlayArea");
if game_q.len() == 0 { return; }
let game = game_q[0];

// 第一次 spawn HUD：以 query("HudKind") 是否为空作为 once flag。
if query("HudKind").len() == 0 {
    if play_q.len() == 0 { return; }
    init_hud(play_q[0]);
}

refresh_hud(game);

// ---- HUD 构建 ------------------------------------------------------------

fn init_hud(play) {
    // 顶部分数：playfield 顶居中。
    let score = spawn_entity();
    set(score, "Node", "", #{
        position_type: "Absolute",
        top: px(20.0),
        left: px(0.0),
        width: px(h::play_width()),
        height: px(60.0),
        align_items: "Center",
        justify_content: "Center",
    });
    set(score, "HudKind", "kind", "score");
    set(score, "BackgroundColor", "", srgba(0.0, 0.0, 0.0, 0.0));
    set_parent(score, play);

    let score_text = spawn_entity();
    set(score_text, "Node", "", #{});
    set(score_text, "Text", "", "0");
    set(score_text, "TextFont", "font_size", h::hud_font_big());
    set(score_text, "TextColor", "", h::hud_text_color());
    set(score_text, "HudKind", "kind", "score_text");
    set_parent(score_text, score);

    // Game Over panel：playfield 居中，state==1 时改 display。
    let panel = spawn_entity();
    set(panel, "Node", "", #{
        position_type: "Absolute",
        top: px(h::play_height() * 0.5 - 100.0),
        left: px(h::play_width() * 0.5 - 140.0),
        width: px(280.0),
        height: px(200.0),
        display: "None",
        flex_direction: "Column",
        align_items: "Center",
        justify_content: "Center",
        row_gap: px(12.0),
        padding: #{
            left: px(20.0), right: px(20.0),
            top: px(20.0),  bottom: px(20.0),
        },
    });
    set(panel, "BackgroundColor", "", h::hud_panel_bg());
    set(panel, "HudKind", "kind", "go_panel");
    set_parent(panel, play);

    let title = spawn_entity();
    set(title, "Node", "", #{});
    set(title, "Text", "", "Game Over");
    set(title, "TextFont", "font_size", h::hud_font_big());
    set(title, "TextColor", "", h::hud_warn_color());
    set_parent(title, panel);

    let go_score = spawn_entity();
    set(go_score, "Node", "", #{});
    set(go_score, "Text", "", "Score: 0");
    set(go_score, "TextFont", "font_size", h::hud_font_medium());
    set(go_score, "TextColor", "", h::hud_text_color());
    set(go_score, "HudKind", "kind", "go_score");
    set_parent(go_score, panel);

    let hint = spawn_entity();
    set(hint, "Node", "", #{});
    set(hint, "Text", "", "Press Space or R to restart");
    set(hint, "TextFont", "font_size", h::hud_font_small());
    set(hint, "TextColor", "", h::hud_text_color());
    set_parent(hint, panel);
}

// ---- HUD 刷新 ------------------------------------------------------------

fn refresh_hud(game) {
    let score = get(game, "game::Game", "score");
    let state = get(game, "game::Game", "state");

    for hud in query("HudKind") {
        let kind = get(hud, "HudKind", "kind");
        if kind == "score_text" {
            set(hud, "Text", "", h::format_score(score));
        } else if kind == "go_score" {
            set(hud, "Text", "", `Score: ${score}`);
        } else if kind == "go_panel" {
            let display = if state == 1 { "Flex" } else { "None" };
            set(hud, "Node", "display", display);
        }
    }
}

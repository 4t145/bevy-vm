// ui plugin / hud system：HUD 树构建 + 状态刷新 + 难度按钮 hover 高亮。
//
// 此 system 排在 cell 后（after: ["cell"]）——这样 refresh_hud 看到的
// `state` / `flag_count` / `mines_placed` 都已是本帧 cell handler 处理后的
// 最新值，玩家点击的反馈同帧呈现。

import "helpers" as h;

let board_q = query("board::Board");
if board_q.len() == 0 {
    return;
}
let board = board_q[0];

// HUD 第一次 spawn（用 query("HudKind") 是否为空作为 once flag）。
let hud_built = query("HudKind").len() > 0;
if !hud_built {
    init_hud();
}
refresh_hud(board);

// ---- HUD 构建 ------------------------------------------------------------

fn init_hud() {
    let root = spawn_entity();
    set(root, "Node", "", #{
        width: h::percent(100.0),
        height: h::percent(100.0),
        display: "Flex",
        flex_direction: "Column",
        align_items: "Stretch",
        justify_content: "FlexStart",
    });
    set(root, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));

    let bar = spawn_entity();
    set(bar, "Node", "", #{
        width: h::percent(100.0),
        height: h::px(40.0),
        display: "Flex",
        flex_direction: "Row",
        align_items: "Center",
        justify_content: "Center",
        column_gap: h::px(16.0),
        padding: #{
            left: h::px(12.0), right: h::px(12.0),
            top: h::px(6.0),  bottom: h::px(6.0),
        },
    });
    set(bar, "BackgroundColor", "", h::hud_bg_color());
    set_parent(bar, root);

    spawn_difficulty_button(bar, "Easy");
    spawn_difficulty_button(bar, "Med");
    spawn_difficulty_button(bar, "Hard");
    spawn_hud_text(bar, "timer", "00:00");
    spawn_hud_text(bar, "mines", "Mines: 10");
    spawn_hud_text(bar, "phase", "Ready");
}

fn spawn_difficulty_button(parent, which) {
    let btn = spawn_entity();
    set(btn, "Node", "", #{
        padding: #{
            left: h::px(10.0), right: h::px(10.0),
            top: h::px(4.0),   bottom: h::px(4.0),
        },
        align_items: "Center",
        justify_content: "Center",
    });
    set(btn, "Button", "", #{});
    set(btn, "BackgroundColor", "", h::button_color());
    set(btn, "Pickable", "", #{});
    set(btn, "DifficultyButton", "which", which);
    set_parent(btn, parent);

    let label = spawn_entity();
    set(label, "Node", "", #{});
    set(label, "Text", "", which);
    set(label, "TextFont", "font_size", h::hud_font_size());
    set(label, "TextColor", "", h::hud_text_color());
    set_parent(label, btn);
}

fn spawn_hud_text(parent, kind, content) {
    let e = spawn_entity();
    set(e, "Node", "", #{});
    set(e, "Text", "", content);
    set(e, "TextFont", "font_size", h::hud_font_size());
    set(e, "TextColor", "", h::hud_text_color());
    set(e, "HudKind", "kind", kind);
    set_parent(e, parent);
}

// ---- HUD 刷新 -----------------------------------------------------------

fn refresh_hud(board) {
    // 计时
    let elapsed = 0.0;
    let timer_q = query("timer::GameTimer");
    if timer_q.len() > 0 {
        elapsed = get(timer_q[0], "timer::GameTimer", "elapsed");
    }

    // 雷数
    let mine_count = get(board, "board::Board", "mine_count");
    let flag_count = 0;
    for tile in query("cell::Tile") {
        if get(tile, "cell::Tile", "flag") == 1 { flag_count += 1; }
    }
    let remaining = mine_count - flag_count;

    // 阶段
    let state = get(board, "board::Board", "state");
    let mines_placed = get(board, "board::Board", "mines_placed");
    let phase = if state == 1 { "Win!" }
        else if state == 2 { "Boom!" }
        else if mines_placed == 0 { "Ready" }
        else { "Playing" };

    for hud in query("HudKind") {
        let kind = get(hud, "HudKind", "kind");
        if kind == "timer" {
            set(hud, "Text", "", h::format_mmss(elapsed));
        } else if kind == "mines" {
            set(hud, "Text", "", `Mines: ${remaining}`);
        } else if kind == "phase" {
            set(hud, "Text", "", phase);
        }
    }

    // 难度按钮 hover 高亮（Hovered 来自 anim plugin，跨 plugin 走全限定）。
    for btn in query("DifficultyButton") {
        let bg = if get(btn, "anim::Hovered", "value") == 1 {
            h::button_hover_color()
        } else {
            h::button_color()
        };
        set(btn, "BackgroundColor", "", bg);
    }
}

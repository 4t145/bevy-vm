// hud module 脚本：第一帧 spawn 文字节点，每帧刷新内容。
//
// HUD 共 4 格：score / lines / level / phase；phase 在 game over / paused 时
// 用警示色，正常游戏中显示等级或剩余行数。

import "helpers" as h;

let board_q = query("board::Board");
let score_q = query("piece::Score");
if board_q.len() == 0 || score_q.len() == 0 { return; }
let score = score_q[0];

// HUD 第一次 spawn——以 query("HudKind") 是否为空作为 once flag。
let hud_built = query("HudKind").len() > 0;
if !hud_built {
    let slot_q = query("board::HudSlot");
    if slot_q.len() > 0 {
        init_hud(slot_q[0]);
    }
}
refresh_hud(score);

// ---- HUD 构建 ------------------------------------------------------------

fn init_hud(slot) {
    spawn_hud_text(slot, "score", "Score: 0");
    spawn_hud_text(slot, "lines", "Lines: 0");
    spawn_hud_text(slot, "level", "Lv 1");
    spawn_hud_text(slot, "phase", "Ready");
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

// ---- HUD 刷新 ------------------------------------------------------------

fn refresh_hud(score) {
    let s = get(score, "piece::Score", "score");
    let l = get(score, "piece::Score", "lines");
    let lv = get(score, "piece::Score", "level");
    let state = get(score, "piece::Score", "state");

    let phase = if state == 1 { "Game Over - R to restart" } else { "Playing" };
    let phase_color = if state == 1 { h::hud_warn_color() } else { h::hud_text_color() };

    for hud in query("HudKind") {
        let kind = get(hud, "HudKind", "kind");
        if kind == "score" {
            set(hud, "Text", "", `Score: ${s}`);
        } else if kind == "lines" {
            set(hud, "Text", "", `Lines: ${l}`);
        } else if kind == "level" {
            set(hud, "Text", "", `Lv ${lv}`);
        } else if kind == "phase" {
            set(hud, "Text", "", phase);
            set(hud, "TextColor", "", phase_color);
        }
    }
}

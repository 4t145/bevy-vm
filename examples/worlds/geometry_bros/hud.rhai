// hud plugin：状态栏 HUD（HP bar、XP bar、等级、时间、击杀数）。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];

let pl_q = query("level::PlayerLevel");
if pl_q.len() == 0 { return; }
let pl = pl_q[0];

// 第一帧 spawn 整个 HUD 树。
if query("HudElement").len() == 0 {
    init_hud();
}
refresh_hud(player, pl);

fn init_hud() {
    // root：fill 全屏，子节点对齐到顶部。
    let root = spawn_entity();
    set(root, "Node", "", #{
        width: h::percent(100.0),
        height: h::percent(100.0),
        display: "Flex",
        flex_direction: "Column",
        align_items: "Stretch",
        justify_content: "FlexStart",
        padding: #{
            left: h::px(12.0), right: h::px(12.0),
            top: h::px(8.0), bottom: h::px(8.0),
        },
    });
    set(root, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));

    // 顶部状态栏 row：HP bar + XP bar + 文字
    let bar_row = spawn_entity();
    set(bar_row, "Node", "", #{
        display: "Flex",
        flex_direction: "Row",
        align_items: "Center",
        column_gap: h::px(16.0),
    });
    set(bar_row, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));
    set_parent(bar_row, root);

    // HP bar（背景 + fill 子节点）
    spawn_bar(bar_row, "hp_fill", h::color_hp_bar_fill(), 200.0);
    spawn_text(bar_row, "hp_text", "HP 100/100");
    spawn_bar(bar_row, "xp_fill", h::color_xp_bar_fill(), 200.0);
    spawn_text(bar_row, "xp_text", "XP 0/10");
    spawn_text(bar_row, "lv_text", "Lv 1");
    spawn_text(bar_row, "wave_text", "W1 20.0s");
    spawn_text(bar_row, "time_text", "00:00");
    spawn_text(bar_row, "kill_text", "Kills 0");
}

fn spawn_bar(parent, kind, color, width_px) {
    let bg = spawn_entity();
    set(bg, "Node", "", #{
        width: h::px(width_px),
        height: h::px(16.0),
        display: "Flex",
        flex_direction: "Row",
        align_items: "Stretch",
    });
    set(bg, "BackgroundColor", "", h::color_hp_bar_bg());
    set_parent(bg, parent);

    let fill = spawn_entity();
    set(fill, "Node", "", #{
        width: h::percent(100.0),
        height: h::percent(100.0),
    });
    set(fill, "BackgroundColor", "", color);
    set(fill, "HudElement", "kind", kind);
    set_parent(fill, bg);
}

fn spawn_text(parent, kind, content) {
    let e = spawn_entity();
    set(e, "Node", "", #{});
    set(e, "Text", "", content);
    set(e, "TextFont", "font_size", 18.0);
    set(e, "TextColor", "", h::color_hud_text());
    set(e, "HudElement", "kind", kind);
    set_parent(e, parent);
}

fn refresh_hud(player, pl) {
    let hp = get(player, "player::Player", "hp");
    let max_hp = get(player, "player::Player", "max_hp");
    let kills = get(player, "player::Player", "kills");
    let survived = get(player, "player::Player", "survived");
    let lv = get(pl, "level::PlayerLevel", "level");
    let xp = get(pl, "level::PlayerLevel", "current_xp");
    let xp_max = get(pl, "level::PlayerLevel", "next_threshold");

    // 波次状态——可能尚未注册（极早期帧），缺省值。
    let wave_n = 1;
    let wave_remaining = 0.0;
    let waves = query("wave::WaveState");
    if waves.len() > 0 {
        wave_n = get(waves[0], "wave::WaveState", "number");
        wave_remaining = get(waves[0], "wave::WaveState", "remaining");
    }

    let hp_pct = if max_hp > 0 { (hp.to_float() * 100.0 / max_hp.to_float()) } else { 0.0 };
    if hp_pct < 0.0 { hp_pct = 0.0; }
    let xp_pct = if xp_max > 0 { (xp.to_float() * 100.0 / xp_max.to_float()) } else { 0.0 };
    if xp_pct > 100.0 { xp_pct = 100.0; }

    for el in query("HudElement") {
        let kind = get(el, "HudElement", "kind");
        if kind == "hp_text" {
            set(el, "Text", "", `HP ${hp}/${max_hp}`);
        } else if kind == "xp_text" {
            set(el, "Text", "", `XP ${xp}/${xp_max}`);
        } else if kind == "lv_text" {
            set(el, "Text", "", `Lv ${lv}`);
        } else if kind == "wave_text" {
            // 倒计时显示一位小数，便于看到读秒。
            let s = (wave_remaining * 10.0).to_int().to_float() / 10.0;
            set(el, "Text", "", `W${wave_n} ${s}s`);
        } else if kind == "time_text" {
            set(el, "Text", "", h::format_mmss(survived));
        } else if kind == "kill_text" {
            set(el, "Text", "", `Kills ${kills}`);
        } else if kind == "hp_fill" {
            set(el, "Node", "width", h::percent(hp_pct));
        } else if kind == "xp_fill" {
            set(el, "Node", "width", h::percent(xp_pct));
        }
    }
}

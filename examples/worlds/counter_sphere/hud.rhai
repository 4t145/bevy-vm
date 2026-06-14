// hud：左下角 HP / Kills / Targets 文本。第一帧 spawn HUD 树，后续 refresh。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let player = players[0];

if query("HudKind").len() == 0 {
    init_hud();
}
refresh_hud(player);

fn init_hud() {
    let root = spawn_entity();
    set(root, "Node", "", #{
        width: h::percent(100.0),
        height: h::percent(100.0),
        display: "Flex",
        flex_direction: "Column",
        align_items: "FlexStart",
        justify_content: "FlexEnd",   // 沉到底部
        padding: #{
            left: h::px(12.0), right: h::px(12.0),
            top: h::px(12.0), bottom: h::px(12.0),
        },
    });
    set(root, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));

    let row = spawn_entity();
    set(row, "Node", "", #{
        display: "Flex",
        flex_direction: "Row",
        column_gap: h::px(16.0),
    });
    set(row, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));
    set_parent(row, root);

    spawn_text(row, "hp_text", "HP --");
    spawn_text(row, "kill_text", "Kills 0");
    spawn_text(row, "left_text", "Targets --");
}

fn spawn_text(parent, kind, content) {
    let e = spawn_entity();
    set(e, "Node", "", #{});
    set(e, "Text", "", content);
    set(e, "TextFont", "font_size", 18.0);
    set(e, "TextColor", "", h::color_hud_text());
    set(e, "HudKind", "kind", kind);
    set_parent(e, parent);
}

fn refresh_hud(player) {
    let hp = get(player, "player::Player", "hp");
    let max_hp = get(player, "player::Player", "max_hp");
    let kills = get(player, "player::Player", "kills");
    let alive = query("enemy::Enemy").len();

    for el in query("HudKind") {
        let kind = get(el, "HudKind", "kind");
        if      kind == "hp_text"   { set(el, "Text", "", `HP ${hp}/${max_hp}`); }
        else if kind == "kill_text" { set(el, "Text", "", `Kills ${kills}`); }
        else if kind == "left_text" { set(el, "Text", "", `Targets ${alive}`); }
    }
}

// HUD 顶部状态栏：每帧刷新文字。
//
// 第一次见到 GameRoot 时挂 status bar 子节点；之后每帧只 update text。

let game = query_first("board::Game");
if game == () { return; }

let built = query("ScrewText").len() > 0;
if !built {
    let roots = query("GameRoot");
    if roots.len() == 0 { return; }
    init_hud(roots[0]);
}

let s_total = get(game, "board::Game", "screws_total");
let s_done = get(game, "board::Game", "screws_collected");
let keys = get(game, "board::Game", "keys");
let ammo = get(game, "board::Game", "ammo");
let state = get(game, "board::Game", "state");
let lvl = get(game, "board::Game", "level_idx") + 1;   // 1-based 显示

let levels = load_config("levels.json");
let total_levels = config_get(levels, "count");

let phase = if state == 3 { "ALL CLEAR" }
    else if state == 1 { "Level Clear!" }
    else if state == 2 { "DEAD — press R" }
    else if s_done >= s_total { "Reach the capsule!" }
    else { "Playing" };

for t in query("LevelText") { set(t, "Text", "", `Level ${lvl}/${total_levels}`); }
for t in query("ScrewText") { set(t, "Text", "", `Screws ${s_done}/${s_total}`); }
for t in query("KeyText")   { set(t, "Text", "", `Keys ${keys}`); }
for t in query("AmmoText")  { set(t, "Text", "", `Ammo ${ammo}`); }
for t in query("PhaseText") {
    set(t, "Text", "", phase);
    let color = if state == 3 { srgba(0.40, 0.85, 0.95, 1.0) }
        else if state == 1 { srgba(0.30, 0.95, 0.40, 1.0) }
        else if state == 2 { srgba(0.95, 0.30, 0.30, 1.0) }
        else if s_done >= s_total { srgba(0.95, 0.85, 0.30, 1.0) }
        else { srgba(0.95, 0.92, 0.40, 1.0) };
    set(t, "TextColor", "", color);
}

fn init_hud(root) {
    let bar = spawn_entity();
    set(bar, "Node", "", #{
        position_type: "Absolute",
        top: px(0.0),
        left: px(0.0),
        width: percent(100.0),
        height: px(36.0),
        display: "Flex",
        flex_direction: "Row",
        align_items: "Center",
        justify_content: "Center",
        column_gap: px(24.0),
    });
    set(bar, "BackgroundColor", "", srgba(0.10, 0.10, 0.12, 0.85));
    set_parent(bar, root);

    spawn_label(bar, "LevelText", "Level 1");
    spawn_label(bar, "ScrewText", "Screws 0/0");
    spawn_label(bar, "KeyText",   "Keys 0");
    spawn_label(bar, "AmmoText",  "Ammo 0");
    spawn_label(bar, "PhaseText", "Playing");
}

fn spawn_label(parent, marker, init) {
    let e = spawn_entity();
    set(e, marker, "", #{});
    set(e, "Text", "", init);
    set(e, "TextFont", "font_size", 18.0);
    set(e, "TextColor", "", srgba(0.95, 0.92, 0.40, 1.0));
    set_parent(e, parent);
}

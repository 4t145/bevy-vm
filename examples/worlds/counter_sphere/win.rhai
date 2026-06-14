// win：玩家击杀达到 5 → 弹 VICTORY 覆盖；已存在则跳过。

import "helpers" as h;

let players = query("player::Player");
if players.len() == 0 { return; }
let kills = get(players[0], "player::Player", "kills");
if kills < 5 { return; }
if query("WinScreen").len() > 0 { return; }

let root = spawn_entity();
set(root, "WinScreen", "", #{});
set(root, "Node", "", #{
    width: h::percent(100.0),
    height: h::percent(100.0),
    display: "Flex",
    flex_direction: "Column",
    align_items: "Center",
    justify_content: "Center",
    row_gap: h::px(16.0),
});
set(root, "BackgroundColor", "", h::color_win_bg());

let title = spawn_entity();
set(title, "Node", "", #{});
set(title, "Text", "", "VICTORY");
set(title, "TextFont", "font_size", 64.0);
set(title, "TextColor", "", h::srgba(0.85, 1.0, 0.85, 1.0));
set_parent(title, root);

let hint = spawn_entity();
set(hint, "Node", "", #{});
set(hint, "Text", "", "Press R to restart");
set(hint, "TextFont", "font_size", 22.0);
set(hint, "TextColor", "", h::color_hud_text());
set_parent(hint, root);

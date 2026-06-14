// death_show：监听 PlayerDied，弹结算画面（survived + kills + level）。

import "helpers" as h;

let triggered = false;
for _ev in events("combat::PlayerDied") {
    triggered = true;
}
if !triggered { return; }
if query("DeathScreen").len() > 0 { return; }   // 已显示则跳过

let players = query("player::Player");
let pl_q = query("level::PlayerLevel");
if players.len() == 0 || pl_q.len() == 0 { return; }
let player = players[0];
let pl = pl_q[0];

let survived = get(player, "player::Player", "survived");
let kills = get(player, "player::Player", "kills");
let lv = get(pl, "level::PlayerLevel", "level");

let root = spawn_entity();
set(root, "DeathScreen", "", #{});
set(root, "Node", "", #{
    width: h::percent(100.0),
    height: h::percent(100.0),
    display: "Flex",
    flex_direction: "Column",
    align_items: "Center",
    justify_content: "Center",
    row_gap: h::px(20.0),
});
set(root, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.85));

let title = spawn_entity();
set(title, "Node", "", #{});
set(title, "Text", "", "GAME OVER");
set(title, "TextFont", "font_size", 48.0);
set(title, "TextColor", "", h::srgba(0.95, 0.30, 0.30, 1.0));
set_parent(title, root);

let stats = spawn_entity();
set(stats, "Node", "", #{});
set(stats, "Text", "", `Survived ${h::format_mmss(survived)}  ·  Kills ${kills}  ·  Lv ${lv}`);
set(stats, "TextFont", "font_size", 24.0);
set(stats, "TextColor", "", h::color_hud_text());
set_parent(stats, root);

let hint = spawn_entity();
set(hint, "Node", "", #{});
set(hint, "Text", "", "Press R to restart");
set(hint, "TextFont", "font_size", 18.0);
set(hint, "TextColor", "", h::color_hud_text());
set_parent(hint, root);

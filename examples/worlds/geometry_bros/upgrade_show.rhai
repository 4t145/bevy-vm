// upgrade_show：监听 wave::WaveCompleted，弹出加强菜单。
//
// 菜单 3 张卡——从池子里随机抽。回血是 wave_clear 的无条件附赠（满血），
// 不占菜单卡位。
// 池子：max_hp / speed / fire_rate / damage / regen。
//
// 同一波只显示一次——已有 UpgradeRoot 跳过。

import "helpers" as h;

let triggered = false;
for _ev in events("wave::WaveCompleted") {
    triggered = true;
}
if !triggered { return; }
if query("UpgradeRoot").len() > 0 { return; }

// 抽 3——Fisher-Yates 简化版。
let pool = ["max_hp", "speed", "fire_rate", "damage", "regen"];
let n = pool.len();
for i in 0..n {
    let j = random_int(i, n);
    if j != i {
        let tmp = pool[i];
        pool[i] = pool[j];
        pool[j] = tmp;
    }
}
let pick0 = pool[0];
let pick1 = pool[1];
let pick2 = pool[2];

let root = spawn_entity();
set(root, "UpgradeRoot", "", #{});
set(root, "Node", "", #{
    width: h::percent(100.0),
    height: h::percent(100.0),
    display: "Flex",
    flex_direction: "Column",
    align_items: "Center",
    justify_content: "Center",
    row_gap: h::px(20.0),
});
set(root, "BackgroundColor", "", h::color_panel_bg());

let title = spawn_entity();
set(title, "Node", "", #{});
set(title, "Text", "", "Wave Cleared! Pick a reward:");
set(title, "TextFont", "font_size", 32.0);
set(title, "TextColor", "", h::color_hud_text());
set_parent(title, root);

let subtitle = spawn_entity();
set(subtitle, "Node", "", #{});
set(subtitle, "Text", "", "(HP fully restored)");
set(subtitle, "TextFont", "font_size", 16.0);
set(subtitle, "TextColor", "", h::color_hud_text());
set_parent(subtitle, root);

let cards = spawn_entity();
set(cards, "Node", "", #{
    display: "Flex",
    flex_direction: "Row",
    column_gap: h::px(20.0),
});
set(cards, "BackgroundColor", "", h::srgba(0.0,0.0,0.0,0.0));
set_parent(cards, root);

spawn_card(cards, pick0, label_for(pick0), desc_for(pick0));
spawn_card(cards, pick1, label_for(pick1), desc_for(pick1));
spawn_card(cards, pick2, label_for(pick2), desc_for(pick2));

fn label_for(kind) {
    if      kind == "max_hp"    { "Max HP +20" }
    else if kind == "speed"     { "Move Speed +20%" }
    else if kind == "fire_rate" { "Fire Rate +25%" }
    else if kind == "damage"    { "Damage +30%" }
    else if kind == "regen"     { "Regen +1 HP/s" }
    else                        { kind }
}

fn desc_for(kind) {
    if      kind == "max_hp"    { "Raise the ceiling" }
    else if kind == "speed"     { "Run faster" }
    else if kind == "fire_rate" { "Shoot more often" }
    else if kind == "damage"    { "Hit harder" }
    else if kind == "regen"     { "Slowly regenerate" }
    else                        { "" }
}

fn spawn_card(parent, kind, title, desc) {
    let card = spawn_entity();
    set(card, "Node", "", #{
        width: h::px(180.0),
        height: h::px(160.0),
        display: "Flex",
        flex_direction: "Column",
        align_items: "Center",
        justify_content: "Center",
        row_gap: h::px(8.0),
        padding: #{
            left: h::px(12.0), right: h::px(12.0),
            top: h::px(12.0),  bottom: h::px(12.0),
        },
    });
    set(card, "Button", "", #{});
    attach_pickable(card);
    set(card, "BackgroundColor", "", h::color_card_bg());
    set(card, "UpgradeCard", "kind", kind);
    set_parent(card, parent);

    let t = spawn_entity();
    set(t, "Node", "", #{});
    set(t, "Text", "", title);
    set(t, "TextFont", "font_size", 18.0);
    set(t, "TextColor", "", h::color_hud_text());
    set_parent(t, card);

    let d = spawn_entity();
    set(d, "Node", "", #{});
    set(d, "Text", "", desc);
    set(d, "TextFont", "font_size", 14.0);
    set(d, "TextColor", "", h::color_hud_text());
    set_parent(d, card);
}

// anim module 脚本：维护 Hovered + 每帧刷新 tile 视觉。
//
// 两步：
// 1. 消费 PickOver / PickOut（host PickingPlugin 镜像到 Bevy Messages<...>），
//    写 Hovered.value 到对应实体。可能命中非 tile 实体——也写无害，下面的
//    视觉刷新跳过它们。
// 2. 扫所有 tile，根据 cell::Tile 状态 + Hovered 写 cell BG + label Text。

import "helpers" as h;

// ---- step 1: 维护 Hovered ----

for ev in events("PickOver") {
    set(ev.entity, "Hovered", "value", 1);
}
for ev in events("PickOut") {
    set(ev.entity, "Hovered", "value", 0);
}

// ---- step 2: 刷新 tile 视觉 ----

for tile in query("cell::Tile") {
    update_appearance(tile);
}

fn update_appearance(cell) {
    let revealed = get(cell, "cell::Tile", "revealed");
    let flag = get(cell, "cell::Tile", "flag");
    let mine = get(cell, "cell::Tile", "mine");
    let adj = get(cell, "cell::Tile", "adj");
    let hovered = get(cell, "Hovered", "value") == 1;
    let label = get(cell, "cell::Tile", "label");

    if revealed == 1 {
        // 已揭开——不响应 hover。
        if mine == 1 {
            set(cell, "BackgroundColor", "", h::color_mine());
            set(label, "Text", "", "X");
            set(label, "TextColor", "", h::flag_text_color());
        } else {
            set(cell, "BackgroundColor", "", h::color_revealed());
            if adj > 0 {
                set(label, "Text", "", adj.to_string());
                set(label, "TextColor", "", h::number_color(adj));
            } else {
                set(label, "Text", "", "");
            }
        }
    } else if flag == 1 {
        let bg = if hovered { h::color_flag_hover() } else { h::color_flag() };
        set(cell, "BackgroundColor", "", bg);
        set(label, "Text", "", "F");
        set(label, "TextColor", "", h::flag_text_color());
    } else {
        let bg = if hovered { h::color_hidden_hover() } else { h::color_hidden() };
        set(cell, "BackgroundColor", "", bg);
        set(label, "Text", "", "");
    }
}

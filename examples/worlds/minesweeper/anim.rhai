// anim module 脚本：维护 Hovered + 每帧刷新 tile 视觉。
//
// 两步：
// 1. 消费 host 端 PickOver / PickOut 事件，写 Hovered.value 到对应实体。
//    可能命中非 tile 实体（按钮等）——也写无害，下面的视觉刷新跳过它们。
// 2. 扫所有 tile，根据 cell::Tile 状态 + Hovered 写 sprite/text。

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

fn update_appearance(e) {
    let revealed = get(e, "cell::Tile", "revealed");
    let flag = get(e, "cell::Tile", "flag");
    let mine = get(e, "cell::Tile", "mine");
    let adj = get(e, "cell::Tile", "adj");
    let hovered = get(e, "Hovered", "value") == 1;

    if revealed == 1 {
        // 已揭开——不响应 hover。
        if mine == 1 {
            set_sprite_color(e, h::color_mine());
            set_text_content(e, "X");
            set_text_color(e, h::flag_text_color());
        } else {
            set_sprite_color(e, h::color_revealed());
            if adj > 0 {
                set_text_content(e, adj.to_string());
                set_text_color(e, h::number_color(adj));
            } else {
                set_text_content(e, "");
            }
        }
    } else if flag == 1 {
        let bg = if hovered { h::color_flag_hover() } else { h::color_flag() };
        set_sprite_color(e, bg);
        set_text_content(e, "F");
        set_text_color(e, h::flag_text_color());
    } else {
        let bg = if hovered { h::color_hidden_hover() } else { h::color_hidden() };
        set_sprite_color(e, bg);
        set_text_content(e, "");
    }
}

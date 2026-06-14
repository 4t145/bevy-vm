// anim plugin 脚本：维护 Hovered + 每帧刷新 tile 视觉。
//
// 两步：
// 1. 消费 host 端 PickOver / PickOut 事件，写 Hovered.value 到对应实体。
//    注意：这些事件可能命中非 tile 实体（按钮等）——也写没关系，那些实体
//    没有 cell::Tile 组件，下面的视觉刷新跳过它们。
// 2. 扫所有 tile，根据 cell::Tile 状态 + Hovered 写 Sprite2d.color/TextLabel。
//    每帧全量刷新——比起"只刷状态变化的 tile"复杂度低、bug 少；棋盘 tile
//    数量有限（最多几百），开销可忽略。

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
            set(e, "Sprite2d", "color", h::color_mine());
            set(e, "TextLabel", "content", "X");
            set(e, "TextLabel", "color", h::flag_text_color());
        } else {
            set(e, "Sprite2d", "color", h::color_revealed());
            if adj > 0 {
                set(e, "TextLabel", "content", adj.to_string());
                set(e, "TextLabel", "color", h::number_color(adj));
            } else {
                set(e, "TextLabel", "content", "");
            }
        }
    } else if flag == 1 {
        let bg = if hovered { h::color_flag_hover() } else { h::color_flag() };
        set(e, "Sprite2d", "color", bg);
        set(e, "TextLabel", "content", "F");
        set(e, "TextLabel", "color", h::flag_text_color());
    } else {
        let bg = if hovered { h::color_hidden_hover() } else { h::color_hidden() };
        set(e, "Sprite2d", "color", bg);
        set(e, "TextLabel", "content", "");
    }
}

// upgrade_hover：维护卡片 hover 状态 + 每帧重绘卡片背景。
//
// PickOver / PickOut 走 host PickingPlugin 镜像到 Bevy `Messages<...>`，VM
// cursor 每帧从 0 起读未消费过的部分。把命中的 entity 标记 Hovered.value=1，
// 离开标 0。然后扫所有 UpgradeCard 根据 Hovered 切背景色。
//
// 没面板时（query("UpgradeCard").is_empty()）跳过——避免空集刷新浪费。

import "helpers" as h;

if query("UpgradeCard").len() == 0 { return; }

for ev in events("PickOver") {
    set(ev.entity, "Hovered", "value", 1);
}
for ev in events("PickOut") {
    set(ev.entity, "Hovered", "value", 0);
}

for card in query("UpgradeCard") {
    let bg = if get(card, "Hovered", "value") == 1 {
        h::color_card_hover()
    } else {
        h::color_card_bg()
    };
    set(card, "BackgroundColor", "", bg);
}

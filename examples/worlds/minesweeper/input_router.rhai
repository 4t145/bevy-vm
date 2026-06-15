// ui plugin / input_router system：把本帧 PickClick 翻译成领域事件。
//
// 此 system 排在 cell 前（before: ["cell"]）——dynamic 事件同帧消费让
// 新生的 cell::* 事件本帧就被 cell handler 看到，零延迟。

for ev in events("PickClick") {
    let which = get(ev.entity, "DifficultyButton", "which");
    if which != "" {
        if ev.button == "Primary" {
            emit("difficulty::ChangeDifficulty", #{ which: which });
        }
        continue;
    }
    // 没挂 cell::Tile 也不是按钮——忽略（HUD 文本之类）。
    if !has_component(ev.entity, "cell::Tile") {
        continue;
    }
    if ev.button == "Primary" {
        emit("cell::RevealCell", #{ entity: ev.entity });
    } else if ev.button == "Secondary" {
        if get(ev.entity, "cell::Tile", "revealed") == 1
            && get(ev.entity, "cell::Tile", "adj") > 0 {
            emit("cell::ChordCell", #{ entity: ev.entity });
        } else {
            emit("cell::FlagCell", #{ entity: ev.entity });
        }
    }
}

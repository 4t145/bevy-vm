// cursor：管理鼠标 grab 状态。
//
// - 第一帧 emit CursorGrab 让玩家立刻进入 FPS 视角。
// - Esc 按下 → emit CursorRelease（玩家可点 viewer 顶栏切换 world，或重新点击窗口
//   通过下方"游戏区点击"再次 grab——这部分由触发逻辑决定，本 demo 仅做最简版）。
//
// 通过 CursorState.grabbed 单例避免每帧重复 emit。
// CursorGrab / CursorRelease 是 bevy_vm::plugin::cursor 注册的 typed VM→host 事件。

let states = query("CursorState");
if states.len() == 0 { return; }
let state = states[0];

let grabbed = get(state, "CursorState", "grabbed");

// 第一帧（grabbed=0）→ 请求 grab。
if grabbed == 0 {
    emit("CursorGrab", #{});
    set(state, "CursorState", "grabbed", 1);
}

// Esc → 释放。注意 grabbed 不重置回 0（避免下一帧又自动 grab）。
// 玩家想重新进入 FPS：按 R 重启 world，或者继续游玩 / 切到别的 world。
for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    if ev.state == "Pressed" && ev.key_code == "Escape" {
        emit("CursorRelease", #{});
        return;
    }
}

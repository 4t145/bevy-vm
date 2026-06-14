// cursor：管理鼠标 grab 状态。
//
// - 第一帧调 set_cursor_grab(true) 进入 FPS 视角。
// - Esc 按下 → set_cursor_grab(false) 释放。
//
// CursorState.grabbed 单例避免每帧重复 grab。

let states = query("CursorState");
if states.len() == 0 { return; }
let state = states[0];

let grabbed = get(state, "CursorState", "grabbed");

if grabbed == 0 {
    set_cursor_grab(true);
    set(state, "CursorState", "grabbed", 1);
}

// Esc → 释放。grabbed 不重置（避免下一帧再次 grab）。
// 玩家想重新进入 FPS：按 R 重启 world，或继续游玩 / 切到别的 world。
for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    if ev.state == "Pressed" && ev.key_code == "Escape" {
        set_cursor_grab(false);
        return;
    }
}

// Drag-by-mouse: read MouseButton + CursorMoved events, mutate Position.
//
// CursorMoved (rather than MouseMotion) is the cross-platform way to track
// the cursor: it fires whenever the cursor position changes inside the window,
// and carries an optional `delta` since the previous event.
//
// SENSITIVITY: 屏幕像素 -> 世界单位的缩放系数。
const SENSITIVITY = 0.01;

// 处理按下/松开。MouseButton.state 是 enum，serde 序列化为字符串。
let btn_count = events("MouseButton").len();
for ev in events("MouseButton") {
    if ev.button == "Left" {
        let next = if ev.state == "Pressed" { 1 } else { 0 };
        for e in query("DragState") {
            set(e, "DragState", "active", next);
        }
    }
}

let cursor_count = events("CursorMoved").len();

// 累加移动。仅在 active 时生效。
for e in query("DragState") {
    let active = get(e, "DragState", "active");
    if active == 1 {
        for ev in events("CursorMoved") {
            // delta 是 Option<Vec2>：第一次按下后第一帧 delta 为 null，跳过。
            // Vec2 序列化为 [x, y] 数组。
            if ev.delta != () {
                let dx = ev.delta[0] * SENSITIVITY;
                let dy = ev.delta[1] * SENSITIVITY;
                // 屏幕 y 向下为正；世界 y 向上为正——取负让方块跟着指针。
                set(e, "Position", "x", get(e, "Position", "x") + dx);
                set(e, "Position", "y", get(e, "Position", "y") - dy);
            }
        }
    }
}

// drag —— 鼠标按住左键拖动立方体。
//
// CursorMoved.delta 累加到 DragState.x/y；每帧 set_translation 写到 Bevy
// Transform。屏幕 y 向下为正，反向写以便方块跟着鼠标。

const SENSITIVITY = 0.01;

for ev in events("MouseButton") {
    if ev.button == "Left" {
        let next = if ev.state == "Pressed" { 1 } else { 0 };
        for e in query("DragState") {
            set(e, "DragState", "active", next);
        }
    }
}

for e in query("DragState") {
    let active = get(e, "DragState", "active");
    if active == 1 {
        for ev in events("CursorMoved") {
            if ev.delta != () {
                let dx = ev.delta[0] * SENSITIVITY;
                let dy = ev.delta[1] * SENSITIVITY;
                set(e, "DragState", "x", get(e, "DragState", "x") + dx);
                set(e, "DragState", "y", get(e, "DragState", "y") - dy);
            }
        }
    }
    set_translation(e, [
        get(e, "DragState", "x"),
        get(e, "DragState", "y"),
        0.0,
    ]);
}

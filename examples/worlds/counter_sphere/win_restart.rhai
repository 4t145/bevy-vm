// win_restart：胜利画面下按 R 键 → emit RestartGame。

if query("WinScreen").len() == 0 { return; }

for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    if ev.state == "Pressed" && ev.key_code == "KeyR" {
        emit("RestartGame", #{});
        return;
    }
}

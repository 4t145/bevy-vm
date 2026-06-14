// death_restart：仅在死亡画面显示时听 R 键 → emit RestartGame。
// 不在死亡画面里时不响应（避免误触）。

if query("DeathScreen").len() == 0 { return; }

for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    if ev.state == "Pressed" && ev.key_code == "KeyR" {
        emit("RestartGame", #{});
        return;
    }
}

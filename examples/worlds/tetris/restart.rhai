// restart plugin 脚本：R 键按下（非 repeat）→ emit piece::RestartGame。

for ev in events("KeyboardInput") {
    if ev.key_code == "KeyR" && ev.state == "Pressed" && !ev.repeat {
        emit("piece::RestartGame", #{});
    }
}

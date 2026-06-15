// restart plugin 脚本：仅在 dead 状态吃 Space / R → emit bird::RestartGame。

let game_q = query("game::Game");
if game_q.len() == 0 { return; }
let game = game_q[0];

if get(game, "game::Game", "state") != 1 { return; }

for ev in events("KeyboardInput") {
    if ev.repeat { continue; }
    if ev.state != "Pressed" { continue; }
    let key = ev.key_code;
    if key == "Space" || key == "KeyR" {
        emit("bird::RestartGame", #{});
        return;
    }
}

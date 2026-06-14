// wave_advance：监听 pause::UpgradePicked → 进入下一波。
//
// 玩家在加强菜单选完属性 → upgrade_pick emit pause::UpgradePicked。
// pause plugin 同事件 resume()；本 system 同事件递增 wave 编号 + 重置
// 倒计时 + emit WaveStarted（让其他 plugin 想响应"新波次开始"有钩子）。

let triggered = false;
for _ev in events("pause::UpgradePicked") {
    triggered = true;
}
if !triggered { return; }

let states = query("WaveState");
if states.len() == 0 { return; }
let state = states[0];

let next = get(state, "WaveState", "number") + 1;
let dur = get(state, "WaveState", "duration");
set(state, "WaveState", "number", next);
set(state, "WaveState", "remaining", dur);
set(state, "WaveState", "in_intermission", 0);
emit("WaveStarted", #{ number: next });

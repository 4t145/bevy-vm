// wave_tick：倒计时本波剩余时间，归零即 emit WaveCompleted + 进入 intermission。
// pause 期间整个 system 由 run_if 跳过——倒计时自然冻结。

let states = query("WaveState");
if states.len() == 0 { return; }
let state = states[0];

if get(state, "WaveState", "in_intermission") == 1 { return; }

let dt = delta();
let remaining = get(state, "WaveState", "remaining") - dt;

if remaining <= 0.0 {
    let n = get(state, "WaveState", "number");
    emit("WaveCompleted", #{ number: n });
    set(state, "WaveState", "remaining", 0.0);
    set(state, "WaveState", "in_intermission", 1);
    return;
}

set(state, "WaveState", "remaining", remaining);

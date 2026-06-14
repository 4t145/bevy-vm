// pause plugin：由事件驱动 pause/resume。
//
// 触发 pause：wave::WaveCompleted（波次结束 → 弹菜单）/ combat::PlayerDied（死亡画面）。
// 触发 resume：UpgradePicked（玩家选完菜单）/ death::RestartGame（按 R 重开）。
// 注意本 system **不带 run_if**——暂停状态下也要跑（不然就没法 resume）。

for _ev in events("wave::WaveCompleted") {
    pause();
}
for _ev in events("combat::PlayerDied") {
    pause();
}

// upgrade.rhai 在玩家点选项时 emit UpgradePicked——本 plugin 消费它解除暂停。
for _ev in events("UpgradePicked") {
    resume();
}

// death plugin 的 RestartGame 同样解除暂停（从死亡画面回到游戏）。
for _ev in events("death::RestartGame") {
    resume();
}

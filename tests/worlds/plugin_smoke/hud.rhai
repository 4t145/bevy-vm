// 跑在 hud plugin 上下文中：
// - 短名 "HudKind" 优先解析成 hud::HudKind（自家命名空间）
// - 跨 plugin 必须写全限定 "tiles::Tile"
// - host 注册的内置组件（如 Position）走全局空间，短名直接命中

import "helpers" as h;

let touched = 0;
for tile in query("tiles::Tile") {
    let v = get(tile, "tiles::Tile", "value");
    set(tile, "tiles::Tile", "value", v + h::increment());
    touched += 1;
}

// 写一个 hud 实体记录被触达的 tile 数。下面 spawn_entity 的 entity 不挂
// HudKind 的话脚本结束就消失——挂上让外部测试 query 得到。
let marker = spawn_entity();
set(marker, "HudKind", "kind", "report");
set(marker, "tiles::Tile", "value", touched);

// board::setup —— 第一帧加载关卡 + spawn 所有 tile + 相机。
//
// 关卡数据来自 levels.json（由 tools/robbo_levels 离线生成）。同一帧 spawn
// 完整棋盘——Robbo 是 player module spawn，但要 board 先把 GameRoot / 相机
// 准备好（spawn 顺序：board → player → hud）。

import "sprites" as sp;

let game = query_first("Game");
if game == () { return; }

if get(game, "Game", "board_built") == 1 { return; }
set(game, "Game", "board_built", 1);

// 加载关卡数据并 spawn。先解析尺寸——相机位置也按之确定。
let levels = load_config("levels.json");
let level_idx = get(game, "Game", "level_idx");
let level_path = `levels.${level_idx}`;

let height = config_len(levels, `${level_path}.rows`);
let width = 16;   // 全部关都是 16 宽

// 棋盘世界坐标：tile (i, j) 中心 = (i*32, -j*32)（j 沿 -Y 向下）。
// 相机看 grid 中心 (w/2*32, -h/2*32)。Camera2d 默认正交、1px = 1 单位。
// viewport_height 设成 (height + 2) * 32：让整张棋盘 + 上下各一格留白
// 都能看到，无论窗口比例如何。
let cam = spawn_entity();
set(cam, "MainCam", "", #{});
attach_camera_2d(cam, #{
    clear_color: srgba(0.05, 0.05, 0.07, 1.0),
    viewport_height: (height.to_float() + 2.0) * 32.0,
});
let cx = (width.to_float() - 1.0) * 16.0;
let cy = -(height.to_float() - 1.0) * 16.0;
set_translation(cam, [cx, cy, 999.9]);

// GameRoot：屏幕坐标 UI——HUD 用。游戏对象用 world-space sprite。
let root = spawn_entity();
set(root, "GameRoot", "", #{});
set(root, "Node", "", #{
    position_type: "Absolute",
    top: px(0.0),
    left: px(0.0),
    width: percent(100.0),
    height: percent(100.0),
});

// BoardRoot 不再当 Cell 的父节点——只作为重建时的 marker（restart 时按
// VmTag despawn 即可，BoardRoot 的存在仅用于让 hud 能 query 知道棋盘存在）。
// 父子层级会触发 Bevy hierarchy 的 Visibility/InheritedVisibility 检查，
// 而 sprite 自带这些组件、空 marker 实体没有——避免 B0004 警告，cell 直接
// 平铺在 World 上。
let board = spawn_entity();
set(board, "BoardRoot", "", #{});

let screws = 0;
for j in 0..height {
    let row = config_get(levels, `${level_path}.rows.${j}`);
    for i in 0..width {
        let c = row.sub_string(i, 1);
        if c == "T" { screws += 1; }

        let tile_idx = sp::tile_for_char(c);
        if tile_idx < 0 { continue; }

        // Robbo（'R'）由 player module 单独 spawn——这里给它建临时 Cell，
        // player.rhai 第一帧 take over。
        let cell = spawn_entity();
        set(cell, "Cell", "i", i);
        set(cell, "Cell", "j", j);
        set(cell, "Cell", "kind", c);

        attach_sprite(cell, #{
            image: "robbo/icons32.png",
            atlas: #{
                tile_size: [32, 32],
                columns: sp::atlas_columns(),
                rows: sp::atlas_rows(),
                index: tile_idx,
            },
            custom_size: [32.0, 32.0],
        });
        set_translation(cell, [i.to_float() * 32.0, -j.to_float() * 32.0, 0.0]);
    }
}

set(game, "Game", "screws_total", screws);

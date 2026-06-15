// board module 脚本：第一帧 spawn 相机 + 全屏 GameRoot + 棋盘 grid。
// restart 后由 cell::RestartGame handler 清空 tile 重建——layout_built 标记
// 用来识别"是否已经 spawn 过 GameRoot/相机"。
//
// 整个 game 走 Bevy UI：tile 是 Node + Button，没有 sprite。这样 picking
// 行为统一（UI backend 一种），不再需要 Camera2d transform 居中棋盘——
// flex layout 把 grid 推到屏幕中间即可。

import "helpers" as h;

let board_q = query("board::Board");
if board_q.len() == 0 {
    return;
}
let board = board_q[0];

if get(board, "board::Board", "layout_built") == 0 {
    init_layout(board);
    set(board, "board::Board", "layout_built", 1);
}

fn init_layout(board) {
    // GameRoot 第一次缺席时一并 spawn 相机 + GameRoot + HUD slot 占位；
    // restart 流程只 despawn BoardGrid，保留 camera / GameRoot / HUD slot，
    // 所以第二局起 query("GameRoot") 仍命中、跳过 spawn。
    //
    // 这里**先 spawn HUD slot 再 spawn BoardGrid** 是关键——保证两者作为
    // GameRoot 的子节点的物理顺序（hierarchy first child = top in column）
    // 跨 restart 一致。如果 HUD 自己第一次跑时 spawn slot，第一局 HUD slot
    // 会在 BoardGrid 之后 append，而 restart 后的 BoardGrid 又在 HUD slot
    // 之后 append——前后两局视觉错位。
    let root_q = query("GameRoot");
    let root = if root_q.len() > 0 {
        root_q[0]
    } else {
        // Camera2d——整 game 唯一相机。UI 都画在它上面。
        let cam = spawn_entity();
        attach_camera_2d(cam, #{
            clear_color: srgba(0.10, 0.10, 0.10, 1.0),
        });

        let r = spawn_entity();
        set(r, "GameRoot", "", #{});
        set(r, "Node", "", #{
            width: percent(100.0),
            height: percent(100.0),
            display: "Flex",
            flex_direction: "Column",
            align_items: "Center",
            justify_content: "FlexStart",
            row_gap: px(12.0),
        });
        set(r, "BackgroundColor", "", srgba(0.10, 0.10, 0.10, 1.0));

        // HUD slot——空的 status bar 容器，hud.rhai 第一次跑时往里 append。
        let slot = spawn_entity();
        set(slot, "HudSlot", "", #{});
        set(slot, "Node", "", #{
            width: percent(100.0),
            height: px(40.0),
            display: "Flex",
            flex_direction: "Row",
            align_items: "Center",
            justify_content: "Center",
            column_gap: px(12.0),
            padding: #{
                left: px(8.0), right: px(8.0),
                top: px(4.0), bottom: px(4.0),
            },
        });
        set(slot, "BackgroundColor", "", h::hud_bg_color());
        set_parent(slot, r);

        r
    };

    spawn_board_grid(root, board);
}

fn spawn_board_grid(root, board) {
    let w = get(board, "board::Board", "width");
    let h = get(board, "board::Board", "height");
    let ts = h::tile_size();
    let gap = h::tile_gap();

    // grid 容器：flex column 装行，每行 flex row 装 tile。
    // 不用 Display::Grid 是因为脚本端尚未支持 RepeatedGridTrack 这类 enum
    // 字段，flex 嵌套的视觉效果完全等价。
    let grid = spawn_entity();
    set(grid, "BoardGrid", "", #{});
    set(grid, "Node", "", #{
        display: "Flex",
        flex_direction: "Column",
        align_items: "Center",
        row_gap: px(gap),
        padding: #{
            left: px(gap), right: px(gap),
            top: px(gap),  bottom: px(gap),
        },
    });
    set(grid, "BackgroundColor", "", h::board_bg_color());
    set_parent(grid, root);

    for y in 0..h {
        let row = spawn_entity();
        set(row, "Node", "", #{
            display: "Flex",
            flex_direction: "Row",
            column_gap: px(gap),
        });
        set(row, "BackgroundColor", "", srgba(0.0, 0.0, 0.0, 0.0));
        set_parent(row, grid);

        for x in 0..w {
            spawn_tile(row, x, y, ts);
        }
    }
}

fn spawn_tile(row, x, y, ts) {
    let cell = spawn_entity();
    set(cell, "Node", "", #{
        width: px(ts),
        height: px(ts),
        align_items: "Center",
        justify_content: "Center",
    });
    set(cell, "Button", "", #{});
    set(cell, "BackgroundColor", "", h::color_hidden());
    attach_pickable(cell);
    set(cell, "cell::Tile", "x", x);
    set(cell, "cell::Tile", "y", y);
    set_parent(cell, row);

    // label child：文字 + 颜色，由 anim.rhai 每帧更新。
    let label = spawn_entity();
    set(label, "Node", "", #{});
    set(label, "Text", "", "");
    set(label, "TextFont", "font_size", h::label_font_size());
    set(label, "TextColor", "", h::text_color_default());
    set_parent(label, cell);

    set(cell, "cell::Tile", "label", label);
}

// board module 脚本：第一帧 spawn 相机 + 全屏 GameRoot + 棋盘 grid。
//
// 整个 game 走 Bevy UI：Cell 是 Node，没有 sprite。flex layout 把 grid 推到
// 屏幕中间。restart 不重建 layout（尺寸固定），piece module 的 RestartGame
// handler 只把所有 Cell.value 清零。

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
    // GameRoot 第一次缺席时一并 spawn 相机 + GameRoot + HUD slot 占位。
    // restart 不动 layout——所以第二局起 query("GameRoot") 仍命中、跳过 spawn。
    //
    // 先 spawn HUD slot 再 spawn BoardGrid，让 hierarchy first child = top
    // in column 跨 restart 一致。
    let root_q = query("GameRoot");
    let root = if root_q.len() > 0 {
        root_q[0]
    } else {
        let cam = spawn_entity();
        attach_camera_2d(cam, #{
            clear_color: h::color_bg(),
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
        set(r, "BackgroundColor", "", h::color_bg());

        let slot = spawn_entity();
        set(slot, "HudSlot", "", #{});
        set(slot, "Node", "", #{
            width: percent(100.0),
            height: px(40.0),
            display: "Flex",
            flex_direction: "Row",
            align_items: "Center",
            justify_content: "Center",
            column_gap: px(20.0),
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

    // grid 容器：flex column 装行，每行 flex row 装 cell。每行的 Cell.y
    // 一致；Cell.x 从 0 递增。row_index 0 = 顶行（y=0）。
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
    set(grid, "BackgroundColor", "", h::color_board_bg());
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
            spawn_cell(row, x, y, ts);
        }
    }
}

fn spawn_cell(row, x, y, ts) {
    let cell = spawn_entity();
    set(cell, "Node", "", #{
        width: px(ts),
        height: px(ts),
        align_items: "Center",
        justify_content: "Center",
    });
    set(cell, "BackgroundColor", "", h::color_empty());
    set(cell, "Cell", "x", x);
    set(cell, "Cell", "y", y);
    set(cell, "Cell", "value", 0);
    set_parent(cell, row);

    // label child：保留个空 Text，调试时可以 anim 写字（如显示 kind）；
    // 默认游戏内为空，仅 hud 那侧显示文字。
    let label = spawn_entity();
    set(label, "Node", "", #{});
    set(label, "Text", "", "");
    set(label, "TextFont", "font_size", h::label_font_size());
    set(label, "TextColor", "", h::hud_text_color());
    set_parent(label, cell);

    set(cell, "Cell", "label", label);
}

// board plugin 脚本：第一帧 / restart 后 spawn 棋盘 tile，并居中相机。
//
// populate_mines 也住在这——cell plugin 在首次 reveal 时通过 emit
// `cell::FirstReveal` 不合适：populate_mines 要看 cell entity，但布雷
// 是棋盘的事，所以由 cell.rhai 在 handle_reveal 里直接 query Board 拉
// width/height 自己布雷。这里 board.rhai 只关心 layout（spawn tile）。

import "helpers" as h;

let board_q = query("Board");
if board_q.len() == 0 {
    return;
}
let board = board_q[0];

if get(board, "Board", "layout_built") == 0 {
    init_layout(board);
    set(board, "Board", "layout_built", 1);
}

fn init_layout(board) {
    let w = get(board, "Board", "width");
    let h = get(board, "Board", "height");
    let ts = h::tile_size();
    let pitch = h::tile_pitch();
    let fsize = h::label_font_size();

    for y in 0..h {
        for x in 0..w {
            let e = spawn_entity();
            set(e, "Position", "x", x.to_float() * pitch);
            set(e, "Position", "y", y.to_float() * pitch);
            set(e, "Position", "z", 0.0);
            set(e, "Sprite2d", "", #{
                color: h::color_hidden(),
                custom_size: [ts, ts],
            });
            set(e, "Pickable", "", #{});
            set(e, "TextLabel", "", #{
                content: "",
                font_size: fsize,
                color: h::text_color_default(),
            });
            // cell::Tile 由 cell plugin 注册——这里全限定引用。
            set(e, "cell::Tile", "x", x);
            set(e, "cell::Tile", "y", y);
        }
    }

    // 居中 2D 相机。难度切换后棋盘尺寸变了也要跟。
    let cx = (w - 1).to_float() * pitch * 0.5;
    let cy = (h - 1).to_float() * pitch * 0.5;
    for cam in query("Camera2d") {
        set(cam, "Position", "x", cx);
        set(cam, "Position", "y", cy);
    }
}

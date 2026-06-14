// board module 脚本：第一帧 / restart 后 spawn 棋盘 tile，并居中相机。
//
// populate_mines 在 cell.rhai 里——首次 reveal 触发；这里只 layout（spawn tile）。

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
            // tile 视觉：sprite 背板 + text 标签共存于同一 entity（都在
            // Transform 上，z 维度文本略高于 sprite 让它显示在前）。
            attach_sprite(e, #{
                color: h::color_hidden(),
                custom_size: [ts, ts],
            });
            attach_text(e, "", fsize, h::text_color_default());
            attach_pickable(e);
            // 翻牌时直接用 set_sprite_color/set_text_content 修改即可。
            set_translation(e, [x.to_float() * pitch, y.to_float() * pitch, 0.0]);
            // cell::Tile 由 cell module 注册——这里全限定引用。
            set(e, "cell::Tile", "x", x);
            set(e, "cell::Tile", "y", y);
        }
    }

    // 居中 2D 相机——attach + 立即 set_translation 把它移到棋盘中心。
    let cx = (w - 1).to_float() * pitch * 0.5;
    let cy = (h - 1).to_float() * pitch * 0.5;
    for cam in query("MainCamera2d") {
        attach_camera_2d(cam, #{
            scale: 1.0,
            clear_color: srgba(0.15, 0.15, 0.18, 1.0),
        });
        set_translation(cam, [cx, cy, 999.9]);
    }
}

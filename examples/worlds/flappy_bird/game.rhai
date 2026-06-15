// game module 脚本：第一帧 spawn 相机 + GameRoot + 居中 PlayArea + 地面。
//
// 后续 restart 不重建 layout——只 bird / pipes 各自复位。Game.elapsed 在
// 这里推进（playing 状态时 += dt），pipes 据此决定 spawn 节奏。

import "helpers" as h;

let game_q = query("Game");
if game_q.len() == 0 { return; }
let game = game_q[0];

if get(game, "Game", "layout_built") == 0 {
    init_layout();
    set(game, "Game", "layout_built", 1);
}

// elapsed 推进——dead 时冻结，让 hud / pipes 看到稳定值。
if get(game, "Game", "state") == 0 {
    set(game, "Game", "elapsed", get(game, "Game", "elapsed") + delta());
}

fn init_layout() {
    let cam = spawn_entity();
    attach_camera_2d(cam, #{
        clear_color: h::color_sky(),
    });

    let root = spawn_entity();
    set(root, "GameRoot", "", #{});
    set(root, "Node", "", #{
        width: percent(100.0),
        height: percent(100.0),
        display: "Flex",
        flex_direction: "Column",
        align_items: "Center",
        justify_content: "Center",
    });
    set(root, "BackgroundColor", "", h::color_sky());

    // PlayArea 是固定 480x640 的"游戏视口"，居中在屏幕里。所有 sprite-like
    // 子节点用 position_type=Absolute 来按 (left, top) 定位——不依赖 flex
    // 排序。BackgroundColor 用天蓝填满画面之外的部分由 GameRoot 负责。
    let play = spawn_entity();
    set(play, "PlayArea", "", #{});
    set(play, "Node", "", #{
        width: px(h::play_width()),
        height: px(h::play_height()),
        // PlayArea 内部子节点用 position_type=Absolute 按 (left, top) 定位——
        // 不依赖 flex 排序。PlayArea 自身仍在 flex 父中按 align_items=Center
        // 居中。管道飘出 PlayArea 时由 pipes plugin 主动 despawn 收尾，所以
        // 不需要在此 clip overflow。
    });
    set(play, "BackgroundColor", "", h::color_sky());
    set_parent(play, root);

    spawn_ground(play);
}

// 地面：playfield 底部一条厚 ground_height 的矩形 + 顶部一条更细的草层。
fn spawn_ground(play) {
    let gh = h::ground_height();
    let pw = h::play_width();
    let ph = h::play_height();
    let grass_thickness = 8.0;

    let dirt = spawn_entity();
    set(dirt, "Ground", "", #{});
    set(dirt, "Node", "", #{
        position_type: "Absolute",
        left: px(0.0),
        top: px(ph - gh),
        width: px(pw),
        height: px(gh),
    });
    set(dirt, "BackgroundColor", "", h::color_ground());
    set_parent(dirt, play);

    let grass = spawn_entity();
    set(grass, "Node", "", #{
        position_type: "Absolute",
        left: px(0.0),
        top: px(ph - gh),
        width: px(pw),
        height: px(grass_thickness),
    });
    set(grass, "BackgroundColor", "", h::color_ground_top());
    set_parent(grass, play);
}

// crosshair：屏幕正中一个 12×12 的十字。
//
// root 100% 全屏 + 居中布局；中心一个 12×12 容器，里面两条交叉线
// （水平 12×2、竖直 2×12，绝对定位）。

import "helpers" as h;

let states = query("CrosshairSpawnState");
if states.len() == 0 { return; }
let s = states[0];
if get(s, "CrosshairSpawnState", "spawned") == 1 { return; }
set(s, "CrosshairSpawnState", "spawned", 1);

let root = spawn_entity();
set(root, "Crosshair", "", #{});
set(root, "Node", "", #{
    width: h::percent(100.0),
    height: h::percent(100.0),
    display: "Flex",
    align_items: "Center",
    justify_content: "Center",
});
set(root, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));

let center = spawn_entity();
set(center, "Node", "", #{
    width: h::px(12.0),
    height: h::px(12.0),
    position_type: "Relative",
});
set(center, "BackgroundColor", "", h::srgba(0.0, 0.0, 0.0, 0.0));
set_parent(center, root);

let h_bar = spawn_entity();
set(h_bar, "Node", "", #{
    position_type: "Absolute",
    left: h::px(0.0),
    top: h::px(5.0),
    width: h::px(12.0),
    height: h::px(2.0),
});
set(h_bar, "BackgroundColor", "", h::color_crosshair());
set_parent(h_bar, center);

let v_bar = spawn_entity();
set(v_bar, "Node", "", #{
    position_type: "Absolute",
    left: h::px(5.0),
    top: h::px(0.0),
    width: h::px(2.0),
    height: h::px(12.0),
});
set(v_bar, "BackgroundColor", "", h::color_crosshair());
set_parent(v_bar, center);

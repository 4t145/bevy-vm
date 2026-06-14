// 跨 plugin 共用 helper module。各 plugin 通过 `import "helpers" as h;` 引入。
//
// 注意 import 的语义：被 import 的模块和宿主在同一个 Engine 里跑，host 函数
// 透明继承；模块内调 `query("X")` 时使用宿主 plugin 的命名空间上下文。

// ---- reflect 形态构造 -----------------------------------------------------

fn srgba(r, g, b, a) {
    #{ "Srgba": #{ red: r, green: g, blue: b, alpha: a } }
}
fn px(v) { #{ "Px": v } }
fn percent(v) { #{ "Percent": v } }

// ---- 调色板（与参考项目 bevy-lab/anim.rs 对齐） --------------------------

fn color_hidden() { srgba(0.45, 0.55, 0.65, 1.0) }
fn color_hidden_hover() { srgba(0.62, 0.72, 0.82, 1.0) }
fn color_revealed() { srgba(0.85, 0.85, 0.88, 1.0) }
fn color_flag() { srgba(0.95, 0.75, 0.20, 1.0) }
fn color_flag_hover() { srgba(1.00, 0.88, 0.40, 1.0) }
fn color_mine() { srgba(0.95, 0.25, 0.25, 1.0) }
fn text_color_default() { srgba(0.10, 0.10, 0.15, 1.0) }
fn flag_text_color() { srgba(0.20, 0.10, 0.10, 1.0) }
fn hud_bg_color() { srgba(0.10, 0.10, 0.10, 0.85) }
fn hud_text_color() { srgba(1.0, 1.0, 1.0, 1.0) }
fn button_color() { srgba(0.30, 0.30, 0.36, 1.0) }
fn button_hover_color() { srgba(0.45, 0.50, 0.65, 1.0) }

// ---- 数字格的颜色 ---------------------------------------------------------

fn number_color(n) {
    if n == 1 { return srgba(0.20, 0.40, 0.95, 1.0); }
    if n == 2 { return srgba(0.20, 0.65, 0.30, 1.0); }
    if n == 3 { return srgba(0.95, 0.30, 0.30, 1.0); }
    if n == 4 { return srgba(0.30, 0.20, 0.65, 1.0); }
    if n == 5 { return srgba(0.65, 0.30, 0.20, 1.0); }
    if n == 6 { return srgba(0.20, 0.65, 0.65, 1.0); }
    if n == 7 { return srgba(0.10, 0.10, 0.15, 1.0); }
    srgba(0.40, 0.40, 0.40, 1.0)
}

// ---- 几何 -----------------------------------------------------------------

fn tile_pitch() { 64.0 }
fn tile_size() { 60.0 }
fn label_font_size() { 32.0 }
fn hud_font_size() { 22.0 }

// ---- 时间格式化 -----------------------------------------------------------

fn format_mmss(secs) {
    let total = secs.to_int();
    let m = total / 60;
    let s = total % 60;
    let mm = if m < 10 { `0${m}` } else { `${m}` };
    let ss = if s < 10 { `0${s}` } else { `${s}` };
    `${mm}:${ss}`
}

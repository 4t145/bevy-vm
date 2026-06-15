// 跨 module 共用 helper，`import "helpers" as h;`。
//
// 把 tetromino 形状表 / 调色板 / 几何常量集中到这里——piece、anim、hud
// 都按 kind 拿一致的颜色 / 形状，避免分散后值漂移。

// ---- 几何 -----------------------------------------------------------------

fn board_width()  { 10 }
fn board_height() { 20 }
fn tile_size()    { 28.0 }
fn tile_gap()     { 1.0 }
fn label_font_size() { 18.0 }
fn hud_font_size()   { 18.0 }

// ---- 重力 / 锁定 ---------------------------------------------------------

fn gravity_base()    { 0.8 }   // 1 级时每格重力间隔（秒）
fn gravity_min()     { 0.05 }  // 等级再高也不低于此值
fn soft_drop_factor(){ 0.08 }  // 软降时 gravity_base * factor
fn lock_delay()      { 0.5 }   // 触底后保留 0.5 秒供平移 / 旋转

// 升级阈值：每 10 行升一级，gravity 乘 0.85^level。
fn lines_per_level() { 10 }
fn gravity_falloff() { 0.85 }

// ---- 调色板 ---------------------------------------------------------------

fn color_bg()         { srgba(0.07, 0.08, 0.10, 1.0) }
fn color_board_bg()   { srgba(0.14, 0.15, 0.18, 1.0) }
fn color_empty()      { srgba(0.20, 0.21, 0.25, 1.0) }
fn color_grid_line()  { srgba(0.10, 0.10, 0.12, 1.0) }
fn color_ghost()      { srgba(0.50, 0.55, 0.65, 0.55) }
fn hud_bg_color()     { srgba(0.10, 0.10, 0.10, 0.85) }
fn hud_text_color()   { srgba(0.95, 0.95, 0.95, 1.0) }
fn hud_warn_color()   { srgba(0.95, 0.30, 0.30, 1.0) }
fn hud_pause_color()  { srgba(0.95, 0.85, 0.20, 1.0) }
fn hud_clear_color()  { srgba(0.20, 0.85, 0.30, 1.0) }

// kind 1..7 对应 I O T S Z J L。
fn piece_color(kind) {
    if kind == 1 { return srgba(0.20, 0.80, 0.95, 1.0); }   // I  cyan
    if kind == 2 { return srgba(0.95, 0.85, 0.20, 1.0); }   // O  yellow
    if kind == 3 { return srgba(0.65, 0.30, 0.85, 1.0); }   // T  purple
    if kind == 4 { return srgba(0.30, 0.85, 0.40, 1.0); }   // S  green
    if kind == 5 { return srgba(0.95, 0.30, 0.30, 1.0); }   // Z  red
    if kind == 6 { return srgba(0.30, 0.45, 0.95, 1.0); }   // J  blue
    if kind == 7 { return srgba(0.95, 0.55, 0.20, 1.0); }   // L  orange
    color_empty()
}

fn piece_letter(kind) {
    if kind == 1 { return "I"; }
    if kind == 2 { return "O"; }
    if kind == 3 { return "T"; }
    if kind == 4 { return "S"; }
    if kind == 5 { return "Z"; }
    if kind == 6 { return "J"; }
    if kind == 7 { return "L"; }
    "?"
}

// ---- 形状表 --------------------------------------------------------------
//
// shape_offsets(kind, rotation) 返回 4 个 [dx, dy] —— 相对 piece 锚点
// (Piece.x, Piece.y) 的位移。布局以 SRS 的 4x4 / 3x3 矩阵简化版为准，未做
// wall-kick：旋转后撞墙就回退（见 piece.rhai）。
//
// I：水平 4 横；O：2x2；T/S/Z/J/L：3x3。

fn shape_offsets(kind, rotation) {
    let r = rotation % 4;
    if kind == 1 {
        // I  锚点 (1,1) of 4x4 box
        if r == 0 { return [[-1, 0], [ 0, 0], [ 1, 0], [ 2, 0]]; }
        if r == 1 { return [[ 1,-1], [ 1, 0], [ 1, 1], [ 1, 2]]; }
        if r == 2 { return [[-1, 1], [ 0, 1], [ 1, 1], [ 2, 1]]; }
        return         [[ 0,-1], [ 0, 0], [ 0, 1], [ 0, 2]];
    }
    if kind == 2 {
        // O  4 旋转一致
        return [[0, 0], [1, 0], [0, 1], [1, 1]];
    }
    if kind == 3 {
        // T
        if r == 0 { return [[-1, 0], [ 0, 0], [ 1, 0], [ 0, 1]]; }
        if r == 1 { return [[ 0,-1], [ 0, 0], [ 0, 1], [ 1, 0]]; }
        if r == 2 { return [[-1, 0], [ 0, 0], [ 1, 0], [ 0,-1]]; }
        return         [[ 0,-1], [ 0, 0], [ 0, 1], [-1, 0]];
    }
    if kind == 4 {
        // S
        if r == 0 { return [[ 0, 0], [ 1, 0], [-1, 1], [ 0, 1]]; }
        if r == 1 { return [[ 0,-1], [ 0, 0], [ 1, 0], [ 1, 1]]; }
        if r == 2 { return [[ 0, 0], [ 1, 0], [-1, 1], [ 0, 1]]; }
        return         [[ 0,-1], [ 0, 0], [ 1, 0], [ 1, 1]];
    }
    if kind == 5 {
        // Z
        if r == 0 { return [[-1, 0], [ 0, 0], [ 0, 1], [ 1, 1]]; }
        if r == 1 { return [[ 1,-1], [ 0, 0], [ 1, 0], [ 0, 1]]; }
        if r == 2 { return [[-1, 0], [ 0, 0], [ 0, 1], [ 1, 1]]; }
        return         [[ 1,-1], [ 0, 0], [ 1, 0], [ 0, 1]];
    }
    if kind == 6 {
        // J
        if r == 0 { return [[-1, 0], [ 0, 0], [ 1, 0], [ 1, 1]]; }
        if r == 1 { return [[ 0,-1], [ 0, 0], [ 0, 1], [ 1,-1]]; }
        if r == 2 { return [[-1,-1], [-1, 0], [ 0, 0], [ 1, 0]]; }
        return         [[-1, 1], [ 0,-1], [ 0, 0], [ 0, 1]];
    }
    if kind == 7 {
        // L
        if r == 0 { return [[-1, 0], [ 0, 0], [ 1, 0], [-1, 1]]; }
        if r == 1 { return [[-1,-1], [ 0,-1], [ 0, 0], [ 0, 1]]; }
        if r == 2 { return [[ 1,-1], [-1, 0], [ 0, 0], [ 1, 0]]; }
        return         [[ 0,-1], [ 0, 0], [ 0, 1], [ 1, 1]];
    }
    []
}

// ---- 计分 ----------------------------------------------------------------
//
// 经典 Tetris：1=100, 2=300, 3=500, 4=800（"Tetris"），都乘以等级。
fn score_for_lines(n, level) {
    let base = if n == 1 { 100 }
        else if n == 2 { 300 }
        else if n == 3 { 500 }
        else if n == 4 { 800 }
        else { 0 };
    base * level
}

// ---- 时间格式 ------------------------------------------------------------

fn format_mmss(secs) {
    let total = secs.to_int();
    let m = total / 60;
    let s = total % 60;
    let mm = if m < 10 { `0${m}` } else { `${m}` };
    let ss = if s < 10 { `0${s}` } else { `${s}` };
    `${mm}:${ss}`
}

// 跨 module 共用 helper，`import "helpers" as h;`。
//
// 把世界尺寸 / 物理常量 / 调色板集中于此，让 game / bird / pipes / hud
// 看到同一组数值。坐标约定：playfield 自身居中，原点在 PlayArea 的中心，
// +X 向右、+Y 向上；像素即世界单位（Camera2d）。

// ---- 世界尺寸 -------------------------------------------------------------

fn play_width()   { 480.0 }   // playfield 宽
fn play_height()  { 640.0 }   // playfield 高
fn ground_height(){ 60.0 }    // 底部地面厚度（像素）
fn bird_size()    { 22.0 }    // 鸟方块边长（碰撞判定按方框）

// 鸟在 X 上固定位置（playfield 内的 X 偏移；负数表示偏左侧）。
fn bird_x()       { -120.0 }

// 鸟初始 y（playfield 中心略上）。
fn bird_y_start() {  60.0 }

// ---- 物理 ----------------------------------------------------------------
//
// 单位：像素 / 秒。flappy bird 风格——重力始终向下，按 Space 给一个固定
// 向上的瞬时速度（"flap"）。
fn gravity()       {  600.0 }   // px/s^2
fn flap_velocity() {  280.0 }   // px/s（瞬时向上速度，绝对值——脚本里取正）
fn max_fall_speed(){  500.0 }   // 终端速度，限幅避免穿越管道

// ---- 管道 ----------------------------------------------------------------

fn pipe_width()        {  64.0 }
fn pipe_gap()          { 150.0 }   // 上下管道之间的开口
fn pipe_min_center_y() { -120.0 }  // 管道开口中心 y 的最小值
fn pipe_max_center_y() {  140.0 }  // 管道开口中心 y 的最大值
fn pipe_speed()        { 160.0 }   // px/s 向左
fn pipe_spawn_period() {   1.6 }   // 秒，每多少秒生成一对新管道
fn pipe_initial_delay(){   1.2 }   // 秒，开局后等多久才开始生成（让玩家就位）

// ---- 调色板 ---------------------------------------------------------------

fn color_sky()        { srgba(0.40, 0.70, 0.95, 1.0) }
fn color_ground()     { srgba(0.85, 0.75, 0.40, 1.0) }
fn color_ground_top() { srgba(0.50, 0.85, 0.30, 1.0) }   // 地面草层
fn color_pipe()       { srgba(0.20, 0.75, 0.30, 1.0) }
fn color_pipe_edge()  { srgba(0.10, 0.50, 0.20, 1.0) }
fn color_bird()       { srgba(0.95, 0.85, 0.20, 1.0) }
fn color_bird_edge()  { srgba(0.85, 0.55, 0.10, 1.0) }
fn hud_text_color()   { srgba(1.0, 1.0, 1.0, 1.0) }
fn hud_warn_color()   { srgba(0.95, 0.30, 0.30, 1.0) }
fn hud_panel_bg()     { srgba(0.0, 0.0, 0.0, 0.55) }

// ---- HUD -----------------------------------------------------------------

fn hud_font_big()    { 48.0 }
fn hud_font_medium() { 28.0 }
fn hud_font_small()  { 18.0 }

// ---- 时间格式 ------------------------------------------------------------

fn format_score(n) {
    `${n}`
}

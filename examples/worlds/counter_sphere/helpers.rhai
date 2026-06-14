// counter_sphere helpers：颜色 / 几何常量 / 工具函数。

fn srgba(r, g, b, a) {
    #{ "Srgba": #{ red: r, green: g, blue: b, alpha: a } }
}
fn px(v)      { #{ "Px": v } }
fn percent(v) { #{ "Percent": v } }

// 颜色。
fn color_player()   { srgba(0.30, 0.70, 1.00, 1.00) }   // 长方体
fn color_enemy()    { srgba(0.95, 0.30, 0.30, 1.00) }   // 球体
fn color_gun()      { srgba(0.55, 0.40, 0.20, 1.00) }   // 烧火棍——木棕
fn color_ground()   { srgba(0.18, 0.20, 0.16, 1.00) }
fn color_obstacle() { srgba(0.32, 0.30, 0.24, 1.00) }
fn color_bullet()   { srgba(1.00, 0.95, 0.30, 1.00) }
fn color_crosshair(){ srgba(0.90, 1.00, 0.90, 0.95) }
fn color_hud_text() { srgba(0.95, 0.95, 0.97, 1.00) }
fn color_win_bg()   { srgba(0.05, 0.30, 0.10, 0.85) }

// 几何 / 节奏常量。
fn ground_y()         { 0.0 }
fn arena_radius()     { 28.0 }
fn player_height()    { 1.8 }     // 长方体 y 尺寸
fn eye_offset_y()     { 0.7 }     // 相机相对玩家中心的高度（朝顶面）
fn enemy_radius()     { 0.6 }
fn bullet_radius()    { 0.08 }
fn bullet_speed()     { 25.0 }
fn bullet_lifetime()  { 1.5 }
fn bullet_damage()    { 25 }
fn collide_bullet_enemy() { 1.0 } // 比真实 r_bullet+r_enemy 略大——避免高速子弹一帧穿过
fn fire_cooldown()    { 0.35 }
fn move_speed()       { 6.0 }
fn turn_speed()       { 2.5 }     // rad/s

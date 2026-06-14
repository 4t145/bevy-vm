// 跨 plugin 共用 helper module。`import "helpers" as h;`

// ---- reflect 形态构造 ----------------------------------------------------

fn srgba(r, g, b, a) {
    #{ "Srgba": #{ red: r, green: g, blue: b, alpha: a } }
}
fn px(v) { #{ "Px": v } }
fn percent(v) { #{ "Percent": v } }

// ---- 颜色 ----------------------------------------------------------------

fn color_player()        { srgba(0.30, 0.55, 0.95, 1.0) }
fn color_enemy_small()   { srgba(0.95, 0.30, 0.30, 1.0) }
fn color_enemy_medium()  { srgba(0.75, 0.20, 0.85, 1.0) }
fn color_enemy_large()   { srgba(0.55, 0.05, 0.10, 1.0) }
fn color_projectile()    { srgba(0.95, 0.85, 0.20, 1.0) }
fn color_xp_orb()        { srgba(0.20, 0.85, 0.95, 1.0) }
fn color_hp_bar_bg()     { srgba(0.20, 0.20, 0.20, 0.85) }
fn color_hp_bar_fill()   { srgba(0.85, 0.20, 0.20, 1.0) }
fn color_xp_bar_fill()   { srgba(0.20, 0.85, 0.95, 1.0) }
fn color_hud_text()      { srgba(1.0, 1.0, 1.0, 1.0) }
fn color_panel_bg()      { srgba(0.10, 0.10, 0.14, 0.92) }
fn color_card_bg()       { srgba(0.20, 0.22, 0.30, 1.0) }
fn color_card_hover()    { srgba(0.30, 0.40, 0.55, 1.0) }

// ---- 几何 ----------------------------------------------------------------

fn ground_y()            { 0.5 }   // 实体中心 y（cube 半高 + 地面 0）
fn arena_radius()        { 18.0 }  // 敌人 spawn 圆周半径（屏幕外）
fn pickup_radius()       { 0.6 }   // 玩家吸经验球距离
fn collide_player_enemy(){ 0.8 }   // 玩家 ↔ 敌人
fn collide_proj_enemy()  { 0.5 }   // 投射物 ↔ 敌人

// ---- 数学 ----------------------------------------------------------------

fn vec_dist_xz(ax, az, bx, bz) {
    let dx = bx - ax;
    let dz = bz - az;
    (dx * dx + dz * dz).sqrt()
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

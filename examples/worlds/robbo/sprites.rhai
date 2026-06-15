// Robbo sprite atlas index 映射。icons32.png 是 12×8 个 32×32 tile 的网格。
//
// 索引来源：参考项目 src/entities.rs 的 create_* 函数硬编码值。同游戏需要
// 多帧动画的实体（Robbo / Bird / Eyes / Bear 等）这里只取第一帧——动画
// 后续在 game system 里按帧切。
//
// `tile_for_char(c)` 是关卡数据 char → atlas index 的总入口；非已知字符
// 回 -1 让调用方跳过 spawn（保留空格）。

fn atlas_columns() { 12 }
fn atlas_rows()    { 8 }
fn atlas_tile()    { 32 }

// ---- walls (按 entities.rs 顺序：O o - Q q p P s S) -------------------
fn sprite_wall_white()    { 2 }
fn sprite_wall_green()    { 29 }
fn sprite_wall_black()    { 19 }
fn sprite_wall_red()      { 3 }
fn sprite_wall_fat()      { 21 }
fn sprite_wall_round()    { 68 }
fn sprite_wall_boulder()  { 69 }
fn sprite_wall_square()   { 10 }
fn sprite_wall_lattice()  { 22 }

// ---- 静态 / 拾取 ------------------------------------------------------
fn sprite_ground()        { 77 }
fn sprite_door()          { 9 }
fn sprite_static_box()    { 20 }
fn sprite_push_box()      { 6 }
fn sprite_screw()         { 4 }
fn sprite_key()           { 7 }
fn sprite_ammo()          { 5 }
fn sprite_capsule()       { 17 }
fn sprite_bomb()          { 8 }
fn sprite_question()      { 12 }

// ---- 动态 / actor（动画首帧）-----------------------------------------
fn sprite_robbo()         { 60 }
fn sprite_bird()          { 15 }
fn sprite_eyes()          { 32 }
fn sprite_bear()          { 13 }
fn sprite_black_bear()    { 30 }
fn sprite_bullet_h()      { 36 }
fn sprite_bullet_v()      { 38 }
fn sprite_blaster_head()  { 84 }
fn sprite_forcefield()    { 45 }
fn sprite_magnet_n()      { 73 }
fn sprite_magnet_e()      { 0 }
fn sprite_magnet_s()      { 72 }
fn sprite_magnet_w()      { 1 }
// Gun 的视觉索引取决于朝向：GUN_TILES = [56, 53, 54, 55]（E S W N）。
// 这里取首个（朝东），当作通用 gun sprite。
fn sprite_gun()           { 56 }
// Teleport 动画两帧 [48, 49]——取首帧。
fn sprite_teleport()      { 48 }

// ---- 字符 → tile index 总入口 -----------------------------------------
//
// 返回 -1 表示无视觉（空地、未知字符）。'.' 也归为 -1，由调用方决定要不要
// 铺底色。
fn tile_for_char(c) {
    if c == "O" { return sprite_wall_white(); }
    if c == "o" { return sprite_wall_green(); }
    if c == "-" { return sprite_wall_black(); }
    if c == "Q" { return sprite_wall_red(); }
    if c == "q" { return sprite_wall_fat(); }
    if c == "p" { return sprite_wall_round(); }
    if c == "P" { return sprite_wall_boulder(); }
    if c == "s" { return sprite_wall_square(); }
    if c == "S" { return sprite_wall_lattice(); }
    if c == "H" { return sprite_ground(); }
    if c == "R" { return sprite_robbo(); }
    if c == "D" { return sprite_door(); }
    if c == "#" { return sprite_static_box(); }
    if c == "~" { return sprite_push_box(); }
    if c == "}" { return sprite_gun(); }
    if c == "&" { return sprite_teleport(); }
    if c == "^" { return sprite_bird(); }
    if c == "@" { return sprite_bear(); }
    if c == "*" { return sprite_black_bear(); }
    if c == "V" { return sprite_eyes(); }
    if c == "'" { return sprite_ammo(); }
    if c == "T" { return sprite_screw(); }
    if c == "%" { return sprite_key(); }
    if c == "!" { return sprite_capsule(); }
    if c == "b" { return sprite_bomb(); }
    if c == "?" { return sprite_question(); }
    if c == "=" { return sprite_forcefield(); }
    if c == "M" { return sprite_magnet_n(); }
    // L 水平激光 / l 垂直激光是关卡里的激光实体（阶段 3）。原版有专属
    // sprite——目前先用 bullet sprite 占位，待 Laser 系统完整实现时再确定。
    if c == "L" { return sprite_bullet_h(); }
    if c == "l" { return sprite_bullet_v(); }
    -1
}

// ---- 阻挡类（推 / 走 / 子弹打）—— 用于 Robbo 移动判定 ---------------
//
// solid: 完全不可穿（不可推、不可破坏）。墙类 + Door + Gun + Magnet +
// Force Field。
fn is_solid_char(c) {
    c == "O" || c == "o" || c == "-" || c == "Q" || c == "q" ||
    c == "p" || c == "P" || c == "s" || c == "S" ||
    c == "D" || c == "}" || c == "M" || c == "="
}

// pushable: 可被 Robbo 推（前方一格空地即可）。原版 Moveable 集合：
// `#`（Box）、`~`（PushBox）、`b`（Bomb）、`?`（QuestionMark）。
fn is_pushable_char(c) {
    c == "~" || c == "#" || c == "b" || c == "?"
}

// is_destroyable_char: 仅"普通伤害"（子弹直击）能销毁的物体。原版
// entities.rs 带 `Destroyable` 标记：
//   Bird ^、Bear @、BlackBear *、Eyes V、Ammo '、Ground H、
//   QuestionMark ?、Bomb b、ForceField =。
fn is_destroyable_char(c) {
    c == "V" || c == "@" || c == "*" || c == "^" ||
    c == "'" || c == "H" || c == "?" || c == "b" || c == "="
}

// is_undestroyable_char: 完全不可摧毁的"墙类"——爆炸都炸不掉。
// 原版 create_wall 是唯一带 `Undestroyable` 的。
//
// 注意 Door / Gun / Magnet / Capsule / Screw / Key 等没带 Undestroyable
// 但本身没爆炸目标——保守起见不当 Undestroyable，让爆炸能波及。
//
// 关键的 `#` Box 和 `~` PushBox：原版**没** Undestroyable——所以
// **爆炸（is_bomb_damage=true 的 do_damage）能销毁它们**，只是子弹直击不行。
fn is_undestroyable_char(c) {
    c == "O" || c == "o" || c == "-" || c == "Q" || c == "q" ||
    c == "p" || c == "P" || c == "s" || c == "S"
}

// enemy: Robbo 主动撞 → 死，子弹击中 → 销毁。
fn is_enemy_char(c) {
    c == "V" || c == "@" || c == "*" || c == "^"
}

// pickup: 走过即捡起（screw/key/ammo）。Capsule 不在此列——它有"还没收齐
// 螺丝时是 solid"的状态化语义，由 robbo.rhai 单独处理。
fn is_pickup_char(c) {
    c == "T" || c == "%" || c == "'"
}

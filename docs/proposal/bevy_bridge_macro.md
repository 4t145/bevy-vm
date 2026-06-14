# Proposal: `BevyBridge` derive 宏

**状态**：推迟。typed 组件字段还较少，过早抽象不划算。
此文档记录设计空间与权衡，避免下次启动时重新调研。

## 背景

`src/render.rs` 现有 7 个 sync system，每个都走相同的"骨架模板"：

```
collect VM 端实体 + spec → for each: spawn 或 update Bevy 端镜像 → despawn 消失的
```

骨架重复，但骨架中"spawn / update Bevy 组件"的内容因渲染类型而异。
todo.md #2 想用 derive 宏自动化这一层。

调研后发现，"骨架重复"和"字段映射重复"是两件事。

## 三种实施路径

### A. 全自动 derive（最重）

宏读 VM 组件结构，按属性指引生成完整 sync system：

```rust
#[derive(Component, BevyBridge)]
#[bridge(target = "(Sprite, Transform)", update = "in_place", pickable)]
struct Sprite2d {
    #[bridge(into = sprite_color_to_bevy)]
    color: [f32; 4],
    #[bridge(resource)]
    image: Option<ImageBuilder>,
    #[bridge(rename = "custom_size", into = optional_size_to_vec2)]
    custom_size: Option<[f32; 2]>,
    flip_x: bool,
    flip_y: bool,
}
```

宏负责生成 `collect_*` / `spawn_*` / `update_*` / `sync_*` 全套。

**优势**：写一次属性，sync 逻辑全消失。
**劣势**：
- 相机/SceneRender 这类有特殊形态（`looking_at`、不原地更新）的，依然要 escape hatch；
- 属性 DSL 复杂度本身堆积成"小语言"，定型后改起来代价高；
- 估计 300+ 行宏 + 大量边界 case。

### B. 半自动：只 derive 字段映射 trait（曾推荐）

宏只生成"VM 字段 → Bevy 字段"这部分，而非完整 system：

```rust
trait BridgeTo<Target> {
    fn apply_to(&self, target: &mut Target, ctx: &mut BuildContext<'_>);
}

#[derive(BevyBridge)]
#[bridge(target = "Sprite")]
struct Sprite2d {
    #[bridge(into = color_to_bevy)]
    color: [f32; 4],
    ...
}
// 展开为 impl BridgeTo<Sprite> for Sprite2d { fn apply_to(...) { ... } }
```

sync system 仍手写，但"15 行字段映射"压成 `spec.apply_to(&mut sprite, &mut ctx)`。

**优势**：宏只解决重复，骨架由 Rust 表达；特殊情况手写 impl 即可。
**劣势**：减少的代码量没那么夸张——本项目里 sync 系统的"骨架"比"字段映射"重很多。

### C. 不做宏，抽 sync 框架

把骨架抽成通用 trait + 通用函数：

```rust
trait Bridge {
    type Spec: Component + Clone;
    type Bundle: Bundle;
    fn collect(world, ...) -> Vec<(VmEntity, Self::Spec, Transform, bool)>;
    fn spawn(commands, &spec, transform, ...) -> Entity;
    fn update(entity_mut, &spec, ...);
}

fn sync_bridged<B: Bridge>(...) { /* 通用 collect → spawn/update → despawn */ }
```

每个组件实现 `Bridge`；sync system 退化为 `app.add_systems(Update, sync_bridged::<SpriteBridge>)`。

**优势**：零 proc-macro 成本；骨架——本项目里的真实重复——被消除；字段映射保持手写最直观。
**劣势**：相机的 transform 推导、SceneRender 的"不原地更新"仍要 trait 上的小开关；多个 SystemParam（Sprite 要 5 个 ResMut，Mesh 要 4 个，签名写在 trait 里很重）。

## 当时的判断

宏路径（A）是被选中的方向，但实际写之前发现：

1. 项目目前的 typed 组件字段不多，"字段映射重复"的总量没大到值得 300 行宏；
2. 真正重复的是骨架（C 处理），但骨架抽象遇到 SystemParam 异构问题；
3. todo.md #3 后会让字段大量增加（PbrMaterial 30 字段、Sprite2d 加 texture_atlas/rect 等）——
   届时字段映射的重复才会爆发，宏的 ROI 才合理。

因此推迟。

## 重启时的入口建议

下次启动顺序（如果选择 A）：

1. 建 `bevy-vm-macros` proc-macro 子 crate（proc-macro 不能与主 crate 同 crate）；
2. **先做 Sprite2d 一个组件的全套**——验证属性 DSL 能不能 carry，再扩；
3. 边界 case 名单：相机的 `looking_at` transform、SceneRender 的"asset 变更等于 despawn"、
   Camera 的 `clear_color: Option<[f32;4]>` 三态映射；
4. 不要在第一版就追求覆盖所有组件——保留手写 escape hatch 是必需。

如果选择 C，先抽 `Bridge` trait + `sync_bridged` 通用函数，把 Sprite/Mesh/Scene 三个普通形态收编，
相机和文本暂留手写。这条路工作量小但收益也较小。

## 触发条件

启动这件事的合理触发点：

- todo.md #3 完成后，`PbrMaterial` 等组件的字段映射重复变得显著；
- 新加渲染类型（粒子、Gizmo、Light）时，再次发现写第 8 个 sync system 在抄第 7 个。

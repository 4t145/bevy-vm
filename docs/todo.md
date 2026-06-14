# Roadmap / TODO

按优先级排列。每条短描述 + 工作量估计 + 收益。完成一条，删一条（或挪到 done 段尾）。

## Pending

### 1. UI 桥接 —— 真实示例 / Interaction 反向流

第一版 UI 桥已完成（详见 Done 段）。还缺：

- **写一个真实的 HUD 示例**（扫雷难度切换面板、开始-结束面板）
  验证 set Node 字段 + 父子树 + sync_ui 整条路径在真实 Bevy App 里跑通
- **Interaction 反向流**：UI 自有 `Interaction`（Hovered/Pressed/None）
  状态机，sync 层从主 World 反向投影回 VM 端，让脚本能监听 button click。
  当前可用 picking observer 替代（按钮挂 `Pickable` 触发 `PickClick`），
  待第一个 demo 跑出来再决定是否非要 Interaction
- **性能优化**（按 `docs/proposal/ui_sync_incremental.md`，等帧时间数据）


### 3. Picking hover 桥（`Pointer<Over>` / `Pointer<Out>`）

UI 悬浮颜色过渡需要。视觉糖优先级。
现有 `PickingPlugin` 已经处理 `Pointer<Click>`——加 over/out 是同一套
observer 机制的复制粘贴。

估计：~80 行 + 测试。

### 5. typed 组件全字段化

- `PbrMaterial` 当前 ~10 字段，应贴 `StandardMaterial` 全集（约 30 字段）。
- `Sprite2d` 还缺 `texture_atlas` / `rect` / `image_mode` / `anchor`。
- `Camera3d/Camera2d` 缺 `viewport` / `msaa_writeback` 等。
- 估计 300-500 行（机械添加）。
- 收益：AI 生成时能用上全部 Bevy 能力。

### 6. viewer / insert_vm_world 清理

- `viewer.rs` 里"缺相机就补 Camera3d"应该是个 plugin。
- "永远 spawn 一个 PointLight"也是。
- 估计 50 行。
- 收益：清理 + 可复用；任何 example 都能直接拿来用。

### 7. `BevyBridge` derive 宏（**推迟**）

- 设计与权衡分析见 `docs/proposal/bevy_bridge_macro.md`。
- 推迟原因：当前字段映射的总重复量不大；真正重复的是 sync 骨架。
  待 #5（typed 全字段化）落地、字段映射的复杂度爆发后再启动收益更明确。

### 扫雷剩余

- difficulty 切换面板 —— 等 #1 UI 桥
- 翻开闪光动画 —— 时间桥已就绪（`time()` / `delta()` 可用）

## Done

（首次落盘前已完成的大重构总结，便于追溯：）

- VM 独立 World + 双层组件（typed/dynamic）
- 配置 polyfill（多格式 + serde_json::Value 作 IR）
- 事件层（typed + dynamic + 双缓冲）+ Bevy Event 桥（in/out 解耦）
- VmPlugin 双侧抽象（builder + App）
- 输入桥（mouse/keyboard）+ picking 桥（observer 直通）
- 相机：`Camera3d` / `Camera2d`，对齐 Bevy `ScalingMode`
- 资源桥：`ResourceBuilder` + `CacheKey` + `ResourceCache`，
  builder 直接当字段值，cache_key 自动去重
- 拆 `Renderable` enum → `Sprite2d` / `Mesh3dRender` / `SceneRender`
  独立 typed 组件
- 三独立 sync system + 独立 EntityMap（修了"共享 map 互相 despawn"bug）
- `render` feature 改名 `bevy-bridge` 并默认开
- typed 组件 `requires(...)` 机制，对齐 Bevy 0.18 `#[require(...)]`：
  注册期由 builder 链式声明、注册期校验环、配置/脚本 set 自动连带
- 扫雷脚本对齐参考实现：chord（右键已揭开数字格自动展开）、
  首次点击不踩雷保护（点击格 + 8 邻居）、R 键重开整局
- `events()` 读端宽容：未注册通道返回空数组而非报错
  （让脚本可以监听 host plugin 提供的事件；headless 测试不挂 plugin 也能跑）
- typed 组件序列化全面切到 `bevy_reflect`（替代 `serde` 派生）。
  - 配套：所有颜色字段 `[f32; 4]` → `bevy_color::Color`（10 色彩空间 enum）
  - 配套：所有自定义 enum 从 internally-tagged `{kind: "..."}` 迁移到
    reflect 的 externally-tagged `{Variant: {...}}` 形态
  - `ComponentRegistry` 现持 `TypeRegistry`；嵌套字段类型也注册（包括
    bevy_color 全套 enum + 项目自家 builder 类型）
  - 决策与代价分析：`docs/proposal/reflect_serialization.md`
- 决定性 RNG（`rand` + `rand_chacha::ChaCha8Rng`）：
  - `WorldConfig.seed: Option<u64>` —— 配置可指定种子让脚本输出可重现
  - 脚本端暴露 `random()` / `random_range(min,max)` / `random_int(low,high)`
  - 扫雷的 LCG 手搓抽样换成 `random_int`
- 时间桥：直接复用 `bevy_time::Time<()>` 资源（不重造类型）。
  - VM 构建期 `insert_resource(Time::default())`
  - `VmWorld::advance_time(Duration)` host 端推进时间
  - 脚本 host 函数 `time() -> f64` / `delta() -> f64`，
    走 `Time::elapsed_secs_f64` / `delta_secs_f64`
  - viewer 路径 `tick_vm` system 自动转发主世界 `Time::delta`，
    忠实对齐主世界节拍
  - headless 测试自己控制 advance 步长——决定性
- UI 桥第一版（直接复用 `bevy::ui::*` 全套类型，零字段镜像）：
  - `ComponentRegistry` 注册 `Node` / `BackgroundColor` / `BorderColor`
    / `Outline` / `ZIndex` / `Button` / `Text` / `TextFont` / `TextColor`
    / `TextLayout` / `TextNodeFlags` 共 11 个 typed 组件
  - 嵌套字段类型 `Val` / `UiRect` / 全套 layout enum（Display / FlexDirection
    / JustifyContent 等）+ 文本类型（Justify / LineBreak 等）注册到 TypeRegistry
  - 加 `register_typed!` / `register_field_types!` 声明宏批量注册，
    名字默认取类型路径最后段
  - `render::sync_ui` system + `UiEntityMap` —— 暴力 clone+insert
    每帧整体复制 UI 组件到主 World 镜像；`sync_hierarchy` 整合 UI 实体
  - 优化路径见 `docs/proposal/ui_sync_incremental.md`
- 父子实体关系桥（直接复用 Bevy 0.18 的 `ChildOf` 关系组件）：
  - VM World 注册 `ChildOf` 组件让 ECS hooks 自动维护反向 `Children`
  - 脚本端 host 函数：`set_parent(child, parent)` / `clear_parent(child)`
    / `parent_of(e) -> Entity?` / `children_of(e) -> [Entity]`
  - 渲染端 `sync_hierarchy` system 跑在所有渲染 sync 之后，把 VM 端的
    `ChildOf(vm_parent)` 经合并的 EntityMap 翻译成主 World 的
    `ChildOf(render_parent)`
  - 自带 `linked_spawn` 语义（父亡子亡），符合场景图直觉

# bevy-vm 设计初稿

## 目标

让 AI 产出脚本 + 配置，驱动一个运行时可变的 Bevy World：动态 spawn 实体、动态 query/mut、按字符串路径反射读写属性，从而「动态创建新世界」。

## 核心模型：一个 World = 一个完备的交互世界

- 一组脚本 + 配置 = 一个独立的、可运行的交互世界，承载在一个独立的 `bevy_ecs::World` 上。
- World 之间**完全独立、数据隔离、零共享借用**。
- 跨 World 通信（若未来需要）只走网络/存储这种异步显式边界，世界内部永不直接互访。

这个边界把架构里最难的问题全部消掉：
- Bevy 编译期静态调度 vs 脚本动态访问 → **绕过**：脚本不参与主 App 的逐帧并行调度。
- 组内多 writer 访问冲突 → **缩到单 World 内**，用独占解释器串行解决。
- 组间并行的借用安全 → **免费**：World 间无共享借用，Rust 所有权天然保证。
- 跨组通信竞态 → **不存在**：通信走网络/存储异步边界。

## 并行策略：粒度是「整个世界」

- **把并行彻底移出 Bevy 的逐帧调度器。** 不让 AI 脚本变成被 Bevy 调度的动态 system。
- 并行发生在 **World 与 World 之间**，由管理层用任务池调度——因为 World 间无共享借用，这是 Rust 里最安全的并行点。
- 单个 World 在这里退化成一个**纯数据容器 + 自带 tick 能力的模拟单元**，由管理层完全掌控其 tick 时机。
- 有 N 个世界就能吃满 N 核，组内无需任何并行逻辑。

## 单 World 内部：独占解释器

- 一个持有 `&mut World` 的独占逻辑（exclusive system / 直接驱动），串行跑这一组的全部脚本。
- 脚本可任意 query / mut / spawn / despawn——因为它独占该 World，没有并发对手。
- 脚本访问集合是否动态**无所谓**：没人争用，Bevy 调度器不参与。
- 这是起点；若某个世界内部脚本多到串行 tick 都嫌慢，再单独给那个世界上「载入时编译为动态 system」做优化。这是优化，不是地基。

## 数据模型：两层值系统

组件分两层，分界线是「引擎内核 vs 游戏内容」而非「类型化 vs 动态」：

- **引擎层（类型化）**：`Position`、`Velocity` 等少数 `#[derive(Reflect, Component)]` 的 Rust 组件，只被静态热路径 system 访问，享受全速、无反射开销。经 `bevy_reflect` 的 `reflect_path` 按点号路径读写。
- **内容层（动态）**：游戏/AI 逻辑的组件（如 Health、Inventory、Item），运行时按名注册，值为 `ron::Value`，可由 AI 在配置里**自声明**。每个动态组件有独立 `ComponentId`，因而 `query` 仍按 archetype 分桶命中，不退化为线性扫描。

两层经**同一套点号路径 API**（`get/set/add/remove(entity, component, path)`）访问，脚本作者无需知道底层属于哪层。`ron::Value` 装任意嵌套数据；实体间引用以「存对方 entity id」表达，悬空 id 在访问时返回明确错误（裸奔契约），配 `is_alive` 判活。

- 配置定义世界**结构**（动态组件声明 + 实体 + 组件初值），脚本定义**行为**（组内可互相 query/mut/spawn/despawn）。

## 原语粒度：动词在「对一类实体做一件事」的高度

- 原语必须是**批量**的：脚本说「对所有带 Health 的实体每帧 -1」是一句声明，背后映射到一个预编译 system 完成整轮迭代。
- 解释器**永不进入逐实体紧循环**，始终待在热路径之外，避免解释开销淹没收益。

## 计算下沉：重活在静态侧

- **物体移动**（position/velocity 积分）、**数据型状态变更**（Health 递减、计时器）→ 下沉到预编译 Rust system，全速并行，脚本只设参数。
- **结构型状态变更**（状态机切换导致增删组件 / spawn / despawn）→ 收口到一个同步阶段（独占 World）。这是 ECS 固有的同步点，数据量通常小，串行不疼。

## 架构分层

```
┌─ 管理层 (Host / Orchestrator) ─────────────────┐
│  · 接收 AI 产出 → 校验 → 实例化成一个 World     │
│  · 持有 World 池,决定谁 tick、谁回收           │
│  · World 间零共享 → tick 丢任务池并行          │
│  · 对外通信只在这层(网络/存储)               │
└────────────────────────────────────────────────┘
        │ 每个世界完全独立、可并行
   ┌────┴────┬─────────┬──────────┐
World 1    World 2   World 3    ...
每个 = 一组 system + 配置,组成完备交互世界
  内部: 配置定义结构(两层组件 + 实体初值)
        脚本(Rhai)定义行为(组内可互相 query/mut/spawn/despawn)
        独占解释器串行 tick
```

## 行为：Rhai 脚本 system

- 行为单元抽象为 `System` trait，`ScriptSystem` 是其一种实现（未来可加预编译 Rust 行为等）。一个世界加载多个 system，按序 tick。
- 脚本即 `.rhai` 文件（与 `.ron` 配置分离），载入时编译成 `AST`，每 tick 复跑。
- 宿主函数通过「作用域裸指针」桥接 `&mut World`：安全性由独占 + 单线程同步执行不变量保证。
- 沙箱（故障隔离）：`set_max_operations` / `set_max_call_levels` 阻止死循环/无限递归拖垮世界；单世界失败可被捕获丢弃。

## 已落地

1. 引擎层类型化组件 `Position`/`Velocity` + 静态 `integrate_movement` 下沉。
2. 内容层动态组件：`register_component_with_descriptor` 注册 `ron::Value` 存储，unsafe 锁在 `component/dynamic.rs`。
3. 统一点号路径访问（`world_access` + `path` 导航），两层分发。
4. Rhai 脚本 system：`query/get/set/add/remove/spawn_entity/despawn/is_alive` 宿主函数，操作数沙箱。
5. 配置（RON）自声明动态组件 + 实体初值；脚本从独立文件加载。
6. 图形渲染层（见下），glTF 模型 + PBR 材质 + 变换，真实资产 demo。

## 图形渲染（`render` feature）

- **平行宇宙桥**：沙箱是独立逻辑 `World`，Bevy 主 World 持有渲染管线。同步层维护
  沙箱实体↔渲染实体映射，每帧增量同步（新增 spawn、存活更新 `Transform`、消失 despawn）。
- **`VmWorld` 作 NonSend 资源**：非 `Send`（`Rc` + Rhai），钉主线程，对单世界零代价
  （沙箱本就串行 tick）；其余 bevy system 照常多线程。
- **`Renderable` 渲染意图组件**，按 `kind` 分派：`cube`/`sphere`（程序图元）、`scene`
  （glTF，走 `SceneRoot`）、`mesh`（自定义网格）。
- **资产引用透传给 `AssetServer`**：字符串支持 `source://path#label`，来源（文件/网络/
  预加载）由 Bevy 的 asset source 注册机制承载，本层不做路径假设；相同资产串经
  `AssetCache` 去重只加载一次。
- **PBR 材质全套**：base_color、贴图、法线贴图、metallic、roughness、emissive、透明度。
- **变换**：`Position` + 可选 `rotation`（欧拉角度，AI 友好）/ `scale`，翻译进 `Transform`。
- 默认关闭 feature，避免日常 test/clippy 编译整个 bevy。

## 待定工程项（不影响可行性，正交于运行时架构）

- 多世界管理层 + 任务池并行 tick。
- 子 World 形态：裸 `bevy_ecs::World`（更轻、更可控，倾向此）vs `SubApp`。
- 故障隔离：单个世界 panic/载入失败不拖垮其他世界（catch + 丢弃该 World）。
- 动态组件 schema 校验：当前裸奔（运行时报错），需要时再加值类型声明。
- 悬空引用代管：当前裸奔（访问报错 + `is_alive`），需要时再上反向索引自动清理。
- 决定论 / 可重放：是否约束随机源与迭代顺序。

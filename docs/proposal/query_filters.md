# Proposal: 脚本侧 Query 过滤器

**状态**：草案，未实施。

Bevy 的 `Query<Q, F>` 第二参 `F` 是过滤器（`With<T>`、`Without<T>`、
`Or<(...)>`、`Changed<T>`、`Added<T>` 等）。我们的脚本目前只有
`query("Component")`——只有"包含某组件"一条过滤；其它语义在脚本端用
循环 + `if` 拼出来，慢且啰嗦。

## 现状的问题

典型代码：

```rhai
// 找"是 Cell 但不是墙"的格子
for cell in query("board::Cell") {
    let kind = get(cell, "board::Cell", "kind");
    if sp::is_undestroyable_char(kind) { continue; }
    // ...
}
```

每帧扫**所有** Cell（关 1 有 16×31 ≈ 200+ 个），逐个 `get` 字段、`if` 比较——
比 Bevy 内置的 `Query<Entity, (With<Cell>, Without<Wall>)>` 多花一个量级时间。

更严重的是**多组件过滤**：

```rhai
// 找"既是 Cell 又是 Tile 但不是 Robbo 站着的格"
for c in query("Cell") {
    if !has_component(c, "Tile") { continue; }
    if get(c, "Cell", "i") == pi && get(c, "Cell", "j") == pj { continue; }
    // ...
}
```

每个候选 entity 走 `has_component`（一次 HashMap 查 + 一次 entity 表查）+
两次 `get`——能用 archetype-level 的 Bevy `Query` 一次拉完的事被拆成 N 次
单点查询。

## 设计

仿 Bevy `Query<Q, F>`，把"过滤条件"作为 query 第二参传入。脚本端：

```rhai
// With：必须含组件
query("Cell", #{ with: ["Tile"] })

// Without：必须不含
query("Cell", #{ without: ["Wall"] })

// 组合：with + without 同时
query("Cell", #{ with: ["Tile"], without: ["Robbo"] })

// 字段值过滤（运行时 evaluator）
query("board::Cell", #{ where: #{ kind: "T" } })

// query_first 同样支持
query_first("Player", #{ with: ["Alive"] })
```

### 三类过滤器

1. **`with: [name, ...]`** —— 必须含的组件名列表。映射到
   `QueryBuilder::with_id(...)` 链。
2. **`without: [name, ...]`** —— 必须不含。映射到 `without_id(...)`。
3. **`where: #{ field: value, ... }`** —— 字段值精确匹配。**Bevy 没有原生
   过滤器对应**，要在 query 拉到 entity 后逐字段比较——但仍比脚本侧的
   `for+if` 快（绕过 Rhai 函数调用 + `Dynamic` 拷贝），且只比一次。

`where` 可选。简单实现：先扫 with/without 命中的 entity，再用 `where` 字段
过滤。第一阶段只做 `with` / `without`，`where` 留给第二步。

## API 形式

### 选项 A：单个 query 函数 + opts map

```rhai
query("Cell", #{ with: ["Tile"], without: ["Wall"] })
query("Cell")  // 兼容现有写法
```

每次调用都解析 opts map——慢，但和现有 API 形态一致。

### 选项 B：链式 builder

```rhai
query_builder("Cell").with("Tile").without("Wall").iter()
```

更像 Bevy。但 Rhai builder 模式需要返回不可变值（每个方法 `&self ->
new builder`），实现繁琐；脚本作者也难记。

### 选项 C：opt-in const-fold（类似 load_config）

```rhai
// 编译期固定，等价于 fold 后 query(handle_id)
query_with("Cell", "Tile")           // == query("Cell", #{with: ["Tile"]})
query_without("Cell", "Wall")        // == query("Cell", #{without: ["Wall"]})
```

把常用组合做成多函数，编译期可被 const-fold 框架直接索引化。但函数数量
爆炸（with×without×where 有 8 种组合）。

**倾向 A**：API 简单、向后兼容、和 `events`/`emit` 的"opts map"风格统一。
慢一点未来再优化。

## 实施细节

### 阶段 1：with / without

```rust
fn query_with_filter(
    world: &mut World,
    registry: &ComponentRegistry,
    component: &str,
    with_extra: &[&str],
    without_any: &[&str],
    vm_id: VmId,
) -> Vec<Entity>
```

走 `QueryBuilder::<Entity>::new(world).with_id(c).with_id(extra).without_id(no)`，
拉到 Vec 后逐个 `world.get::<VmTag>` 过滤（同当前实现）。后续做 [`script_hot_path_perf.md`]
里的 `QueryState` 缓存时一并优化。

### 阶段 2：where 字段过滤

`where: #{ kind: "T" }` 解析成 `Vec<(field_path, expected_value)>`，每个
候选 entity 用现有 `world_access::get` 取出字段值跟预期比。**只支持顶层
字段相等**（不做 `>` `<` 这种），保持简单——复杂逻辑用脚本端 `for + if`
继续写。

### `query_first` 也带 filter

```rhai
query_first("Player", #{ with: ["Alive"], where: #{ team: 1 } })
```

返回第一个匹配；零匹配返回 `()`。

## 验证

- 单测：headless 测每种组合、零结果、字段类型不匹配返回空
- demo 改造：minesweeper / robbo 里的"扫全表+if 过滤"循环替换 `query` 调用，
  目测帧时间下降
- 行为不变：旧的 `query("X")` 调用一字不改仍工作

## 不做的事

- **`Or<(...)>`** —— 组合 `with` 多名是 AND 语义；如果脚本作者要 OR，自己
  写两段 `for` 合并。Bevy 的 `Or` filter 是为类型组合设计的，脚本端用 string
  名做 OR 价值不大。
- **`Changed<T>` / `Added<T>`** —— change detection 跨 tick 状态，超出本提案
  范围。VM tick 流模型里"changed since last tick"语义不直观，后续真要做
  独立 proposal。
- **类型化字段比较** —— `where` 只支持精确相等；`age >= 18` 这种留给脚本端。

## 推荐节奏

1. **阶段 1 先**：with + without，覆盖 `for cell in query(...) { if has_component()
   continue; }` 模式
2. 看实际数据决定是否做阶段 2 `where`——可能在 [`script_hot_path_perf.md`]
   的 (B) cache + (C) `QueryState` 复用上后，原始 `query + 脚本 if` 已经够快

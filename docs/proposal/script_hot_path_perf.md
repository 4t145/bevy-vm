# Proposal: 脚本热路径性能优化

**状态**：草案，未实施。

每个 ScriptSystem 每 tick 调用大量 `query` / `get` / `set` / `has_component` /
`events` / `emit` host fn。这些调用目前都隐含**字符串 → 注册表查找**和
**每次现编 Bevy QueryState** 的开销。本文档梳理瓶颈与改进方案，按收益
从大到小给出落地顺序。

## 当前热路径剖面

以 `query("Player")` 为例：

1. `resolve_component_name(plugin_name, name, |n| registry.resolve(n).is_some())`
   - 至少 2 次 `HashMap<String, _>` 查找：先查 `<plugin>::<name>`，
     再查全局 `<name>`
   - 每次查找有 `String` 拼接（`format!("{plugin}::{name}")`）
2. `world_access::query_with_component_tagged` 又调一次
   `registry.resolve(name)` 拿 `ComponentId`
3. `QueryBuilder::<Entity>::new(world).with_id(component_id).build()`
   现编一个 `QueryState`——Bevy 自家代码会跨 tick 复用
   `QueryState`，我们没有
4. `query.iter(world).collect::<Vec<Entity>>()` 全量拉取
5. 然后**逐 entity** `world.get::<VmTag>(entity)` 做归属过滤

`get` / `set` 同样的字符串解析 + path 查找，在每 tick 几十次到几百次。

## 改进路线

按 ROI 从高到低排序：

### A. resolve 缓存：把字符串解析做一次（`ScriptSystem` 编译期）

`ScriptSystem::compile` 时遍历 Rhai AST，挑出所有
`query("Player")` / `get(e, "X", "y")` 这类字面量字符串实参，
**编译期一次性 resolve** 到
`(qualified_name: Rc<str>, ComponentId)` 缓存。运行时遇到字面量
直接 lookup `&str → ComponentId`，零字符串拼接、零 HashMap 查询。

字符串实参非字面量（变量 / 表达式生成）的情况仍走 fallback。

**工程量**：中等。Rhai AST 有遍历 API。
**预期收益**：消除 90% 的 resolve 调用——demo 里几乎所有 component
名都是字面量。

### B. 运行期 LRU/HashMap cache（fallback 路径）

`ScriptSystem` 内 `RefCell<HashMap<String, ResolvedComponent>>`，
第一次 resolve 后存进去；后续相同 short name 直接 hit。

不依赖 AST 分析，覆盖**变量 / 动态拼接**的字符串实参。和 (A) 互补：
(A) 处理字面量 fast-path，(B) 处理动态 fallback。

**工程量**：小。
**预期收益**：第一帧热身后稳定；变量参数也能命中。

### C. 复用 `QueryState`：cache by `ComponentId`

每个 ScriptSystem 持一个 `RefCell<HashMap<ComponentId, QueryState<Entity>>>`，
避免每次 `QueryBuilder::build()`。Bevy 的 `QueryState` 内部维护
archetype indices，第二次 `iter` 同一个 state 几乎是 O(matched archetypes)。

需用 dynamic query API（`QueryBuilder::<Entity>::new()` 配 `with_id`）；
返回的 `QueryState<Entity>` 是单态，可以直接缓存。

需注意 `QueryState` 在 archetype 增减时 Bevy 自己会更新（`update_archetypes` /
`as_readonly` 等），缓存层不主动失效。

**工程量**：中等。
**预期收益**：消除每次 `QueryBuilder::build()` 的初始 archetype 配对开销。

### D. Tag 过滤改成 query 端查值

当前 `query_with_component_tagged`：query 出所有带组件的 entity，
**然后** `world.get::<VmTag>(entity)` 逐个看 `tag.vm == vm_id`。

Bevy 没有"按 component 字段值过滤"的 query。取舍方案：

1. **保持现状**：值过滤在 client 侧——`QueryState` 一次性把 (Entity, &VmTag)
   一并拉出来，少一次 `world.get`。
2. **per-VM marker**：给每个 VM 注册一个 ZST 组件 `VmOwnerN`，作为
   `with::<VmOwnerN>()` 的 filter 条件。冲突分析友好；代价是注册时机
   要早于第一次 spawn。

倾向方案 1——简单且足够好。

**工程量**：小。
**预期收益**：每个 query 少一次 entity-by-entity HashMap 查找（`get::<VmTag>`
内部走 ComponentSparseSet）。

### E. 避免 `iter().collect::<Vec>()` 的反复分配

`query` 结果落进 `Vec<Entity>` 给脚本——每次 query 都是一次堆分配。
脚本几乎都立即扫一遍这个 Vec。

可做的：thread-local 或 ScriptSystem 持有的 reusable `Vec<Entity>` 缓冲。
脚本拿到的 Rhai `Array` 还是新构造的，但可以基于复用的 `Vec` 一次性
build，省掉中间拷贝。

**工程量**：小。
**预期收益**：降低 GC / alloc 压力，对帧率影响小但累积明显。

### F. path 解析预编译

`get(e, "Player", "i")` 第二个字段名同样每次解析路径
（`"a.b[2].c"` 这种）。可参考 (A) AST 预编译，对路径字面量做一次
PathSegments 切分缓存。

**工程量**：中等。需理解当前 `path_get` / `path_set` 内部表示。
**预期收益**：取决于 `set` / `get` 调用密度——在 alien_cake_addict 这种
demo 里非常密集。

## 推荐落地顺序

1. **先做 B**（运行期 cache）：单点改动覆盖 `query` / `get` / `set` /
   `has_component` / `events` / `emit`。一次性显著降低字符串处理频次。
2. **测一下**：用 minesweeper / alien_cake_addict 跑 60s + frame timing。
   有具体数据再决定下一步。
3. **B 不够再做 C**（QueryState 复用）：第二轮收益主要在 query 上。
4. **A、D、E、F** 留到 profile 数据指向具体瓶颈再做。

## 不做的事

- **多线程并行 ScriptSystem**：见
  [`schedule_concurrency.md`](./schedule_concurrency.md)（待写）。
  并行需要先做声明式访问意图。本文只关心 single-thread 的 micro 优化。
- **JIT / AOT 编译 Rhai**：Rhai 自身有解释器开销，但更换运行时（如
  改用 Lua / wasm）属于大改，不在本提案范围。

## 验证方法

每个阶段落地后：

1. **行为不变**：`cargo test --workspace --all-targets` 全绿。
2. **性能提升**：写一个 micro-benchmark world——`query("Tile")` × 100 +
   `get(e, "Tile", "x")` × 1000。在改动前后跑同一 seed，对比每帧耗时。
3. **回归保护**：bench 进 `tests/` 或独立 bench crate（criterion），
   防止后续改动悄悄退化。

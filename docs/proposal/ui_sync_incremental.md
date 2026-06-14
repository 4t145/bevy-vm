# Proposal: UI sync 增量优化

**状态**：第一版 `sync_ui` 走暴力 clone+insert。本文记录后续优化路径，
帧时间数据出来后再决策是否启动。

## 第一版策略

每帧对每个 VM 端 UI 实体：

```rust
let node = vm_world.entity(vm_e).get::<Node>().clone();
commands.entity(render_e).insert(node);
// ... 同样的逻辑 for BackgroundColor / Text / TextFont / ...
```

无脏检测，无 hash 比对，每帧整体覆盖。

## 已知开销来源（按从大到小排）

1. **Bevy UI layout 重算**（taffy）—— 我们无法影响。100 节点典型 ~100 μs，
   1000 节点 ~1 ms。
2. **Change detection 误触发** —— 每帧 `insert` 让所有节点变 `Changed<Node>`，
   layout 系统每帧全量算（即使 VM 端字段没变）。
3. **archetype 迁移** —— 第一次 spawn 镜像有迁移；后续是同 archetype 内
   覆盖，便宜。
4. **Component clone** —— `Node` 含 30+ 字段（含 `Vec` / `enum`），
   ~200-500 ns/clone。

## 触发优化的条件

加 `bevy::diagnostic::FrameTimeDiagnosticsPlugin` 跑 demo，若任一为真则启动：

- sync_ui system 自身耗时 > 500 μs/帧
- demo 帧时间 > 5 ms 且 sync_ui 占 > 30%

否则不优化——优化是有成本的，无收益时引入复杂度只让代码更脆。

## 候选方案（按工程量从小到大）

### A. Hash gating（仅 sync 改动，不动 VM 端）

镜像实体上挂 `LastUiHash`，存上次 sync 后的 Node 字段 hash：

```rust
fn hash_node(node: &Node) -> u64 {
    let mut h = DefaultHasher::new();
    // 手写 hash impl 因为 Node 含 f32（用 to_bits）
    node.display.hash(&mut h);
    node.width.to_bits().hash(&mut h);  // 等等
    h.finish()
}

let new_hash = hash_node(&vm_node);
if last.0 != new_hash {
    commands.entity(render_e).insert(vm_node);
    last.0 = new_hash;
}
```

**收益**：稳态 HUD 零 archetype churn / 零 change detection 误触发。
**代价**：~50 行手写 hash impl，新增组件类型时要扩。
**风险**：hash collision 概率极低，但出现就是个 silent bug；
配 debug build 校验或宽容 trade-off。

### B. Changed query 走 VM 端

VM 内部用 `Changed<Node>` query 决定要 sync 哪些。`ChangeTicks` 在 VM World
内本来就维护——只要 sync 系统能访问 query，能拿到。

但当前 sync 是 NonSend resource 路径（VM 持 raw `&mut World`，由 `tick_vm`
独占），不是 Bevy `App` 的 system，没有 `Query` SystemParam。要改：
让 sync 接 `&mut World` 后用 `world.query_filtered::<Entity, Changed<Node>>()`
内部 query。

**收益**：和 A 类似，但增量颗粒度更细。
**代价**：每个 sync 路径都要重写 query 形态；和现有 sync_sprites/meshes
统一性变差（它们没用 Changed 是因为字段映射有派生项 = build_transform，
不能直接靠 ECS 的 ChangeTicks 判断）。

### C. 字段级 diff（仅在差异时 insert）

```rust
let render_node = render_world.entity(render_e).get::<Node>();
if render_node != Some(&vm_node) {
    commands.entity(render_e).insert(vm_node);
}
```

**收益**：和 A 等价，避免 hash collision 问题，但每帧多一次主 World 读 + 比较。
**代价**：两次 World 借用（VM 读 + 主 World 读）；且 `Node` 没派生 `Eq`（含 f32），
比较语义要自定义。

### D. 双阶段：脏标记 + 批量 commit

VM 端 `set` 路径自动给该实体打个 `Dirty<Node>` 标记。sync 时只看 `Dirty`，
sync 完清。

**收益**：理论最优，零冗余。
**代价**：脏标记机制要侵入 `world_access::set` 路径；新增 typed 组件
都要登记 `Dirty<X>`；和"脚本 set 字段是普通 ECS 写"的简单心智冲突。

## 我的预判

如果优化时机来了，**先做 A**（hash gating）。原因：

- 改动局部，sync 层内部，不污染 VM 端 / 脚本端
- 收益已经覆盖最大头（archetype churn + change detection 误触发）
- collision 概率（u64 hash）在游戏 UI 场景下可忽略，debug 校验兜底

D 是"理论最优"但侵入性最大，留作 A 不够时再考虑。

## 触发后的最小验证

加优化后必须有这两个数据点对比：

- sync_ui 自身耗时 before / after（Bevy `SystemMetrics` 或自加 `Instant::now()`）
- 100 节点稳态 demo 的 99 percentile 帧时间

无显著改善就回退——避免引入"看起来对但没收益"的复杂度。

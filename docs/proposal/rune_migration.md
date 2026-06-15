# Proposal: 评估 Rhai → Rune 迁移

**状态**：草案，未实施。pre-spike 计划。

## 出发点

Rhai 用了大半年，痛点逐渐清晰：

- **类型系统弱**：`query("Tile")` 字面量打错、`get(e, "Player", "i")` 字段不存在 —— 都得跑到运行时才报。我们靠 const-fold + 即将做的 AST lint 缓解，但**根本是动态语言**。
- **AST 解释器**：[script_hot_path_perf.md](./script_hot_path_perf.md) 估的"每帧重走 AST"是底层成本。Rune 是栈式 VM，**run 期** 5-10× 数值密集 / 2-3× host-fn 密集。

但换语言不是免费——**实质要重写脚本侧 + 整个 host fn 桥**。这份提案的目的：在动手前划清"换的代价"和"换的收益"，给 spike 一个明确的验收标准。

## Rune 执行模型对照（决策上下文）

迁移前先把两边的对象映射讲清楚——之后讨论 host fn / 状态桥时不至于
对错位。

| Rhai | Rune | 角色 |
|---|---|---|
| `Engine` | `Context` + `Arc<RuntimeContext>` | host fn 注册 + 全局配置（共享，长寿） |
| `AST` | `Arc<Unit>` | 单脚本编译产物（共享，长寿） |
| `Scope`（execute 时建） | `Vm`（execute 时建） | 单次执行的栈 + 寄存器 |

要点：

- **`Vm::new` 是轻的**：只是 `Stack + 几个 usize`，每帧每 ScriptSystem 现建
  没问题。host fns 不会被重新注册。
- **`RuntimeContext` 跨 ScriptSystem 共享**：所有 ScriptSystem 共用一份
  `Arc<RuntimeContext>`——比 Rhai 现状（每个 ScriptSystem 新建 Engine 重复
  注册 ~50 个 host fn）启动开销低。
- **`Unit` per-script**：跟 Rhai `AST` 一对一对应。
- **状态注入需重新设计**：每帧 `Vm::new` 时通过 `Vm::call_with_args` 或
  `Any` 包装传 `*mut World` 等。我们 Slots 的 raw 指针桥不能照搬——
  Rune 的 lifetime 模型不同。

迁移后 ScriptSystem 大概长这样：

```rust
struct ScriptRuntime {                 // 单实例，per-VM 或 per-VmRegistry
    runtime: Arc<RuntimeContext>,
}
struct ScriptSystem {
    runtime: Arc<RuntimeContext>,      // 引用共享
    unit: Arc<Unit>,                   // per-script bytecode
    // Vm 每 tick 现建，不持有
}
impl System for ScriptSystem {
    fn run(&self, ctx: &mut TickContext) -> Result<...> {
        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        // 注入 World/Events/... 到 vm context
        vm.execute(["main"], ())?...
    }
}
```

## 不确定的关键技术点

下面 8 项是 spike 必须验证的——任何一项过不了就退出。

### 1. Host fn 注册迁移

| Rhai | Rune |
|---|---|
| `engine.register_fn("query", \|name: &str\| -> Vec<Dynamic> { ... })` | `module.function(["query"], \|name: &str\| -> Vec<Value> { ... })` |
| 同一 `Engine` 注册全部 fn | `Module` 收集 fn → `Context::install(module)` |

我们大约 50 个 host fn——纯翻译工作量 ≈ 1-2 天。**风险**：Rune 对 fn signature 有限制（rune-rs 0.13 时不能完全任意 trait bound），少量 fn 可能要 wrapper。

### 2. Slots（raw `*mut World/Events/RNG/Time/Pause`）

我们当前靠 raw 指针 + `Rc<Cell<*mut _>>` 把 `&mut World` 桥进 'static rhai 闭包。Rune 的 host fn 闭包同样 'static，**桥设计可以直接搬**——RefCell + raw pointer 的 trick 不依赖语言。

风险：Rune 是否允许 host fn 在执行中 panic 时安全 unwind？需要测。

### 3. Const-fold（`on_parse_token`）

**最不确定的一项**。Rhai 的 `internals` feature 给我们 token-level hook，把 `load_config("path")` 重写成 `load_config(handle_int)`。

Rune 没找到等价 API：
- `rune::compile` 编译流程更封闭
- 有 `Diagnostics` 和 `Sources` 但**没看到 token mapper hook**
- 替代方案：source 字符串预处理（自己写 mini lexer，不依赖 Rune 内部）

**spike 必测**：能否用 Rune 公开 API 实现"`load_config` 字面量参数 → 整数句柄"。

### 4. 模块系统

| Rhai | Rune |
|---|---|
| `import "helpers" as h; h::px(10.0)` | `use helpers; helpers::px(10.0)` |
| `set_module_resolver(FileModuleResolver::new_with_path(dir))` | `Sources::insert(Source::with_path(file_path))` 多 source |

需要验证：
- 同名 helper 在不同 module 里 import 是否各自独立（Rhai 是；Rune 是？）
- helpers 能否定义跨 fn 共享常量（如 `fn px(v) { ... }`）

### 5. Dynamic ↔ Value

- Rhai `Dynamic`：动态分发 + clone-on-pass
- Rune `Value`：内部是 RefCell-like 的 Shared，clone 成本不一样

我们 host fn 大量在 `Map<String, Dynamic>`（如 `attach_sprite(e, #{ image: ..., atlas: ... })`）和 `Vec<Dynamic>`。Rune 等价是 `Object` 和 `Vec<Value>`。spike 测：嵌套 map 解构 + array 索引 + map.get(...) 三个常用 pattern。

### 6. 脚本端语法迁移

```
Rhai:                              Rune:
let games = query("Game");        let game = query_first("Game");
if games.len() == 0 { return; }   if game.is_unit() { return; }
let game = games[0];

for ev in events("Pick") { ... } for ev in events("Pick") { ... }   // 大概率 OK

#{ width: percent(100.0), ... }   // Rune 用 anonymous struct？object literal？

`text ${var}`                     // Rhai interpolated string；Rune 也支持

import "helpers" as h;            use crate::helpers;  // ?
h::px(10.0)                       helpers::px(10.0)
```

**workload 估计**：robbo / minesweeper / alien_cake 三个最大 demo 的脚本总行数 ≈ 1500，全部翻译 ≈ 1 天，但前提是上面 5 项都通了。

### 7. 字符串 path 仍是动态

**这是 Rune 救不了的痛点**。`get(e, "Player", "i")`——Player / i 必须运行时查 ECS 注册表。换 Rune 后：

- `query` / `get` / `set` / `events` / `emit` 仍走 string → ComponentRegistry 查找
- 可能从 Rune 拿到强类型的 `f64`/`String` 参数（少一次 Dynamic 解码），但**主成本在 ComponentRegistry 查找**

类型救不了 ECS 字符串接口——除非把 ECS 接口改成 codegen（每组件一个强类型 host fn）。**这是另一份 proposal 级的工作**，不在 Rune 迁移范围。

### 8. AI 生成质量

Rhai 模仿 JS/Rust 语法，公开训练语料中"Rhai-like" 多。Rune 是小众语言——AI 写 Rune 出错率显著更高，对一个**核心场景就是 AI 生成代码**的项目是真痛点。

**spike 不必测**：找几个公开 Rune 例子让 AI 写一段类似的，主观判断。

## 收益估算

如果 spike 全过、迁移完成：

| 维度 | 预期改善 |
|---|---|
| 帧时间（数值密集 system） | 5-10× faster |
| 帧时间（host-fn 密集，主流） | 2-3× faster |
| 编译速度（一次性，load 时） | 慢一点（Rune 编译做更多事），无所谓 |
| 类型错误捕获 | 字面量错可编译期发现（仅 Rune 静态可见的部分；ECS 字符串错仍运行时） |
| AI 生成质量 | **下降**（Rune 语料少） |

## 性能 vs 类型：换 Rune 解决哪个问题

下面这条**关键判断**决定要不要做这次迁移。

### "换 Rune 解决性能"是常见误判

我们当前热点剖面（[`script_hot_path_perf.md`](./script_hot_path_perf.md)）：

| 单次操作开销 | Rhai 现状 | 换 Rune 后 |
|---|---|---|
| 解释器跑一行 | ~0.1µs | ~0.01µs（**10× 改善**，但本来就不多） |
| `query("Tile")` 字符串解析 + HashMap | 数 µs | **不变**（host fn 内） |
| `get(e, "Tile", "flag")` reflect path 解析 | 数 µs | **不变**（host fn 内） |
| `QueryBuilder::build` 现场建 | 数 µs | **不变**（host fn 内） |
| `Dynamic` 包装/拆包 | 数百 ns | **不变**（仍要跨 host 边界） |

**minesweeper 一帧扫 200 个 cell** 做 `get(e, "Tile", "flag")`：

- 200 × 0.1µs = 20µs 在解释器
- 200 × 数 µs = **数百 µs** 在 host fn 边界

换 Rune 后解释器那 20µs 降到 2µs——**节省 18µs，不影响体感**。
真正的性能路径是在 host fn 边界，跟换 VM 关系不大。

### 三档峰值（理论）

| | 解释器开销 | 字节码 | JIT/Native |
|---|---|---|---|
| Rhai | 高 | 无 | 无 |
| Rune | 中 | 有 | 无 |
| WASM | 低 | 有 | 有 |

理论 Rhai → Rune **3-10×**，Rune → WASM 再 **3-10×**，
**但都只在解释器自身那一档**——host fn 边界仍是瓶颈。

### 结论：换 Rune 的真正卖点是类型，不是性能

如果你现在体感"脚本慢"：

1. **先 profile**：`query` / `get` / `set` host fn 加 tracing，
   跑 60 秒 minesweeper，确认是不是 host fn 边界占主要时间
2. **做 resolve cache**（[`script_hot_path_perf.md`](./script_hot_path_perf.md) (B) + (C)）—— 小改动、ROI 5-10×
3. **再判断是不是要换 VM**

如果你想**换 Rune 解决类型不满意**——那是独立目标，按下面 spike 节奏来。

如果**真要榨到接近 native**——直接走 WASM，别在 Rune 中间停。WASM 也得先把 host fn 边界做对（marshalling / 跨 linear memory）。

---

**一句话**：性能不靠换 VM，靠把 host fn 边界做对。Rune 迁移的合理理由
**只有"想要更强的类型系统"**——这本身值不值，由你权衡 AI 生成质量损失。

## 风险

1. **AI 生成质量下降**抵消性能收益（VM 的核心场景就是 AI 创作）
2. const-fold 没替代 → 性能优化路径变窄
3. 迁移过程中 demo 全部需要一次性切换（Rhai 和 Rune 不能在同一 VM 共存）—— ~1500 行脚本工作量
4. const-fold proposal 已落地，再切语言相当于丢掉这部分投资

## Spike 验收清单

按优先级：

1. **Const-fold 等价路径**（不通过 → 立即放弃换语言）
   - `load_config("levels/01.json")` 编译期能换成 `load_config(0)`
   - 如果只能走 source 字符串预处理，可接受
2. **Host fn + raw `*mut World` 桥**
   - 注册 5 个 fn（spawn_entity / despawn / query / get / set）
   - 跑 100 次循环不 leak / 不 UB
3. **嵌套 map 在 host fn 边界**
   - `attach_sprite(e, #{ image: "x", atlas: #{ tile_size: [32, 32], index: 5 } })` 能完整传递
4. **module / import**
   - 多文件脚本，A `use B; B::helper()` 能跑
5. **`Vm::execute` 性能**
   - 1000 次 host fn call + 1000 次纯计算，对比 Rhai 同量级脚本
6. **错误处理**
   - host fn 返回 `Err`，脚本拿到的是带 backtrace 的诊断信息

## 执行计划

**阶段 0（半天，本 proposal）**：写完此文档，对齐验收标准 ✓

**阶段 1（半天，spike）**：
- 新分支 `rune-spike`
- 不动主代码，独立 crate `tools/rune-spike`
- 翻译 alien_cake 的 `bonus.rhai`（cake 旋转 + 5s 刷新）作为 Rune 脚本
- 满足上面 6 条验收清单

**阶段 2（决策点）**：
- spike 全过 → 写"完整迁移 proposal + 分阶段计划"
- 任意一项不过 → 写"why we're staying on Rhai"，归档作为反面案例

**阶段 3（如果继续，估 1-2 周）**：
- 把所有 host fn 翻译到 Rune
- 把 robbo / minesweeper / alien_cake 三个 demo 的脚本翻译到 Rune
- 对比 60s 帧时间数据
- 决定是否把 default flavor 切到 Rune

## 不在范围

- **WASM 路线**：单独 proposal。WASM 解决类型问题更彻底但 host-fn marshalling 更重，是另一种权衡。
- **完全 codegen ECS 接口**：把 `get(e, "Player", "i")` 改成强类型 fn 的 codegen 工作，无论 Rhai 还是 Rune 都可做，但属于另一份 proposal。
- **Rhai 内 AST lint pass**：是 Rhai 体系的渐进改进，不需要切语言。建议先做这个看效果再决定要不要换 Rune。

## 推荐顺序

1. **先做 Rhai 的 AST lint pass**（半天，已规划在 [script_hot_path_perf.md] (A)）
2. **看效果**：如果 lint pass 解决了 80% 的"类型不满"，**就不用换** Rune
3. 还是想换 → 跑 spike

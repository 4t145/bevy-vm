# Proposal: Reflect 序列化路（R1）

**状态**：spike 完成，决策待定。

spike 测试在 `tests/reflect_spike.rs`，7 个用例全过。
本文档记录 spike 实测结果与全面切换的代价。

## 动机重述

Bevy 0.18 的内置组件（特别是 `bevy_ui`、widget、相机、`Mesh3d` 等）：

- **几乎全员**派生 `Reflect`
- **约一半**派生 `Serialize`/`Deserialize`（`bevy_ui` 的 30+ 类型有，
  `Text`、`Button`、`Interaction` 等 widget 类型没有）

VM 当前的 `TypedComponent` 要求 `T: Serialize + Deserialize`——
这卡掉了"VM 直接复用 Bevy 类型"。改成 reflect 路径后，UI 层、
任何带 Reflect 的 Bevy 组件都能被 VM 直接挂。

## Spike 实测结论

### ✅ 普通 struct 完美兼容

```json
ReflectPosition { x: 1.0, y: 2.5, z: -3.0 }
→ {"x":1.0,"y":2.5,"z":-3.0}
```

与现状 `serde_json::to_value` 输出**字节相同**。

`Sprite2d` 的 `Option<T>`、`[f32; 4]`、`bool` 等也都一致。

### ❌ Enum 形态变更（核心痛点）

| 形态 | 当前（serde internally tagged） | reflect 输出 |
|---|---|---|
| Unit variant | `{"kind": "window_size"}` | `"WindowSize"` |
| Struct variant | `{"kind": "fixed", "width": 1280.0, "height": 720.0}` | `{"Fixed": {"width": 1280.0, "height": 720.0}}` |

差异点：
1. **PascalCase variant 名**——reflect 不支持 `rename_all = "snake_case"`
2. **Externally tagged**——`{"VariantName": {...}}` 而非 `{"kind": "...", ...}`
3. Unit variant 是裸字符串

### ✅ `ReflectComponent` 装回 ECS 干净

```rust
let registration = registry.get(TypeId::of::<T>())?;
let deserializer = TypedReflectDeserializer::new(registration, &registry);
let boxed: Box<dyn PartialReflect> = deserializer.deserialize(json)?;
let reflect_component = registration.data::<ReflectComponent>()?;
reflect_component.insert(&mut entity_mut, boxed.as_ref(), &registry);
```

读回则是反向：直接 `world.entity(e).get::<T>()` 用强类型读，或者用
`reflect_component.reflect(...)` 走 dyn 路径再 `TypedReflectSerializer::new`。

### ⚠ 几个工程细节

- **必须 `register_type::<T>()`**，否则 `registry.get(TypeId::of::<T>())` 返回 None
- **必须派生 `#[reflect(Component)]`** 才能拿到 `ReflectComponent` metadata
- **必须派生 `#[reflect(Default)]`** 才能从 registry 取默认值
- `ReflectSerializer`（带 wrapper 形态）的 key 是**完整 Rust 路径**
  （`my_crate::module::Type`），不能当组件名 key 用——仍要自己持
  `name → TypeRegistration` 映射

## 全面切换的工作量估计

### 必改的代码

1. **`TypedComponent` 的 vtable**（~80 行）
   - `insert_default`：从 `T::default()` 改成 `registry.get(...).data::<ReflectDefault>().unwrap().default()`
   - `insert_from_value` / `write_value`：走 `TypedReflectDeserializer`
   - `read_value`：走 `TypedReflectSerializer`
   - 函数指针 vtable 可以保留，body 切到 reflect 路径
   - trait bound：`T: Component + Reflect + FromReflect + GetTypeRegistration + TypePath + Default`

2. **`ComponentRegistry` 持有 `TypeRegistry`**（~30 行）
   - 新增字段；register_typed 时调 `type_registry.register::<T>()`
   - 反序列化时把 registry ref 透传到 reflect 路径

3. **VM 端所有 typed 组件加 `#[derive(Reflect)]` + `#[reflect(Component, Default)]`**
   （~10 类型 × 2 行 = 20 行，机械改动）

4. **去掉所有 `#[serde(tag = "kind", rename_all = "snake_case")]`**
   - `OrthoScalingMode`、`CameraProjection`、`MeshBuilder`、`MaterialBuilder`、
     `ImageBuilder` 五处
   - 同时去掉 `Serialize`/`Deserialize` 派生（reflect 不需要）

### 必改的配置（迁移成本）

7 个配置文件含 `kind:` 字段，每文件 1-3 处：

| 文件 | 涉及 |
|---|---|
| `examples/worlds/drag.ron` | mesh.kind, material.kind, projection.kind |
| `examples/worlds/gallery.ron` | mesh.kind, material.kind ×N |
| `examples/worlds/minesweeper.ron` | scaling_mode.kind |
| `examples/worlds/object_viewer_3d.ron` | mesh.kind, material.kind, projection.kind |
| `examples/worlds/orbit.ron` | mesh.kind, material.kind ×N |
| `examples/worlds/primitives.ron` | mesh.kind, material.kind ×N |
| 其它 | 各 1-2 处 |

迁移示例：
```ron
// 旧
mesh: (kind: "cube", size: [1.0, 1.0, 1.0])

// 新
mesh: { Cuboid: (size: [1.0, 1.0, 1.0]) }
```

每文件 5-15 分钟，全量 ~1.5 小时。

### 错误质量降级

reflect 错误："invalid value: map, expected map with a single key"——
没有 serde 派生那种"missing field `width`"那种精确反馈。
对 AI 写配置不算友好。

可缓解：包一层错误转换，把 reflect 的 dyn error 翻译成更具体的消息
（带组件名、字段路径）。但成本不在 spike 范围内估计。

## 风险与对冲

### 风险 1：`Handle<T>` / `Box<dyn ResourceBuilder>` 等"运行时句柄"字段

`PbrMaterial` 等组件**不**直接持 `Handle`，持的是 `MaterialBuilder` enum。
`MaterialBuilder` 内部分支也都是普通字段（路径、颜色、数）。

✅ **无风险**——VM 端 typed 组件设计上就避开了运行时句柄。

### 风险 2：第三方 typed 组件（如果将来用）

未来想用某个 Bevy 生态库的 `Component`：
- 该 lib 派生 Reflect → ✅ 可直接用
- 该 lib 没派生 Reflect → ❌ 我们包一个 newtype 里加 `#[derive(Reflect)]`

这比"自己重写 Bevy 组件字段集"还是省得多。

### 对冲：保留 serde 双轨？

**不推荐**。两套路径 = 两套测试 + 两套错误处理 + 让人不知道用哪个。
要切就切干净。

如果担心一次切完出问题，可以**先在分支上跑通**，merge 前做完整 examples 跑通。

## 推荐路线

**全面切**，分两个 PR：

1. **PR-A：核心机制**
   - `TypedComponent` 切到 reflect
   - `ComponentRegistry` 持 `TypeRegistry`
   - VM 现有 typed 组件加 Reflect 派生
   - 配置文件迁移
   - 现有所有测试通过

2. **PR-B：拓展**
   - 把 Bevy `Node`、`Text` 等 UI 组件直接 register 进 typed map
   - 配套父子关系桥（`ChildOf` / `Children`）
   - 这是后续"UI 层"的入口

## 不切的 alternative

如果决定**不**切 reflect：
- UI 层走"VM 自家镜像 Bevy 字段"路径（重新定义 `UiNode` 等）
- 工作量约 500-800 行（取决于覆盖多少组件）
- 持续维护成本：每次 Bevy UI 字段变化要手动跟进

切 reflect 是一次性投资约 200 行核心 + 配置迁移；
不切是反复支付字段同步的运维成本。

## 我的建议

切。

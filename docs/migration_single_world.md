# 迁移到 Single-World 架构

## 大改概览

VM 不再持自有 `bevy_ecs::World`。每个 `VmInstance` 以 `&mut World` 接收宿主提供的 World，所有它 spawn 的实体直接住在那里——主 Bevy 世界。`VmTag(VmId)` component 标记实体属于哪个 VM；多个 VM 通过 `VmRegistry` (NonSend resource) 共存。

废除的层：

- `render.rs` 的整个 sync 桥接（`SpriteEntityMap` / `MeshEntityMap` / `SceneEntityMap` / `CameraMap*` / `TextMap` / `UiEntityMap` / `despawn_missing` / `sync_hierarchy` / 所有 `sync_*` 系统）
- VM-typed 视觉组件：`Position` / `Rotation` / `Scale` / `Velocity` / `Mesh3dRender` / `Sprite2d` / `SceneRender` / `TextLabel` / `Camera3d` / `Camera2d`
- `VmEntityRef`（不再需要——entity_id 在 VM 与主 World 共享）

新增的：

- `crate::vm_id::{VmId, VmTag}`
- `crate::VmRegistry`
- `bevy_vm::render::insert_vm_instance(app, vm)` / `despawn_tagged_entities(world, vm_id)`
- 脚本端 host fn：`attach_mesh / attach_pbr / attach_sprite / attach_camera_3d / attach_camera_2d / attach_text / attach_scene / set_transform / set_translation / set_yaw / get_translation`
- UI helpers 下沉为 host fn（无需脚本侧 `helpers.rhai` 重定义）：`srgba(r,g,b,a)`、`srgb(r,g,b)`、`px(v)`、`percent(v)`

## 旧 world 的迁移模板

旧版 world.ron：

```ron
(components: [...],
 entities: [(components: {
    "Position": (x: 0.0, y: 0.0, z: 0.0),
    "Mesh3dRender": (mesh: { "Sphere": (radius: 0.5) }, material: { "Pbr": (...) }),
    "Spin": (speed: 0.5),
 })],
 systems: [Script(path: "spin.rhai")])
```

新版：

1. 删掉 entities 段里的视觉 typed 组件，把视觉描述的字段塞进自家 dynamic 组件（`Spin` 或新建 `Setup`）。
2. 加一个 `setup.rhai` 第一帧 attach；用 `done` 单例标记防止重复挂。
3. 行为脚本（spin.rhai 等）用 `set_yaw / set_translation / set_transform` 写 Bevy `Transform`。

```ron
(components: [
    (name: "Spin", default: (speed: 0.0, color_r: 1.0, color_g: 0.0, color_b: 0.0,
                              shape: "Sphere", x: 0.0, yaw_deg: 0.0)),
    (name: "Setup", default: (done: 0)),
],
 entities: [
    (components: { "Spin": (speed: 0.5, x: -2.0, ...) }),
    (components: { "Setup": (done: 0) }),
 ],
 systems: [Script(path: "setup.rhai"), Script(path: "spin.rhai")])
```

```rhai
// setup.rhai
let s = query("Setup")[0];
if get(s, "Setup", "done") == 1 { return; }
set(s, "Setup", "done", 1);
for e in query("Spin") {
    attach_mesh(e, #{ Sphere: 0.5 });
    attach_pbr(e, #{ base_color: srgb(0.9, 0.3, 0.3), metallic: 0.4, roughness: 0.5 });
    set_translation(e, [get(e, "Spin", "x"), 0.0, 0.0]);
}
```

## 测试模式

旧测试：

```rust
let mut vm = VmWorld::load(path)?;
vm.tick()?;
let entities = vm.query("Foo");
let val = vm.get(entity, "Foo", "x")?;
```

新测试：

```rust
let mut world = bevy_ecs::world::World::new();
let mut vm = VmInstance::load(&mut world, path)?;
vm.tick(&mut world)?;
let entities = vm.query(&mut world, "Foo");
let val = vm.get(&world, entity, "Foo", "x")?;
```

视觉 host fn (`attach_mesh` / `attach_pbr` / `attach_camera_3d`) 需要 Bevy
`Assets<Mesh>` / `Assets<StandardMaterial>` / `AssetServer` 等资源——这些只
在真 Bevy `App`（带 `DefaultPlugins`）下存在。Headless tests 不应执行
attach；改测脚本端 dynamic 组件的状态。

## 已迁移的 demos

- ✅ `orbit/` —— 三立方体弹跳。
- ✅ `primitives/` —— 9 种几何图元 + 地面 + 自转。
- ✅ `gallery/` —— glTF 场景 + 程序图元混合。
- ✅ `object_viewer_3d/` —— 鼠标轨道相机。
- ✅ `drag/` —— 鼠标拖动立方体。

## 仍需迁移

- ⏳ `minesweeper/` —— 多 plugin、Camera2d、Sprite 棋盘、UI panel、状态机。需要：
  - 把每个 plugin 的 entities 段视觉字段挪到 setup.rhai
  - Sprite 通过 `attach_sprite` 挂
  - Camera2d 通过 `attach_camera_2d` 挂
  - UI 节点通过 set 走 Node / BackgroundColor 等 Bevy typed 组件（已通过
    `register_ui_types` 注册，可直接用）
- ⏳ `geometry_bros/` —— 类似 minesweeper，复杂场景。
- ⏳ `counter_sphere/` —— 第一人称 demo，Camera3d + 相机跟随脚本。
- ⏳ `tests/{counter_sphere, geometry_bros, minesweeper, drag_flow}.rs` 这些
  集成测试已被 stub 成空文件，待对应 demo 迁移完毕重写。
- ⏳ `examples/{drag, inspect}.rs` 同上。

//! Per-world interpreter — the driving core of a single sandbox.
//!
//! [`VmWorld`] owns a private `bevy_ecs::World`, builds initial entities from
//! a [`WorldConfig`], and runs the world's systems once per [`Self::tick`].
//! The world is exclusively owned by its interpreter — no concurrent borrowers
//! — so dynamic system access sets are fine; the supervisor is responsible
//! for parallelism across worlds.
//!
//! Each tick has three phases:
//! 1. **System phase**: every loaded [`System`] runs in declared order.
//!    Scripts read/write the World and the [`EventStore`] front buffer here.
//! 2. **Static phase**: typed `Position`/`Velocity` integration via direct
//!    queries — no reflection.
//! 3. **Event swap**: the [`EventStore`] swaps `front <- back; back.clear()`,
//!    so events emitted during this tick become readable next tick.
//!
//! ## Construction: builder
//!
//! Worlds are built through [`VmWorldBuilder`]:
//!
//! ```ignore
//! use bevy_vm::{VmWorld, VmWorldBuilder};
//! # use serde::{Deserialize, Serialize};
//! # #[derive(Default, Clone, Serialize, Deserialize)]
//! # struct Click { x: f32, y: f32 }
//! let vm = VmWorldBuilder::new()
//!     .with_event::<Click>("Click")
//!     .load("worlds/example.ron")?;
//! # Ok::<(), bevy_vm::VmError>(())
//! ```
//!
//! [`VmWorld::load`] is preserved as a no-event shorthand.

use crate::component::{ComponentKind, ComponentRegistry, Position, Velocity};
use crate::config::{ConfigFormat, EntityConfig, SystemConfig, WorldConfig};
use crate::error::VmError;
use crate::event::{EventError, EventKind, EventRegistry, EventStore, merge_with_default};
use crate::system::{ScriptSystem, System};
use crate::world_access::{self, WorldAccessError};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

/// Build a [`VmWorld`] with custom typed events registered up front.
///
/// Typed events must be registered **before** loading the config, because
/// the script engine captures the [`EventRegistry`] when it compiles. Dynamic
/// events declared in the config are added on top automatically.
#[derive(Default)]
pub struct VmWorldBuilder {
    events: EventRegistry,
}

impl VmWorldBuilder {
    /// Empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a typed event channel under `name` for Rust type `T`.
    ///
    /// `T` does **not** need to implement [`Default`]; payload merging on
    /// emit is skipped for such channels — fitting for events the platform
    /// always produces fully populated (input events, network frames, …).
    /// Use [`Self::with_event_default`] when you want partial-payload emits
    /// to fall back on `T::default()`.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the registration fails (name clash).
    pub fn with_event<T>(mut self, name: &str) -> Result<Self, VmError>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    {
        self.events.register_typed::<T>(name)?;
        Ok(self)
    }

    /// Like [`Self::with_event`] but additionally records `T::default()` as
    /// the merge baseline for partial payloads (gameplay events declared by
    /// the host where scripts may want to omit fields).
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the registration fails (name clash or
    /// serialization of the default instance fails).
    pub fn with_event_default<T>(mut self, name: &str) -> Result<Self, VmError>
    where
        T: Serialize + for<'de> Deserialize<'de> + Default + Send + 'static,
    {
        self.events.register_typed_with_default::<T>(name)?;
        Ok(self)
    }

    /// Load a world config from `config_path`, picking the parser by file
    /// extension (see [`crate::config::ConfigFormat::from_extension`]).
    ///
    /// `config_path` 可以是文件（`.ron` / `.json`）或目录——目录情况下自动
    /// 找下面的 `world.ron`。被加载的根 world 通过 `plugins:` 字段引用的
    /// plugin 文件递归展开，按 `dependencies:` 拓扑排序后逐个注册。
    ///
    /// # Errors
    ///
    /// Same as [`VmWorld::load`].
    pub fn load(self, config_path: impl AsRef<Path>) -> Result<VmWorld, VmError> {
        let config_path = config_path.as_ref();
        let resolved = resolve_root_path(config_path);
        let plugins = crate::plugin_loader::load_root(&resolved)?;
        self.build_from_plugins(plugins)
    }

    /// Build a [`VmWorld`] from `text` (using the explicit `format`) and
    /// the base directory used to resolve script paths.
    ///
    /// # Errors
    ///
    /// Same as [`VmWorld::load`].
    pub fn from_text(
        self,
        text: &str,
        format: ConfigFormat,
        base_dir: impl AsRef<Path>,
    ) -> Result<VmWorld, VmError> {
        let config = WorldConfig::from_text(text, format)?;
        // inline 文本模式不支持 plugins 引用——这里 plugins/dependencies 必须空。
        // 如果要加载多 plugin，请走 load() 用文件路径。
        if !config.plugins.is_empty() {
            return Err(VmError::Parse(
                "inline `from_text` does not support `plugins:` field; use `load()` with a file path"
                    .to_owned(),
            ));
        }
        let plugins = vec![crate::plugin_loader::LoadedPlugin {
            name: crate::plugin_loader::ROOT_PLUGIN.to_owned(),
            config,
            base_dir: base_dir.as_ref().to_path_buf(),
        }];
        self.build_from_plugins(plugins)
    }

    /// Convenience: parse from JSON text. Equivalent to
    /// `from_text(text, ConfigFormat::Json, base_dir)`.
    ///
    /// # Errors
    ///
    /// Same as [`Self::from_text`].
    #[cfg(feature = "config-json")]
    pub fn from_json(self, text: &str, base_dir: impl AsRef<Path>) -> Result<VmWorld, VmError> {
        self.from_text(text, ConfigFormat::Json, base_dir)
    }

    /// Convenience: parse from RON text. Equivalent to
    /// `from_text(text, ConfigFormat::Ron, base_dir)`.
    ///
    /// # Errors
    ///
    /// Same as [`Self::from_text`].
    #[cfg(feature = "config-ron")]
    pub fn from_ron(self, text: &str, base_dir: impl AsRef<Path>) -> Result<VmWorld, VmError> {
        self.from_text(text, ConfigFormat::Ron, base_dir)
    }

    /// Common builder tail: 把已加载 + 拓扑排好序的 plugin 列表装配成
    /// [`VmWorld`]。
    ///
    /// `plugins` 中至少要有一个根 plugin（[`crate::plugin_loader::ROOT_PLUGIN`]
    /// 名）；`seed` 字段只看根，其它 plugin 写了也忽略。
    fn build_from_plugins(
        mut self,
        plugins: Vec<crate::plugin_loader::LoadedPlugin>,
    ) -> Result<VmWorld, VmError> {
        let mut world = World::new();
        let mut components = ComponentRegistry::with_builtins(&mut world);

        // 注册 Bevy 的 ChildOf 关系组件——VM World 内部的 ECS hooks 会
        // 自动维护反向 Children 集合，脚本通过 set_parent host 函数操作。
        // 不进 ComponentRegistry：脚本不按名字 set 它，且 ChildOf 没有
        // ReflectDefault（合理：默认值 Entity::PLACEHOLDER 永远无效）。
        world.register_component::<bevy_ecs::hierarchy::ChildOf>();

        // 决定性 RNG：根 plugin 的 seed 字段决定。
        let root_seed = plugins
            .iter()
            .find(|p| p.name == crate::plugin_loader::ROOT_PLUGIN)
            .and_then(|p| p.config.seed);
        let rng = match root_seed {
            Some(seed) => crate::random::VmRng::from_seed(seed),
            None => crate::random::VmRng::from_entropy(),
        };
        world.insert_resource(rng);

        // 直接复用 Bevy 的 Time<()>——脚本端 time() / delta() 走它的
        // elapsed_secs_f64 / delta_secs_f64。host 端通过 advance_time 喂 dt：
        // viewer 路径自动转发主世界的 Time::delta；headless 测试自己控制步长。
        //
        // 暂停语义（仿 Bevy `Time<Virtual>::pause`）：
        // - `set_paused(true)` 时 `advance_time` 内部把 dt 替换为 0
        // - 脚本读到 `delta() == 0` 时 movement / spawn timer 自然冻结
        // - HUD / UI 仍跑（system 仍执行，只是没有时间流逝）
        world.insert_resource(bevy_time::Time::<()>::default());
        world.insert_resource(VmPauseState::default());

        // 第一阶段：注册所有 plugin 的 components/events，命名空间前缀化。
        // 根 plugin 的内容不带前缀（视作全局空间）。
        for plugin in &plugins {
            register_plugin_components(&mut world, &mut components, plugin)?;
            register_plugin_events(&mut self.events, plugin)?;
        }

        let components = Rc::new(components);
        let events = Rc::new(self.events);

        // 第二阶段：spawn 所有 plugin 的 entities。entities 引用组件名
        // 用全限定（外人引用）或短名（自己内部引用，加载时已 rewrite）。
        for plugin in &plugins {
            spawn_plugin_entities(&mut world, &components, plugin)?;
        }

        // 第三阶段：加载所有 plugin 的 scripts。脚本顺序由 SystemSet 拓扑
        // 决定（before/after），与 Bevy `add_systems(...).before(...).after(...)`
        // 对齐——plugin 拓扑只决定**注册**顺序，运行顺序独立。
        let systems = schedule_scripts(&plugins, &components, &events)?;
        let store = EventStore::new(&events);
        Ok(VmWorld {
            world,
            components,
            events,
            store,
            systems,
        })
    }
}

/// Self-contained, tickable simulation unit.
pub struct VmWorld {
    world: World,
    components: Rc<ComponentRegistry>,
    events: Rc<EventRegistry>,
    store: EventStore,
    systems: Vec<Box<dyn System>>,
}

impl VmWorld {
    /// Convenience: build a world from a config file without registering any
    /// typed events. Equivalent to `VmWorldBuilder::new().load(path)`.
    ///
    /// # Errors
    ///
    /// - [`VmError::Io`] when the config or a referenced script file cannot
    ///   be read.
    /// - [`VmError::Parse`] when the config text fails to parse.
    /// - [`VmError::UnknownComponent`] when a referenced component is not
    ///   registered.
    /// - [`VmError::Registry`] / [`VmError::InitTypedComponent`] /
    ///   [`VmError::InitDynamicField`] / [`VmError::InsertDynamicDefault`]
    ///   when component initialization fails.
    /// - [`VmError::Event`] when an event registration fails.
    /// - [`VmError::ScriptCompile`] when a script source cannot be compiled.
    pub fn load(config_path: impl AsRef<Path>) -> Result<Self, VmError> {
        VmWorldBuilder::new().load(config_path)
    }

    /// Convenience: like [`Self::load`] but reading the config string directly,
    /// with the format chosen explicitly.
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    pub fn from_text(
        text: &str,
        format: ConfigFormat,
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, VmError> {
        VmWorldBuilder::new().from_text(text, format, base_dir)
    }

    /// Convenience: parse from JSON text. See [`Self::from_text`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    #[cfg(feature = "config-json")]
    pub fn from_json(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, VmError> {
        VmWorldBuilder::new().from_json(text, base_dir)
    }

    /// Convenience: parse from RON text. See [`Self::from_text`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    #[cfg(feature = "config-ron")]
    pub fn from_ron(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, VmError> {
        VmWorldBuilder::new().from_ron(text, base_dir)
    }

    /// Advance the world by one tick: run systems → integrate → swap event
    /// buffers.
    ///
    /// # Errors
    ///
    /// Returns the corresponding [`VmError`] if any system raises one.
    ///
    /// 事件模型（typed 双缓冲、dynamic 单缓冲——见 [`crate::event`] 模块文档）：
    /// - typed events：host send_event 写 back（下一 tick 进 front）；脚本
    ///   `events("X")` 读 front。tick 末 swap，pump_out 在 tick 之后立即
    ///   drain 本帧的内容。
    /// - dynamic events：脚本 emit 直接写 buffer，同 tick 后续 system 立即
    ///   可读（plugin 间事件链零延迟）。tick 末 clear——dynamic 事件不跨
    ///   帧存活。
    pub fn tick(&mut self) -> Result<(), VmError> {
        for system in &self.systems {
            system.run(&mut self.world, &mut self.store)?;
        }
        integrate_movement(&mut self.world);
        self.store.end_tick_all();
        Ok(())
    }

    /// Advance the VM clock by `delta`.
    ///
    /// 调用方决定时间策略：
    /// - viewer / `bevy-bridge` 路径：[`crate::render::tick_vm`] 每帧自动
    ///   转发主世界的 [`bevy_time::Time::delta`]，example 作者无感。
    /// - headless 测试：手动调本方法控制步长，保证可重现。
    ///
    /// 期望在 [`Self::tick`] 之**前**调用——脚本本帧读 `time()` / `delta()`
    /// 时已经看到推进后的值。
    ///
    /// 暂停状态（[`Self::set_paused`]）下 `delta` 被替换为 `Duration::ZERO`，
    /// 让所有依赖 `delta()` 的脚本逻辑自然冻结。
    pub fn advance_time(&mut self, delta: std::time::Duration) {
        let paused = self
            .world
            .get_resource::<VmPauseState>()
            .map(|p| p.paused)
            .unwrap_or(false);
        let effective = if paused {
            std::time::Duration::ZERO
        } else {
            delta
        };
        if let Some(mut time) = self.world.get_resource_mut::<bevy_time::Time>() {
            time.advance_by(effective);
        }
    }

    /// Pause / unpause the VM clock. 仿 Bevy `Time<Virtual>::pause` 的语义：
    /// 暂停时 [`Self::advance_time`] 调用注入的 dt 被替换为零，脚本 `delta()`
    /// 看到 0；`time()` 不再增长；system 本身仍执行（HUD / UI 可继续刷新）。
    pub fn set_paused(&mut self, paused: bool) {
        if let Some(mut state) = self.world.get_resource_mut::<VmPauseState>() {
            state.paused = paused;
        }
    }

    /// Whether the VM clock is currently paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.world
            .get_resource::<VmPauseState>()
            .map(|p| p.paused)
            .unwrap_or(false)
    }

    /// Read-only borrow of the underlying World.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Shared handle to the [`ComponentRegistry`] driving this VM.
    ///
    /// Useful when host code wants to call [`crate::world_access`] directly
    /// outside of a tick — same registry the script host functions use.
    /// Returns an [`Rc`] clone so the caller can hold a reference while
    /// also calling [`Self::world_mut`]; the registry is never mutated
    /// after construction.
    #[must_use]
    pub fn components(&self) -> Rc<ComponentRegistry> {
        Rc::clone(&self.components)
    }

    /// Mutable borrow of the underlying World.
    ///
    /// Mainly for the render-sync layer that wants to `query` typed components
    /// directly. Script-driven mutation should go through [`Self::tick`]
    /// rather than touching the World from the outside.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Read-only borrow of the [`EventStore`] (front buffer).
    #[must_use]
    pub fn events(&self) -> &EventStore {
        &self.store
    }

    /// Mutable borrow of the [`EventStore`].
    pub fn events_mut(&mut self) -> &mut EventStore {
        &mut self.store
    }

    /// Send a typed event into the back buffer for `name`, by value.
    ///
    /// `name` must have been registered with [`VmWorldBuilder::with_event::<T>`]
    /// for the **same** `T`. Zero serialization on this path — the event is
    /// stored as `T` and pump_out hands it back to Bevy as `T`.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the channel is unknown, dynamic, or
    /// registered for a different `T`.
    pub fn send_event<T: Send + 'static>(&mut self, name: &str, event: T) -> Result<(), VmError> {
        self.store.push_typed(name, event).map_err(VmError::from)
    }

    /// Send a dynamic event by name. The payload is merged with the declared
    /// default before being pushed to `name`'s back buffer.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the channel is unknown or typed.
    pub fn send_event_dynamic(&mut self, name: &str, payload: Value) -> Result<(), VmError> {
        let merged = match self.events.resolve(name) {
            Some(EventKind::Typed(_)) => {
                return Err(EventError::KindMismatch {
                    name: name.to_owned(),
                }
                .into());
            }
            Some(EventKind::Dynamic(dyn_event)) => {
                merge_with_default(payload, Some(&dyn_event.default))
            }
            None => {
                return Err(EventError::UnknownEvent {
                    name: name.to_owned(),
                }
                .into());
            }
        };
        self.store.push_dynamic(name, merged).map_err(VmError::from)
    }

    /// Drain the front buffer of typed channel `name`, returning every event
    /// readable this tick by value.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the channel is unknown, dynamic, or
    /// registered for a different `T`.
    pub fn drain_events<T: Send + 'static>(&mut self, name: &str) -> Result<Vec<T>, VmError> {
        self.store.drain_typed::<T>(name).map_err(VmError::from)
    }

    /// Drain the front buffer of dynamic channel `name` as raw [`Value`]s.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the channel is unknown or typed.
    pub fn drain_events_dynamic(&mut self, name: &str) -> Result<Vec<Value>, VmError> {
        self.store.drain_dynamic(name).map_err(VmError::from)
    }

    /// Query every entity carrying `component`.
    #[must_use]
    pub fn query(&mut self, component: &str) -> Vec<Entity> {
        let registry = Rc::clone(&self.components);
        world_access::query_with_component(&mut self.world, &registry, component)
    }

    /// Read the value at `path` on `entity`'s `component`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldAccessError`] if the entity or component is missing,
    /// or the path is invalid. See [`crate::world_access::get`].
    pub fn get(
        &self,
        entity: Entity,
        component: &str,
        path: &str,
    ) -> Result<Value, WorldAccessError> {
        world_access::get(&self.world, &self.components, entity, component, path)
    }

    /// Full-state inspection: clone every entity's component values.
    ///
    /// Designed for debuggers / inspectors — it deep-clones the world state
    /// and **must not** be used in a per-frame render path; rendering should
    /// read only the visual fields it needs and incrementally sync.
    #[must_use]
    pub fn inspect(&mut self) -> WorldSnapshot {
        let entities = world_access::all_entities(&mut self.world);
        let registry = Rc::clone(&self.components);
        let entities = entities
            .into_iter()
            .map(|entity| self.inspect_entity(&registry, entity))
            .collect();
        WorldSnapshot { entities }
    }

    /// Snapshot a single entity: collect every attached component's full value.
    fn inspect_entity(&self, registry: &ComponentRegistry, entity: Entity) -> EntitySnapshot {
        let components = world_access::components_of(&self.world, registry, entity)
            .into_iter()
            .filter_map(|name| {
                let value = world_access::read_component(&self.world, registry, entity, &name)
                    .ok()
                    .flatten()?;
                Some((name, value))
            })
            .collect();
        EntitySnapshot { entity, components }
    }
}

/// Full state snapshot of a world at a point in time, for inspectors / debug.
#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    /// Snapshots of every entity in the world.
    pub entities: Vec<EntitySnapshot>,
}

/// Single-entity snapshot: id + every attached component's full value.
#[derive(Debug, Clone)]
pub struct EntitySnapshot {
    /// Entity id.
    pub entity: Entity,
    /// Component name → full value pairs.
    pub components: Vec<(String, Value)>,
}

/// 把 `load(path)` 的入参规范化到具体文件路径——目录传入时自动找
/// `world.ron`。这是文件夹入口的所在。
fn resolve_root_path(input: &Path) -> std::path::PathBuf {
    if input.is_dir() {
        input.join("world.ron")
    } else {
        input.to_path_buf()
    }
}

/// Load a single system. `plugin_name` 给 ScriptSystem 提供命名空间上下文。
fn load_system(
    system_config: &SystemConfig,
    base_dir: &Path,
    plugin_name: &Rc<str>,
    components: &Rc<ComponentRegistry>,
    events: &Rc<EventRegistry>,
) -> Result<Box<dyn System>, VmError> {
    let SystemConfig::Script { path, run_if, .. } = system_config;
    let script_path = base_dir.join(path);
    let source = std::fs::read_to_string(&script_path).map_err(|e| VmError::Io {
        path: script_path.display().to_string(),
        reason: e.to_string(),
    })?;
    let script_dir = script_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let system = ScriptSystem::compile(
        &source,
        Rc::clone(plugin_name),
        &script_dir,
        Rc::clone(components),
        Rc::clone(events),
        run_if,
    )?;
    Ok(Box::new(system))
}

/// 注册一个 plugin 声明的所有 dynamic components 到 [`ComponentRegistry`]。
///
/// 命名规则：root plugin（[`crate::plugin_loader::ROOT_PLUGIN`]）的组件
/// 不加前缀；其它 plugin 的组件 `Foo` 注册为 `<plugin>::Foo`。这样组件名
/// 在不同 plugin 间不会冲突，跨 plugin 引用走全限定。
///
/// # Errors
///
/// 名字与已注册组件冲突时返回 [`VmError::Registry`]。
fn register_plugin_components(
    world: &mut World,
    registry: &mut ComponentRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
) -> Result<(), VmError> {
    for decl in &plugin.config.components {
        let name = qualify_name(&plugin.name, &decl.name);
        registry.register_dynamic(world, &name, decl.default.clone())?;
    }
    Ok(())
}

/// 注册一个 plugin 声明的所有 dynamic events 到 [`EventRegistry`]。
/// 命名规则同 [`register_plugin_components`]。
///
/// # Errors
///
/// 名字冲突时返回 [`VmError::Event`]。
fn register_plugin_events(
    registry: &mut EventRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
) -> Result<(), VmError> {
    for decl in &plugin.config.events {
        let name = qualify_name(&plugin.name, &decl.name);
        registry.register_dynamic(&name, decl.default.clone())?;
    }
    Ok(())
}

/// 命名前缀化：`<plugin>::<short>`。根 plugin 的内容直接用短名（全局空间）。
/// 短名已包含 `::` 时视作"已经全限定"——直接返回，避免 root 重复加前缀。
fn qualify_name(plugin: &str, short: &str) -> String {
    if plugin == crate::plugin_loader::ROOT_PLUGIN || short.contains("::") {
        short.to_owned()
    } else {
        format!("{plugin}::{short}")
    }
}

/// Spawn 一个 plugin 声明的所有 entities。
///
/// entities 里的组件 key 走 [`qualify_component_ref`] 解析：plugin 内部
/// 写短名引用自己 plugin 的组件（如 `"Tile"`，自动加前缀变 `<plugin>::Tile`），
/// 跨 plugin 引用必须写全限定（`"tiles::Tile"`）；引用 host 内置（如
/// `Position`）也走全局名，不加前缀。
fn spawn_plugin_entities(
    world: &mut World,
    registry: &ComponentRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
) -> Result<(), VmError> {
    for entity_config in &plugin.config.entities {
        spawn_entity(world, registry, plugin, entity_config)?;
    }
    Ok(())
}

/// Spawn one entity and initialize every component declared on it.
fn spawn_entity(
    world: &mut World,
    registry: &ComponentRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
    entity_config: &EntityConfig,
) -> Result<(), VmError> {
    let entity = world.spawn_empty().id();
    for (component_name, overrides) in &entity_config.components {
        let resolved = qualify_component_ref(registry, &plugin.name, component_name)?;
        match registry.resolve(&resolved) {
            Some(ComponentKind::Typed(_)) => {
                init_typed_component(world, registry, entity, &resolved, overrides)?;
            }
            Some(ComponentKind::Dynamic(_)) => {
                init_dynamic_component(world, registry, entity, &resolved, overrides)?;
            }
            None => return Err(VmError::UnknownComponent(resolved)),
        }
    }
    Ok(())
}

/// 把一个组件名引用解析为注册表里的真实名字。
///
/// 解析顺序：
/// 1. `name` 已含 `::` → 视作全限定，直接用
/// 2. registry 已有 `name` → 全局名（host typed 组件 / 根 plugin 组件 / host 注册的 typed 事件）
/// 3. registry 已有 `<my_plugin>::<name>` → 自己 plugin 的组件
/// 4. 都没有 → 返回 [`VmError::UnknownComponent`]
///
/// 这样让 plugin 内部写自家组件短名最舒服，又能引用全局 / 跨 plugin 类型。
fn qualify_component_ref(
    registry: &ComponentRegistry,
    plugin_name: &str,
    name: &str,
) -> Result<String, VmError> {
    if name.contains("::") {
        return Ok(name.to_owned());
    }
    if registry.resolve(name).is_some() {
        return Ok(name.to_owned());
    }
    let qualified = format!("{plugin_name}::{name}");
    if registry.resolve(&qualified).is_some() {
        return Ok(qualified);
    }
    Err(VmError::UnknownComponent(name.to_owned()))
}

/// Initialize a typed component by deserializing the whole config value.
///
/// Going through the full serde path enforces the type invariant (any field
/// failure rejects the whole component), and supports nested structs / enums
/// like [`crate::component::render::Renderable`].
fn init_typed_component(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component_name: &str,
    overrides: &Value,
) -> Result<(), VmError> {
    let typed = registry
        .typed(component_name)
        .ok_or_else(|| VmError::UnknownComponent(component_name.to_owned()))?;
    typed
        .insert_from_value(world, entity, registry.type_registry(), overrides.clone())
        .map_err(|source| VmError::InitTypedComponent {
            component: component_name.to_owned(),
            source,
        })?;
    world_access::apply_required(world, registry, entity, component_name).map_err(|source| {
        VmError::InitTypedRequired {
            component: component_name.to_owned(),
            source,
        }
    })
}

/// Initialize a dynamic component: insert its default instance, then apply
/// the config overrides as a top-level field merge.
fn init_dynamic_component(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component_name: &str,
    overrides: &Value,
) -> Result<(), VmError> {
    world_access::insert_dynamic_default(world, registry, entity, component_name).map_err(
        |source| VmError::InsertDynamicDefault {
            component: component_name.to_owned(),
            source,
        },
    )?;
    let Value::Object(fields) = overrides else {
        return Ok(());
    };
    for (field, value) in fields.iter() {
        world_access::set(
            world,
            registry,
            entity,
            component_name,
            field,
            value.clone(),
        )
        .map_err(|source| VmError::InitDynamicField {
            component: component_name.to_owned(),
            field: field.clone(),
            source,
        })?;
    }
    Ok(())
}

/// 收集所有 plugin 的 script system + 它们的 SystemSet 关系，按
/// `before` / `after` 约束拓扑排序，输出最终的执行顺序。
///
/// 与 Bevy `add_systems((a, b).chain()).add_systems(c.after(a))` 的语义对齐：
/// - 每个 plugin 隐式是一个 set，名字 = plugin 名
/// - 每个 script 显式可加入 `in_set` 列出的 set
/// - `before: ["x"]` 把 `current.path → 任意 in_set("x") 或 plugin "x" 的 system`
///   的边加进 system 图——current 必须先于它们运行
/// - `after: ["x"]` 反向同理
///
/// 默认顺序（不写 before/after 时）：plugin 拓扑顺序 + 同 plugin 内声明顺序。
/// 这样所有"老"配置不动也能 work。
fn schedule_scripts(
    plugins: &[crate::plugin_loader::LoadedPlugin],
    components: &Rc<ComponentRegistry>,
    events: &Rc<EventRegistry>,
) -> Result<Vec<Box<dyn System>>, VmError> {
    use std::collections::{HashMap, HashSet};

    // ---- 收集所有 system 的元数据 ------------------------------------------
    struct ScriptMeta<'a> {
        plugin: &'a crate::plugin_loader::LoadedPlugin,
        config: &'a SystemConfig,
        /// `<plugin>::<file_stem>`—— 全局唯一的 system 名。
        full_name: String,
        sets: Vec<String>,
    }

    let mut metas: Vec<ScriptMeta<'_>> = Vec::new();
    for plugin in plugins {
        for system_config in &plugin.config.systems {
            let SystemConfig::Script { path, in_set, .. } = system_config;
            let stem = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(path);
            let full_name = format!("{}::{}", plugin.name, stem);
            // 隐式 set：plugin 名；显式 set：in_set 列表。
            let mut sets = vec![plugin.name.clone()];
            sets.extend(in_set.iter().cloned());
            metas.push(ScriptMeta {
                plugin,
                config: system_config,
                full_name,
                sets,
            });
        }
    }

    // ---- 建立 set → system 反向映射 ----------------------------------------
    let mut systems_in_set: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, meta) in metas.iter().enumerate() {
        for set in &meta.sets {
            systems_in_set.entry(set.clone()).or_default().push(i);
        }
    }
    // 同时支持 before/after 引用具体 system 名（`<plugin>::<stem>`）。
    let by_full_name: HashMap<&str, usize> = metas
        .iter()
        .enumerate()
        .map(|(i, m)| (m.full_name.as_str(), i))
        .collect();

    let resolve_ref = |name: &str| -> Vec<usize> {
        if let Some(targets) = systems_in_set.get(name) {
            return targets.clone();
        }
        if let Some(&idx) = by_full_name.get(name) {
            return vec![idx];
        }
        Vec::new()
    };

    // ---- 构造有向边（before edges 数组 + in_degree 计数） ------------------
    let n = metas.len();
    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); n]; // edges[i] = {j | i must run before j}
    let mut in_degree: Vec<usize> = vec![0; n];

    let add_edge =
        |edges: &mut Vec<HashSet<usize>>, in_degree: &mut Vec<usize>, from: usize, to: usize| {
            if from == to {
                return;
            }
            if edges[from].insert(to) {
                in_degree[to] += 1;
            }
        };

    // 默认边：仅同 plugin 内多 script 按声明顺序串。跨 plugin 默认无序——
    // 与 Bevy 对齐：system 间没声明 ordering 时不强加顺序，由 before/after
    // 决定。Kahn 排序仍然稳定（同层取声明序最早的）。
    for i in 1..n {
        if metas[i - 1].plugin.name == metas[i].plugin.name {
            add_edge(&mut edges, &mut in_degree, i - 1, i);
        }
    }

    // 显式 before/after 解析。
    for (i, meta) in metas.iter().enumerate() {
        let SystemConfig::Script { before, after, .. } = meta.config;
        for target_name in before {
            let targets = resolve_ref(target_name);
            if targets.is_empty() {
                return Err(VmError::UnknownSystemRef {
                    system: meta.full_name.clone(),
                    missing: target_name.clone(),
                    kind: "before",
                });
            }
            for &j in &targets {
                add_edge(&mut edges, &mut in_degree, i, j);
            }
        }
        for source_name in after {
            let sources = resolve_ref(source_name);
            if sources.is_empty() {
                return Err(VmError::UnknownSystemRef {
                    system: meta.full_name.clone(),
                    missing: source_name.clone(),
                    kind: "after",
                });
            }
            for &j in &sources {
                add_edge(&mut edges, &mut in_degree, j, i);
            }
        }
    }

    // ---- Kahn 拓扑排序 ----------------------------------------------------
    // 默认边的方向是声明序——已经无环。before/after 显式边可能制造冲突；
    // 检测环时报 SystemOrderCycle。
    let mut order: Vec<usize> = Vec::with_capacity(n);
    loop {
        // 选最早声明的"零入度"system（保持稳定输出）。
        let pick = (0..n).find(|&i| in_degree[i] == 0 && !order.contains(&i));
        let Some(idx) = pick else {
            break;
        };
        order.push(idx);
        for &j in &edges[idx] {
            in_degree[j] -= 1;
        }
        // 把 idx 标为 emitted（in_degree 永远 > 0 不会再选）。
        in_degree[idx] = usize::MAX;
    }

    if order.len() != n {
        // 还有未 emit 的——找一条环路报错。
        let stuck: Vec<&str> = (0..n)
            .filter(|i| !order.contains(i))
            .map(|i| metas[i].full_name.as_str())
            .collect();
        return Err(VmError::SystemOrderCycle {
            chain: stuck.join(" <-> "),
        });
    }

    // ---- 按 order 顺序加载 + 编译 ScriptSystem ----------------------------
    let mut systems: Vec<Box<dyn System>> = Vec::with_capacity(n);
    for idx in order {
        let meta = &metas[idx];
        let plugin_name: Rc<str> = Rc::from(meta.plugin.name.as_str());
        let system = load_system(
            meta.config,
            &meta.plugin.base_dir,
            &plugin_name,
            components,
            events,
        )?;
        systems.push(system);
    }
    Ok(systems)
}

/// VM 全局暂停标志——挂为 World resource，由 [`VmWorld::set_paused`]
/// 操控，[`VmWorld::advance_time`] 据此决定是否推进时间。
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct VmPauseState {
    /// 暂停中？暂停时 advance_time 注入零 dt——脚本 `delta()` = 0。
    pub paused: bool,
}

/// Static-phase movement integration.
fn integrate_movement(world: &mut World) {
    let mut query = world.query::<(&mut Position, &Velocity)>();
    for (mut position, velocity) in query.iter_mut(world) {
        position.x += velocity.x;
        position.y += velocity.y;
        position.z += velocity.z;
    }
}

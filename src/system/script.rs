//! Script system: exposes World + event-store operations to AI-authored Rhai
//! scripts.
//!
//! [`ScriptSystem`] is one implementation of [`System`]. Internally it holds
//! a `rhai::Engine` populated with host functions plus a compiled `AST`
//! (compiled once at load time, re-run every tick). Scripts read/write the
//! sandbox through host functions: `query`, `get`, `set`, `remove`, lifecycle
//! helpers, and the event helpers `emit` / `events`.
//!
//! # Safely accessing `&mut World` and `&mut EventStore`
//!
//! Rhai host closures are `'static` and cannot capture borrows handed to
//! `tick`. We bridge with **scoped raw pointers**: before each run the World
//! pointer and the EventStore pointer are written into shared
//! `Rc<Cell<*mut _>>` slots; host functions deref through the slot during
//! execution and the slots are nulled out afterwards. Soundness rests on the
//! architecture: each interpreter exclusively owns its World, runs scripts
//! single-threaded, and never re-enters itself — so the pointers are valid
//! for the entire run with no aliasing or concurrent access. Outside a run
//! the slots are null and host functions error rather than deref.

mod convert;

use crate::component::ComponentRegistry;
use crate::error::VmError;
use crate::event::{EventKind, EventRegistry, EventStore, merge_with_default};
use crate::random::VmRng;
use crate::system::{Pause, System, TickContext};
use crate::world_access;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_time::Time;
use rhai::{AST, Dynamic, Engine, Scope};
use std::cell::Cell;
use std::ptr;
use std::rc::Rc;

/// 脚本每 tick 允许执行的最大操作数，用于阻止 AI 写出的死循环拖垮世界。
const MAX_OPERATIONS: u64 = 1_000_000;

/// 脚本最大调用深度，防止无限递归。
const MAX_CALL_LEVELS: usize = 64;

/// Top-level 表达式最大深度。Rhai 默认 64；提到 128 让真实游戏脚本里
/// 偶尔出现的"嵌套 map literal + 几个数组" 这类形态不至于爆掉。
const MAX_EXPR_DEPTH: usize = 128;
/// 函数体表达式最大深度。Rhai 默认 32；提到 128 与 top-level 对齐——
/// 函数比 top-level 更受限是 rhai 早期的安全考量，但对游戏脚本反而是
/// 累赘（init_board 这种把数组/对象字面量塞进函数的写法很常见）。
const MAX_FUNCTION_EXPR_DEPTH: usize = 128;

use crate::vm::id::{VmId, VmTag};

/// 实体在脚本侧的表示：实体位编码后的整数。
type ScriptEntity = i64;

/// 脚本运行期间共享的 `&mut` 槽：World + EventStore + 三件 per-instance
/// 资源（RNG / Time / Pause）。
///
/// 全部走 raw `*mut`：Rhai host 闭包是 `'static`，无法捕获 borrow，唯一
/// 安全办法就是「单线程串行 tick + slots 在 run 期非空、外部空」的不变式
/// 维持。详见 [`crate::system::script`] 模块文档。
#[derive(Default)]
struct Slots {
    world: Cell<*mut World>,
    events: Cell<*mut EventStore>,
    rng: Cell<*mut VmRng>,
    time: Cell<*mut Time<()>>,
    pause: Cell<*mut Pause>,
}

impl Slots {
    fn fill(&self, ctx: &mut TickContext<'_>) {
        self.world.set(ptr::from_mut(ctx.world));
        self.events.set(ptr::from_mut(ctx.events));
        self.rng.set(ptr::from_mut(ctx.rng));
        self.time.set(ptr::from_mut(ctx.time));
        self.pause.set(ptr::from_mut(ctx.pause));
    }

    fn clear(&self) {
        self.world.set(ptr::null_mut());
        self.events.set(ptr::null_mut());
        self.rng.set(ptr::null_mut());
        self.time.set(ptr::null_mut());
        self.pause.set(ptr::null_mut());
    }
}

/// 运行一段 Rhai 脚本的 system。
pub struct ScriptSystem {
    engine: Engine,
    ast: AST,
    /// `run_if` 条件——预编译的 Rhai 表达式 AST 列表。每帧执行 system
    /// body 之前 eval 全部，全 true 才跑。仿 Bevy `add_systems(...).run_if(...)`。
    run_if: Vec<AST>,
    slots: Rc<Slots>,
    /// VM 实例 id——host 函数 query/spawn 用它给 entity 加 VmTag、给 query
    /// 加 VmTag 过滤。
    vm_id: VmId,
}

impl ScriptSystem {
    /// 编译脚本并注册全部宿主函数。
    ///
    /// `plugin_name` 是脚本所属的 plugin 名（`<root>` 为顶级 world）——host
    /// 函数收到组件 / 事件短名时优先查 `<plugin_name>::<short>` 命名空间。
    /// `registry` 是该世界的组件注册表。
    /// `run_if_exprs` 列出 run-if 条件的 Rhai 表达式（编译期 compile 成 AST）。
    ///
    /// # Errors
    ///
    /// 脚本无法编译为合法 Rhai AST 时返回 [`VmError::ScriptCompile`]——
    /// `run_if` 表达式编译失败也走此变体。
    pub fn compile(
        script: &str,
        plugin_name: Rc<str>,
        script_dir: &std::path::Path,
        components: Rc<ComponentRegistry>,
        events: Rc<EventRegistry>,
        run_if_exprs: &[String],
        vm_id: VmId,
    ) -> Result<Self, VmError> {
        let mut engine = Engine::new();
        engine.set_max_operations(MAX_OPERATIONS);
        engine.set_max_call_levels(MAX_CALL_LEVELS);
        engine.set_max_expr_depths(MAX_EXPR_DEPTH, MAX_FUNCTION_EXPR_DEPTH);

        // 让脚本可 `import "helpers" as h;` 加载相对当前脚本目录的模块——
        // 多 plugin 场景下 helpers.rhai 共享 host 函数（同一 Engine）+
        // 命名空间上下文（被 import 的模块继承宿主 plugin 的 plugin_name，
        // 因为它们运行在同一个 Engine instance）。
        engine.set_module_resolver(rhai::module_resolvers::FileModuleResolver::new_with_path(
            script_dir.to_path_buf(),
        ));

        let slots: Rc<Slots> = Rc::new(Slots::default());
        register_world_functions(&mut engine, &slots, &components, &plugin_name, vm_id);
        register_event_functions(&mut engine, &slots, &events, &plugin_name);
        register_random(&mut engine, &slots);
        register_time(&mut engine, &slots);
        register_logging(&mut engine);
        register_ui_helpers(&mut engine);
        #[cfg(feature = "bevy-bridge")]
        render_host::register_render_attach(&mut engine, &slots, vm_id);

        let ast = engine
            .compile(script)
            .map_err(|e| VmError::ScriptCompile(e.to_string()))?;

        // 预编译每条 run_if 表达式——`compile_expression` 限定为 expression-only，
        // 不允许语句，更贴近 Bevy condition 的纯函数语义。
        let run_if = run_if_exprs
            .iter()
            .map(|src| engine.compile_expression(src))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| VmError::ScriptCompile(format!("run_if expression: {e}")))?;

        Ok(Self {
            engine,
            ast,
            run_if,
            slots,
            vm_id,
        })
    }

    /// VmId this script was compiled against.
    #[must_use]
    pub fn vm_id(&self) -> VmId {
        self.vm_id
    }
}

impl System for ScriptSystem {
    /// 在给定上下文上执行一次脚本。
    ///
    /// run_if 表达式先 eval（共享同一 slots，能调 host 函数）；任一为 false
    /// 直接返回 Ok(()) 跳过 system body。
    ///
    /// 执行期间宿主函数可通过作用域指针读写 World / EventStore / RNG /
    /// Time / Pause；返回前指针槽被清空。
    fn run(&self, ctx: &mut TickContext<'_>) -> Result<(), VmError> {
        self.slots.fill(ctx);
        let result = self.run_inner();
        self.slots.clear();
        result
    }
}

impl ScriptSystem {
    /// run() 的主体——slots 已 fill，本 fn 返回前调用方负责 clear。
    fn run_inner(&self) -> Result<(), VmError> {
        // run_if：全 true 才 run system body。
        for cond in &self.run_if {
            let mut scope = Scope::new();
            let allowed: bool = self
                .engine
                .eval_ast_with_scope(&mut scope, cond)
                .map_err(|e| VmError::ScriptRuntime(format!("run_if eval: {e}")))?;
            if !allowed {
                return Ok(());
            }
        }
        let mut scope = Scope::new();
        self.engine
            .run_ast_with_scope(&mut scope, &self.ast)
            .map_err(|e| VmError::ScriptRuntime(e.to_string()))
    }
}

/// 在空指针槽上访问被托管资源时的统一错误信息。
const NO_ACTIVE_WORLD: &str = "script host function called with no active World";
const NO_ACTIVE_EVENTS: &str = "script host function called with no active EventStore";

/// 把脚本里收到的组件 / 事件短名解析成注册表中的真实键。
///
/// 顺序：
/// 1. `name` 含 `::` → 视作全限定，原样返回
/// 2. `<plugin>::<name>` 在注册表里 → 自己 plugin 的命名空间命中
/// 3. `<name>` 在注册表里 → 全局名（host typed 组件 / typed 事件 / 根 plugin 内容）
/// 4. 都没命中 → 原样返回，让下游报"未注册"错误
///
/// 这条规则让 plugin 内部短名引用自家组件最自然，又能透明地穿透到全局空间
/// （`Position`、`PickClick` 等）。
fn resolve_component_name<F>(plugin_name: &str, name: &str, exists: F) -> String
where
    F: Fn(&str) -> bool,
{
    if name.contains("::") {
        return name.to_owned();
    }
    if plugin_name != crate::world_module::ROOT_MODULE {
        let qualified = format!("{plugin_name}::{name}");
        if exists(&qualified) {
            return qualified;
        }
    }
    if exists(name) {
        return name.to_owned();
    }
    // 全没命中——返回原名让下游报清晰错误（包含原始短名）。
    name.to_owned()
}

/// 把全部 World 操作注册为 Rhai 宿主函数。
fn register_world_functions(
    engine: &mut Engine,
    slots: &Rc<Slots>,
    registry: &Rc<ComponentRegistry>,
    plugin_name: &Rc<str>,
    vm_id: VmId,
) {
    register_query(engine, slots, registry, plugin_name, vm_id);
    register_value_access(engine, slots, registry, plugin_name);
    register_lifecycle(engine, slots, vm_id);
}

/// 注册 `query(component) -> [entity]` 和 `has_component(entity, name)`。
fn register_query(
    engine: &mut Engine,
    slots: &Rc<Slots>,
    registry: &Rc<ComponentRegistry>,
    plugin_name: &Rc<str>,
    vm_id: VmId,
) {
    let slots_q = Rc::clone(slots);
    let reg_q = Rc::clone(registry);
    let plug_q = Rc::clone(plugin_name);
    engine.register_fn("query", move |component: &str| -> Vec<Dynamic> {
        let Some(world) = with_world_mut(&slots_q) else {
            return Vec::new();
        };
        let resolved = resolve_component_name(&plug_q, component, |n| reg_q.resolve(n).is_some());
        world_access::query_with_component_tagged(world, &reg_q, &resolved, vm_id)
            .into_iter()
            .map(|entity| Dynamic::from(encode_entity(entity)))
            .collect()
    });

    let slots_h = Rc::clone(slots);
    let reg_h = Rc::clone(registry);
    let plug_h = Rc::clone(plugin_name);
    engine.register_fn(
        "has_component",
        move |entity: ScriptEntity, component: &str| -> bool {
            let Some(world) = with_world_mut(&slots_h) else {
                return false;
            };
            let resolved =
                resolve_component_name(&plug_h, component, |n| reg_h.resolve(n).is_some());
            world_access::has_component(world, &reg_h, decode_entity(entity), &resolved)
        },
    );
}

/// 注册统一的值访问：`get` / `set` / `remove`，参数均为 `(entity, component, path)`。
fn register_value_access(
    engine: &mut Engine,
    slots: &Rc<Slots>,
    registry: &Rc<ComponentRegistry>,
    plugin_name: &Rc<str>,
) {
    let slots_get = Rc::clone(slots);
    let reg = Rc::clone(registry);
    let plug = Rc::clone(plugin_name);
    engine.register_fn(
        "get",
        move |entity: ScriptEntity,
              component: &str,
              path: &str|
              -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            // Position / Rotation 是 AI 友好的 alias——透明转发到 Bevy Transform
            // 的 translation / rotation。让旧 worlds 仍能 `get(e, "Position", "x")`
            // 而不必到处改成 get_translation()。
            #[cfg(feature = "bevy-bridge")]
            if let Some(value) = transform_alias_get(
                world_ref(&slots_get)?,
                decode_entity(entity),
                component,
                path,
            ) {
                return Ok(value);
            }
            let world = world_ref(&slots_get)?;
            let resolved = resolve_component_name(&plug, component, |n| reg.resolve(n).is_some());
            let value = world_access::get(world, &reg, decode_entity(entity), &resolved, path)
                .map_err(error_to_rhai)?;
            Ok(convert::to_dynamic(&value))
        },
    );

    let slots_set = Rc::clone(slots);
    let reg = Rc::clone(registry);
    let plug = Rc::clone(plugin_name);
    engine.register_fn(
        "set",
        move |entity: ScriptEntity,
              component: &str,
              path: &str,
              value: Dynamic|
              -> Result<(), Box<rhai::EvalAltResult>> {
            let ron_value = convert::from_dynamic(value).map_err(error_to_rhai)?;
            #[cfg(feature = "bevy-bridge")]
            if transform_alias_set(
                world_mut(&slots_set)?,
                decode_entity(entity),
                component,
                path,
                &ron_value,
            ) {
                return Ok(());
            }
            let world = world_mut(&slots_set)?;
            let resolved = resolve_component_name(&plug, component, |n| reg.resolve(n).is_some());
            world_access::set(
                world,
                &reg,
                decode_entity(entity),
                &resolved,
                path,
                ron_value,
            )
            .map_err(error_to_rhai)
        },
    );

    let slots_remove = Rc::clone(slots);
    let reg = Rc::clone(registry);
    let plug = Rc::clone(plugin_name);
    engine.register_fn(
        "remove",
        move |entity: ScriptEntity,
              component: &str,
              path: &str|
              -> Result<(), Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_remove)?;
            let resolved = resolve_component_name(&plug, component, |n| reg.resolve(n).is_some());
            world_access::remove(world, &reg, decode_entity(entity), &resolved, path)
                .map_err(error_to_rhai)
        },
    );
}

/// `Position.{x,y,z}` 透明读：转发到 Bevy `Transform.translation`。
/// 不存在 Transform 时返回 `Some(0.0)` —— 兼容旧 worlds "声明即存在" 语义
/// （脚本可在 set 之前 get）。
#[cfg(feature = "bevy-bridge")]
fn transform_alias_get(
    world: &World,
    entity: Entity,
    component: &str,
    path: &str,
) -> Option<Dynamic> {
    if component != "Position" && component != "Rotation" {
        return None;
    }
    let transform = world
        .get_entity(entity)
        .ok()
        .and_then(|e| e.get::<bevy::prelude::Transform>().copied())
        .unwrap_or_default();
    if component == "Position" {
        let v = match path {
            "x" => transform.translation.x as f64,
            "y" => transform.translation.y as f64,
            "z" => transform.translation.z as f64,
            _ => return None,
        };
        return Some(Dynamic::from_float(v));
    }
    // Rotation：读 quat 反推 YXZ 欧拉（弧度→度）。
    let (yaw, pitch, roll) = transform.rotation.to_euler(bevy::math::EulerRot::YXZ);
    let v = match path {
        "x" => pitch.to_degrees() as f64,
        "y" => yaw.to_degrees() as f64,
        "z" => roll.to_degrees() as f64,
        _ => return None,
    };
    Some(Dynamic::from_float(v))
}

/// `Position.{x,y,z}` / `Rotation.{x,y,z}` 透明写：原地改 Bevy Transform。
/// 缺 Transform 自动补一个 identity。命中并写入返回 true，否则 false 让 set
/// 走通用 reflect 路径。
#[cfg(feature = "bevy-bridge")]
fn transform_alias_set(
    world: &mut World,
    entity: Entity,
    component: &str,
    path: &str,
    value: &serde_json::Value,
) -> bool {
    use bevy::math::EulerRot;
    use bevy::prelude::{Quat, Transform};
    if component != "Position" && component != "Rotation" {
        return false;
    }
    let v = match value.as_f64() {
        Some(f) => f as f32,
        None => return false,
    };
    let mut em = match world.get_entity_mut(entity) {
        Ok(em) => em,
        Err(_) => return false,
    };
    let mut transform = em.get::<Transform>().copied().unwrap_or_default();
    if component == "Position" {
        match path {
            "x" => transform.translation.x = v,
            "y" => transform.translation.y = v,
            "z" => transform.translation.z = v,
            _ => return false,
        }
    } else {
        // Rotation：path 是 x/y/z（pitch/yaw/roll，单位度）。读现有 quat 拿
        // 当前三轴值，改一轴，再合成新 quat。
        let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);
        let mut yaw_d = yaw.to_degrees();
        let mut pitch_d = pitch.to_degrees();
        let mut roll_d = roll.to_degrees();
        match path {
            "x" => pitch_d = v,
            "y" => yaw_d = v,
            "z" => roll_d = v,
            _ => return false,
        }
        transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            yaw_d.to_radians(),
            pitch_d.to_radians(),
            roll_d.to_radians(),
        );
    }
    em.insert(transform);
    true
}

/// 注册实体生命周期：`spawn_entity() -> id`、`despawn(id) -> bool`、`is_alive(id) -> bool`。
fn register_lifecycle(engine: &mut Engine, slots: &Rc<Slots>, vm_id: VmId) {
    let slots_spawn = Rc::clone(slots);
    engine.register_fn(
        "spawn_entity",
        move || -> Result<ScriptEntity, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_spawn)?;
            let entity = world_access::spawn(world);
            world.entity_mut(entity).insert(VmTag::new(vm_id));
            Ok(encode_entity(entity))
        },
    );

    let slots_despawn = Rc::clone(slots);
    engine.register_fn(
        "despawn",
        move |entity: ScriptEntity| -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_despawn)?;
            Ok(world_access::despawn(world, decode_entity(entity)))
        },
    );

    let slots_alive = Rc::clone(slots);
    engine.register_fn(
        "is_alive",
        move |entity: ScriptEntity| -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_ref(&slots_alive)?;
            Ok(world_access::is_alive(world, decode_entity(entity)))
        },
    );

    // ---- 父子关系 ---------------------------------------------------------

    let slots_set_parent = Rc::clone(slots);
    engine.register_fn(
        "set_parent",
        move |child: ScriptEntity,
              parent: ScriptEntity|
              -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_set_parent)?;
            Ok(world_access::set_parent(
                world,
                decode_entity(child),
                decode_entity(parent),
            ))
        },
    );

    let slots_clear_parent = Rc::clone(slots);
    engine.register_fn(
        "clear_parent",
        move |child: ScriptEntity| -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_clear_parent)?;
            Ok(world_access::clear_parent(world, decode_entity(child)))
        },
    );

    let slots_parent_of = Rc::clone(slots);
    engine.register_fn(
        "parent_of",
        move |entity: ScriptEntity| -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            let world = world_ref(&slots_parent_of)?;
            Ok(
                match world_access::parent_of(world, decode_entity(entity)) {
                    Some(parent) => Dynamic::from(encode_entity(parent)),
                    None => Dynamic::UNIT,
                },
            )
        },
    );

    let slots_children_of = Rc::clone(slots);
    engine.register_fn(
        "children_of",
        move |entity: ScriptEntity| -> Result<Vec<Dynamic>, Box<rhai::EvalAltResult>> {
            let world = world_ref(&slots_children_of)?;
            Ok(world_access::children_of(world, decode_entity(entity))
                .into_iter()
                .map(|child| Dynamic::from(encode_entity(child)))
                .collect())
        },
    );
}

/// 注册事件相关函数：
/// - `emit(name, payload)`：把 payload 写入事件 `name` 的 back 缓冲。typed
///   通道走 Value→T 反序列化校验后入队 `Vec<T>`；dynamic 通道走默认合并后
///   原样入队 `Vec<Value>`。
/// - `events(name) -> [Dynamic]`：返回事件 `name` 当前 tick 的 front 缓冲
///   快照。typed 通道按需序列化每个 T 为 Value 再转 Dynamic；dynamic 通道
///   直接转。
fn register_event_functions(
    engine: &mut Engine,
    slots: &Rc<Slots>,
    registry: &Rc<EventRegistry>,
    plugin_name: &Rc<str>,
) {
    use crate::event::ChannelStorage;

    let slots_emit = Rc::clone(slots);
    let reg_emit = Rc::clone(registry);
    let plug_emit = Rc::clone(plugin_name);
    engine.register_fn(
        "emit",
        move |name: &str, payload: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
            let payload = convert::from_dynamic(payload).map_err(error_to_rhai)?;
            let store = events_mut(&slots_emit)?;
            let resolved =
                resolve_component_name(&plug_emit, name, |n| reg_emit.resolve(n).is_some());
            match reg_emit.resolve(&resolved) {
                Some(EventKind::Typed(typed)) => {
                    let merged = merge_with_default(payload, typed.default.as_ref());
                    let storage = store.storage_mut(&resolved).ok_or_else(|| {
                        into_rhai_error(format!("event `{resolved}` is not registered"))
                    })?;
                    let ChannelStorage::Typed(buffer) = storage else {
                        return Err(into_rhai_error(format!(
                            "event `{resolved}` channel kind mismatch"
                        )));
                    };
                    typed
                        .emit_from_value(buffer, &resolved, merged)
                        .map_err(error_to_rhai)
                }
                Some(EventKind::Dynamic(dyn_event)) => {
                    let merged = merge_with_default(payload, Some(&dyn_event.default));
                    store.push_dynamic(&resolved, merged).map_err(error_to_rhai)
                }
                None => Err(into_rhai_error(format!(
                    "event `{resolved}` is not registered"
                ))),
            }
        },
    );

    let slots_read = Rc::clone(slots);
    let reg_read = Rc::clone(registry);
    let plug_read = Rc::clone(plugin_name);
    engine.register_fn(
        "events",
        move |name: &str| -> Result<Vec<Dynamic>, Box<rhai::EvalAltResult>> {
            let store = events_ref(&slots_read)?;
            let resolved =
                resolve_component_name(&plug_read, name, |n| reg_read.resolve(n).is_some());
            // 读端宽容：通道未注册时返回空数组而非报错。让脚本可以"假如有 X
            // 就处理"地监听 host plugin 提供的事件——headless 测试或裁剪
            // build 下 plugin 缺失时脚本不该崩。emit() 写端仍严格。
            let Some(storage) = store.storage(&resolved) else {
                tracing::debug!(
                    target: "bevy_vm::script",
                    "events(`{resolved}`): channel not registered, returning empty",
                );
                return Ok(Vec::new());
            };
            match (reg_read.resolve(&resolved), storage) {
                (Some(EventKind::Typed(typed)), ChannelStorage::Typed(buffer)) => {
                    let len = buffer.front_len();
                    let mut out = Vec::with_capacity(len);
                    for index in 0..len {
                        let value = typed
                            .serialize_front_at(buffer, index)
                            .map_err(error_to_rhai)?;
                        out.push(convert::to_dynamic(&value));
                    }
                    Ok(out)
                }
                (Some(EventKind::Dynamic(_)), ChannelStorage::Dynamic(buffer)) => {
                    Ok(buffer.current().iter().map(convert::to_dynamic).collect())
                }
                _ => Err(into_rhai_error(format!(
                    "event `{resolved}` channel kind mismatch"
                ))),
            }
        },
    );
}

/// 注册 `random()` / `random_range(min, max)` / `random_int(low, high)` 宿主。
///
/// 三者直接读写 [`Slots::rng`] 槽——RNG 是 per-instance 的，不挂在 World 资源
/// 上避免多 VM 互相污染。每帧 [`crate::VmInstance::tick`] 通过
/// [`crate::system::TickContext::rng`] 把当前 VM 的 RNG 借进来。
fn register_random(engine: &mut Engine, slots: &Rc<Slots>) {
    let slots_f = Rc::clone(slots);
    engine.register_fn(
        "random",
        move || -> Result<f64, Box<rhai::EvalAltResult>> {
            let rng = rng_mut(&slots_f)?;
            Ok(rng.next_f64())
        },
    );

    let slots_r = Rc::clone(slots);
    engine.register_fn(
        "random_range",
        move |min: f64, max: f64| -> Result<f64, Box<rhai::EvalAltResult>> {
            let rng = rng_mut(&slots_r)?;
            Ok(rng.next_f64_range(min, max))
        },
    );

    let slots_i = Rc::clone(slots);
    engine.register_fn(
        "random_int",
        move |low: i64, high: i64| -> Result<i64, Box<rhai::EvalAltResult>> {
            let rng = rng_mut(&slots_i)?;
            Ok(rng.next_i64_range(low, high))
        },
    );
}

/// 注册 `time()` / `delta()` / `pause()` / `resume()` / `is_paused()` 宿主。
///
/// 全部走 per-instance slots（[`Slots::time`] / [`Slots::pause`]），不再
/// 触碰 World 资源，避免多 VM 共用 World 时彼此撞 `Time<()>` /
/// `VmPauseState`。
fn register_time(engine: &mut Engine, slots: &Rc<Slots>) {
    let slots_t = Rc::clone(slots);
    engine.register_fn("time", move || -> Result<f64, Box<rhai::EvalAltResult>> {
        let time = time_ref(&slots_t)?;
        Ok(time.elapsed_secs_f64())
    });

    let slots_d = Rc::clone(slots);
    engine.register_fn("delta", move || -> Result<f64, Box<rhai::EvalAltResult>> {
        let time = time_ref(&slots_d)?;
        Ok(time.delta_secs_f64())
    });

    let slots_p = Rc::clone(slots);
    engine.register_fn("pause", move || -> Result<(), Box<rhai::EvalAltResult>> {
        pause_mut(&slots_p)?.paused = true;
        Ok(())
    });

    let slots_r = Rc::clone(slots);
    engine.register_fn("resume", move || -> Result<(), Box<rhai::EvalAltResult>> {
        pause_mut(&slots_r)?.paused = false;
        Ok(())
    });

    let slots_ip = Rc::clone(slots);
    engine.register_fn(
        "is_paused",
        move || -> Result<bool, Box<rhai::EvalAltResult>> { Ok(pause_ref(&slots_ip)?.paused) },
    );
}

/// 注册 UI / 颜色构造 helper（脚本端常用 dict 构造器）。
fn register_ui_helpers(engine: &mut Engine) {
    use rhai::FLOAT;
    use rhai::Map;

    fn key(s: &str) -> rhai::ImmutableString {
        rhai::ImmutableString::from(s)
    }

    fn wrap(variant: &str, value: Dynamic) -> Map {
        let mut m: Map = Map::new();
        m.insert(key(variant).into(), value);
        m
    }

    engine.register_fn("srgba", move |r: FLOAT, g: FLOAT, b: FLOAT, a: FLOAT| {
        let mut inner: Map = Map::new();
        inner.insert(key("red").into(), Dynamic::from_float(r));
        inner.insert(key("green").into(), Dynamic::from_float(g));
        inner.insert(key("blue").into(), Dynamic::from_float(b));
        inner.insert(key("alpha").into(), Dynamic::from_float(a));
        wrap("Srgba", Dynamic::from(inner))
    });
    engine.register_fn("srgb", move |r: FLOAT, g: FLOAT, b: FLOAT| {
        let mut inner: Map = Map::new();
        inner.insert(key("red").into(), Dynamic::from_float(r));
        inner.insert(key("green").into(), Dynamic::from_float(g));
        inner.insert(key("blue").into(), Dynamic::from_float(b));
        inner.insert(key("alpha").into(), Dynamic::from_float(1.0));
        wrap("Srgba", Dynamic::from(inner))
    });
    engine.register_fn("px", move |v: FLOAT| wrap("Px", Dynamic::from_float(v)));
    engine.register_fn("percent", move |v: FLOAT| {
        wrap("Percent", Dynamic::from_float(v))
    });
}

/// Bevy-bridge-only host functions: `attach_mesh / attach_pbr / attach_sprite
/// / attach_camera_3d / attach_camera_2d / attach_text / attach_scene
/// / set_transform / set_translation`。直接在主 World 操作 Bevy 原生组件。
#[cfg(feature = "bevy-bridge")]
#[allow(dead_code, unused_imports, clippy::collapsible_if)]
mod render_host {
    use super::{
        Slots, VmId, VmTag, decode_entity, error_to_rhai, into_rhai_error, with_world_mut,
        world_mut,
    };
    use bevy::asset::{AssetServer, Assets, Handle};
    use bevy::color::Color;
    use bevy::math::{Vec2, Vec3};
    use bevy::prelude::{
        Camera, Camera2d, Camera3d, ClearColorConfig, Mesh, Mesh3d, MeshMaterial3d,
        OrthographicProjection, PerspectiveProjection, Projection, Quat, Sprite, StandardMaterial,
        Transform,
    };
    use bevy::scene::{Scene, SceneRoot};
    use bevy::sprite::Text2d;
    use bevy::text::{TextColor, TextFont};
    use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
    use bevy_ecs::query::With;
    use bevy_ecs::resource::Resource;
    use rhai::{Dynamic, Engine, FLOAT, Map};
    use std::rc::Rc;

    /// 资源 handle 缓存——按 cache key 复用同一个 Bevy `Handle`。同 VM 多次
    /// 调用同样参数的 `cuboid(1,2,3)` 共享同一个 Mesh asset。
    #[derive(Resource, Default)]
    pub struct AttachCache {
        meshes: std::collections::HashMap<String, Handle<Mesh>>,
        materials: std::collections::HashMap<String, Handle<StandardMaterial>>,
    }

    impl AttachCache {
        /// 返回内部 mesh map（render plugin 切换 world 时清场用）。
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.meshes.is_empty() && self.materials.is_empty()
        }
    }

    /// Decode a `Map` value into a Bevy `Color` via reflect-style enum form
    /// `#{Srgba: #{red,green,blue,alpha}}`. Falls back to white on shape mismatch.
    fn color_from_dynamic(value: Dynamic) -> Color {
        let Some(map) = try_into_map(value) else {
            return Color::WHITE;
        };
        for (variant, payload) in map {
            let Some(inner) = try_into_map(payload) else {
                continue;
            };
            let r = float_field(&inner, "red");
            let g = float_field(&inner, "green");
            let b = float_field(&inner, "blue");
            let a = float_field(&inner, "alpha").unwrap_or(1.0);
            let r = r.unwrap_or(1.0);
            let g = g.unwrap_or(1.0);
            let b = b.unwrap_or(1.0);
            return match variant.as_str() {
                "Srgba" => Color::srgba(r, g, b, a),
                "LinearRgba" => Color::linear_rgba(r, g, b, a),
                _ => Color::srgba(r, g, b, a),
            };
        }
        Color::WHITE
    }

    fn float_field(map: &Map, name: &str) -> Option<f32> {
        map.get(name).and_then(|v| {
            if let Ok(f) = v.as_float() {
                Some(f as f32)
            } else if let Ok(i) = v.as_int() {
                Some(i as f32)
            } else {
                None
            }
        })
    }

    fn vec3_field(map: &Map, name: &str) -> Option<Vec3> {
        let value = map.get(name)?.clone();
        if let Some(arr) = try_into_array(value.clone()) {
            if arr.len() == 3 {
                let x = arr[0]
                    .as_float()
                    .ok()
                    .map(|f| f as f32)
                    .or_else(|| arr[0].as_int().ok().map(|i| i as f32))?;
                let y = arr[1]
                    .as_float()
                    .ok()
                    .map(|f| f as f32)
                    .or_else(|| arr[1].as_int().ok().map(|i| i as f32))?;
                let z = arr[2]
                    .as_float()
                    .ok()
                    .map(|f| f as f32)
                    .or_else(|| arr[2].as_int().ok().map(|i| i as f32))?;
                return Some(Vec3::new(x, y, z));
            }
        }
        if let Some(inner) = try_into_map(value) {
            let x = float_field(&inner, "x")?;
            let y = float_field(&inner, "y")?;
            let z = float_field(&inner, "z")?;
            return Some(Vec3::new(x, y, z));
        }
        None
    }

    fn try_into_map(value: Dynamic) -> Option<Map> {
        if value.is::<Map>() {
            value.try_cast::<Map>()
        } else {
            None
        }
    }

    fn try_into_array(value: Dynamic) -> Option<rhai::Array> {
        if value.is::<rhai::Array>() {
            value.try_cast::<rhai::Array>()
        } else {
            None
        }
    }

    fn ensure_cache(world: &mut bevy_ecs::world::World) -> &mut AttachCache {
        if !world.contains_resource::<AttachCache>() {
            world.insert_resource(AttachCache::default());
        }
        world.resource_mut::<AttachCache>().into_inner()
    }

    fn ensure_tagged(
        world: &mut bevy_ecs::world::World,
        entity: bevy_ecs::entity::Entity,
        vm_id: VmId,
    ) -> Result<(), Box<rhai::EvalAltResult>> {
        let mut em = world
            .get_entity_mut(entity)
            .map_err(|_| into_rhai_error(format!("entity {entity:?} does not exist")))?;
        em.insert(VmTag::new(vm_id));
        Ok(())
    }

    pub fn register_render_attach(engine: &mut Engine, slots: &Rc<Slots>, vm_id: VmId) {
        // ---- attach_mesh(entity, spec) ----------------------------------
        // spec 形如 #{Cuboid: [w,h,d]} / #{Sphere: r} / #{Cylinder: [r,h]}
        // / #{Plane: [w,h]} / #{Tetrahedron: edge}。
        let slots_m = Rc::clone(slots);
        engine.register_fn(
            "attach_mesh",
            move |entity: i64, spec: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_m)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let handle = build_mesh_handle(world, &spec)?;
                world.entity_mut(entity).insert(Mesh3d(handle));
                Ok(())
            },
        );

        // ---- attach_pbr(entity, material_dict) --------------------------
        // material_dict 形如 #{base_color: srgba(...), metallic: 0.0, roughness: 0.5,
        //                     emissive: srgba(...), alpha_mode: "Opaque" / "Blend"}
        let slots_pbr = Rc::clone(slots);
        engine.register_fn(
            "attach_pbr",
            move |entity: i64, material: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_pbr)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let handle = build_pbr_handle(world, material)?;
                world.entity_mut(entity).insert(MeshMaterial3d(handle));
                Ok(())
            },
        );

        // ---- attach_sprite(entity, dict) --------------------------------
        // dict 形如 #{color: srgba(...), custom_size: [w,h], image: "path"}
        let slots_sp = Rc::clone(slots);
        engine.register_fn(
            "attach_sprite",
            move |entity: i64, spec: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_sp)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let sprite = build_sprite(world, spec);
                world.entity_mut(entity).insert(sprite);
                Ok(())
            },
        );

        // ---- attach_camera_3d(entity, opts) -----------------------------
        // opts 形如 #{projection: "perspective" / "orthographic",
        //            fov_degrees: 60, near: 0.1, far: 1000.0,
        //            target: [x,y,z], up: [0,1,0], order: 0,
        //            clear_color: srgba(...), active: true}
        let slots_c3 = Rc::clone(slots);
        engine.register_fn(
            "attach_camera_3d",
            move |entity: i64, opts: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_c3)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let (camera, projection, transform) = build_camera_3d(opts);
                world.entity_mut(entity).insert((
                    Camera3d::default(),
                    camera,
                    projection,
                    transform,
                ));
                Ok(())
            },
        );

        // ---- attach_camera_2d(entity, opts) -----------------------------
        let slots_c2 = Rc::clone(slots);
        engine.register_fn(
            "attach_camera_2d",
            move |entity: i64, opts: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_c2)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let (camera, projection, transform) = build_camera_2d(opts);
                world
                    .entity_mut(entity)
                    .insert((Camera2d, camera, projection, transform));
                Ok(())
            },
        );

        // ---- attach_text(entity, content, font_size, color) -------------
        let slots_t = Rc::clone(slots);
        engine.register_fn(
            "attach_text",
            move |entity: i64,
                  content: &str,
                  font_size: FLOAT,
                  color: Dynamic|
                  -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_t)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                world.entity_mut(entity).insert((
                    Text2d::new(content.to_owned()),
                    TextFont {
                        font_size: font_size as f32,
                        ..Default::default()
                    },
                    TextColor(color_from_dynamic(color)),
                ));
                Ok(())
            },
        );

        // ---- attach_scene(entity, asset_path) ---------------------------
        let slots_sc = Rc::clone(slots);
        engine.register_fn(
            "attach_scene",
            move |entity: i64, asset_path: &str| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_sc)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let server = world
                    .get_resource::<AssetServer>()
                    .ok_or_else(|| into_rhai_error("AssetServer missing".to_owned()))?;
                let handle: Handle<Scene> = server.load(asset_path.to_owned());
                world.entity_mut(entity).insert(SceneRoot(handle));
                Ok(())
            },
        );

        // ---- set_transform(entity, translation, euler_xyz_deg, scale) ---
        let slots_tr = Rc::clone(slots);
        engine.register_fn(
            "set_transform",
            move |entity: i64,
                  translation: Dynamic,
                  euler_deg: Dynamic,
                  scale: Dynamic|
                  -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_tr)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let translation = vec3_from(translation).unwrap_or(Vec3::ZERO);
                let euler = vec3_from(euler_deg).unwrap_or(Vec3::ZERO);
                let scale = vec3_from(scale).unwrap_or(Vec3::ONE);
                let rotation = Quat::from_euler(
                    bevy::math::EulerRot::YXZ,
                    euler.y.to_radians(),
                    euler.x.to_radians(),
                    euler.z.to_radians(),
                );
                world.entity_mut(entity).insert(Transform {
                    translation,
                    rotation,
                    scale,
                });
                Ok(())
            },
        );

        // ---- set_translation(entity, [x,y,z]) ---------------------------
        let slots_st = Rc::clone(slots);
        engine.register_fn(
            "set_translation",
            move |entity: i64, translation: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_st)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                let translation = vec3_from(translation).unwrap_or(Vec3::ZERO);
                let mut em = world
                    .get_entity_mut(entity)
                    .map_err(|_| into_rhai_error("entity gone".to_owned()))?;
                if let Some(mut t) = em.get_mut::<Transform>() {
                    t.translation = translation;
                } else {
                    em.insert(Transform::from_translation(translation));
                }
                Ok(())
            },
        );

        // ---- get_translation(entity) -> [x,y,z] -------------------------
        let slots_gt = Rc::clone(slots);
        engine.register_fn(
            "get_translation",
            move |entity: i64| -> Result<rhai::Array, Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_gt)?;
                let entity = decode_entity(entity);
                let t = world
                    .get_entity(entity)
                    .ok()
                    .and_then(|e| e.get::<Transform>().copied())
                    .unwrap_or_default();
                Ok(vec![
                    Dynamic::from_float(t.translation.x as FLOAT),
                    Dynamic::from_float(t.translation.y as FLOAT),
                    Dynamic::from_float(t.translation.z as FLOAT),
                ])
            },
        );

        // ---- set_yaw(entity, radians) — 设置仅绕 Y 轴的旋转 -------------
        let slots_sy = Rc::clone(slots);
        engine.register_fn(
            "set_yaw",
            move |entity: i64, yaw_rad: FLOAT| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_sy)?;
                let entity = decode_entity(entity);
                let rotation = Quat::from_rotation_y(yaw_rad as f32);
                let mut em = world
                    .get_entity_mut(entity)
                    .map_err(|_| into_rhai_error("entity gone".to_owned()))?;
                if let Some(mut t) = em.get_mut::<Transform>() {
                    t.rotation = rotation;
                } else {
                    em.insert(Transform::from_rotation(rotation));
                }
                Ok(())
            },
        );

        // ---- set_cursor_grab(bool) —— 锁/解锁主窗口光标。
        // true  → CursorGrabMode::Locked + visible=false（FPS / 鼠标视角模式）
        // false → CursorGrabMode::None   + visible=true  （正常自由光标）
        // 直接动主世界 PrimaryWindow.CursorOptions——单 World 架构下脚本拥有
        // 这个权限，省掉 cursor plugin / 事件桥那一跳。
        let slots_cg = Rc::clone(slots);
        engine.register_fn(
            "set_cursor_grab",
            move |grab: bool| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_cg)?;
                let mut q = world.query_filtered::<&mut CursorOptions, With<PrimaryWindow>>();
                if let Ok(mut cursor) = q.single_mut(world) {
                    if grab {
                        cursor.grab_mode = CursorGrabMode::Locked;
                        cursor.visible = false;
                    } else {
                        cursor.grab_mode = CursorGrabMode::None;
                        cursor.visible = true;
                    }
                }
                Ok(())
            },
        );

        // ---- is_cursor_grabbed() -> bool —— 当前光标是否被锁定。
        let slots_cs = Rc::clone(slots);
        engine.register_fn(
            "is_cursor_grabbed",
            move || -> Result<bool, Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_cs)?;
                let mut q = world.query_filtered::<&CursorOptions, With<PrimaryWindow>>();
                Ok(q.single(world)
                    .map(|c| c.grab_mode != CursorGrabMode::None)
                    .unwrap_or(false))
            },
        );

        // ---- set_sprite_color(entity, color) ----------------------------
        // entity 必须已 attach_sprite——直接改 Bevy Sprite.color。
        let slots_sc = Rc::clone(slots);
        engine.register_fn(
            "set_sprite_color",
            move |entity: i64, color: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_sc)?;
                let entity = decode_entity(entity);
                let new_color = color_from_dynamic(color);
                let mut em = world
                    .get_entity_mut(entity)
                    .map_err(|_| into_rhai_error("entity gone".to_owned()))?;
                if let Some(mut sprite) = em.get_mut::<Sprite>() {
                    sprite.color = new_color;
                }
                Ok(())
            },
        );

        // ---- set_text_content(entity, str) -------------------------------
        // 改 Text2d 的字符串内容。
        let slots_tc = Rc::clone(slots);
        engine.register_fn(
            "set_text_content",
            move |entity: i64, content: &str| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_tc)?;
                let entity = decode_entity(entity);
                let mut em = world
                    .get_entity_mut(entity)
                    .map_err(|_| into_rhai_error("entity gone".to_owned()))?;
                if let Some(mut text) = em.get_mut::<Text2d>() {
                    if text.0 != content {
                        text.0 = content.to_owned();
                    }
                }
                Ok(())
            },
        );

        // ---- look_at(entity, target_xyz, up_xyz) ------------------------
        // 设 entity 的 Transform 为"位置不动、旋转使 -Z 指向 target"。
        // 相机跟随的常用工具——之前 Camera3d.target 的等价。
        let slots_la = Rc::clone(slots);
        engine.register_fn(
            "look_at",
            move |entity: i64,
                  target: Dynamic,
                  up: Dynamic|
                  -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_la)?;
                let entity = decode_entity(entity);
                let target = vec3_from(target).unwrap_or(Vec3::ZERO);
                let up = vec3_from(up).unwrap_or(Vec3::Y);
                let mut em = world
                    .get_entity_mut(entity)
                    .map_err(|_| into_rhai_error("entity gone".to_owned()))?;
                let translation = em
                    .get::<Transform>()
                    .map(|t| t.translation)
                    .unwrap_or(Vec3::ZERO);
                em.insert(Transform::from_translation(translation).looking_at(target, up));
                Ok(())
            },
        );

        // ---- attach_pickable(entity) -------------------------------------
        // 显式给 entity 挂 Bevy 的 picking Pickable——大多数情况下 mesh /
        // sprite picking 后端不需要这个（默认 opt-in），但 UI 按钮 / 自定义
        // 拾取规则需要。
        let slots_pk = Rc::clone(slots);
        engine.register_fn(
            "attach_pickable",
            move |entity: i64| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_pk)?;
                let entity = decode_entity(entity);
                ensure_tagged(world, entity, vm_id)?;
                world
                    .entity_mut(entity)
                    .insert(bevy::picking::Pickable::default());
                Ok(())
            },
        );

        // ---- set_text_color(entity, color) -------------------------------
        let slots_tcl = Rc::clone(slots);
        engine.register_fn(
            "set_text_color",
            move |entity: i64, color: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let world = world_mut(&slots_tcl)?;
                let entity = decode_entity(entity);
                let new_color = color_from_dynamic(color);
                let mut em = world
                    .get_entity_mut(entity)
                    .map_err(|_| into_rhai_error("entity gone".to_owned()))?;
                if let Some(mut tc) = em.get_mut::<TextColor>() {
                    *tc = TextColor(new_color);
                }
                Ok(())
            },
        );
    }

    fn build_mesh_handle(
        world: &mut bevy_ecs::world::World,
        spec: &Dynamic,
    ) -> Result<Handle<Mesh>, Box<rhai::EvalAltResult>> {
        let map = try_into_map(spec.clone())
            .ok_or_else(|| into_rhai_error("attach_mesh: spec must be a #{...} map".to_owned()))?;
        let (variant, payload) = map
            .into_iter()
            .next()
            .ok_or_else(|| into_rhai_error("attach_mesh: empty spec".to_owned()))?;
        let cache_key = format!("{}:{:?}", variant, payload);
        if !world.contains_resource::<AttachCache>() {
            world.insert_resource(AttachCache::default());
        }
        if let Some(handle) = world.resource::<AttachCache>().meshes.get(&cache_key) {
            return Ok(handle.clone());
        }
        let f32_at = |arr: &rhai::Array, idx: usize, default: f32| -> f32 {
            arr.get(idx)
                .and_then(|v| {
                    v.as_float()
                        .ok()
                        .or_else(|| v.as_int().ok().map(|i| i as f64))
                })
                .map(|v| v as f32)
                .unwrap_or(default)
        };
        let mesh = match variant.as_str() {
            "Cuboid" | "Cube" => {
                let arr = try_into_array(payload).unwrap_or_default();
                let w = f32_at(&arr, 0, 1.0);
                let h = f32_at(&arr, 1, 1.0);
                let d = f32_at(&arr, 2, 1.0);
                bevy::prelude::Mesh::from(bevy::math::primitives::Cuboid::new(w, h, d))
            }
            "Sphere" => {
                let r = if let Ok(f) = payload.as_float() {
                    f as f32
                } else if let Ok(i) = payload.as_int() {
                    i as f32
                } else if let Some(m) = try_into_map(payload.clone()) {
                    float_field(&m, "radius").unwrap_or(0.5)
                } else {
                    0.5
                };
                bevy::prelude::Mesh::from(bevy::math::primitives::Sphere::new(r))
            }
            "Cylinder" => {
                let arr = try_into_array(payload).unwrap_or_default();
                let r = f32_at(&arr, 0, 0.5);
                let h = f32_at(&arr, 1, 1.0);
                bevy::prelude::Mesh::from(bevy::math::primitives::Cylinder::new(r, h))
            }
            "Capsule" => {
                let arr = try_into_array(payload).unwrap_or_default();
                let r = f32_at(&arr, 0, 0.5);
                let h = f32_at(&arr, 1, 1.0);
                bevy::prelude::Mesh::from(bevy::math::primitives::Capsule3d::new(r, h))
            }
            "Cone" => {
                let arr = try_into_array(payload).unwrap_or_default();
                let r = f32_at(&arr, 0, 0.5);
                let h = f32_at(&arr, 1, 1.0);
                bevy::prelude::Mesh::from(bevy::math::primitives::Cone {
                    radius: r,
                    height: h,
                })
            }
            "ConicalFrustum" => {
                let arr = try_into_array(payload).unwrap_or_default();
                let r_top = f32_at(&arr, 0, 0.3);
                let r_bot = f32_at(&arr, 1, 0.5);
                let h = f32_at(&arr, 2, 1.0);
                bevy::prelude::Mesh::from(bevy::math::primitives::ConicalFrustum {
                    radius_top: r_top,
                    radius_bottom: r_bot,
                    height: h,
                })
            }
            "Torus" => {
                let arr = try_into_array(payload).unwrap_or_default();
                let inner = f32_at(&arr, 0, 0.3);
                let outer = f32_at(&arr, 1, 0.8);
                bevy::prelude::Mesh::from(bevy::math::primitives::Torus::new(inner, outer))
            }
            "Plane" => {
                // 地板：法线朝 +Y、横铺在 XZ。Plane3d 是 Bevy 自带的水平面
                // 原语；旧版用 Rectangle 是 XY 平面（立面），导致相机俯视
                // 只能看到边缘，玩家"穿过地板"。
                // 入参约定：[w, h] = 完整宽 / 深（沿 X / Z），与 size 直观。
                let arr = try_into_array(payload).unwrap_or_default();
                let w = f32_at(&arr, 0, 10.0);
                let h = f32_at(&arr, 1, 10.0);
                bevy::prelude::Mesh::from(bevy::math::primitives::Plane3d::new(
                    Vec3::Y,
                    Vec2::new(w * 0.5, h * 0.5),
                ))
            }
            "Tetrahedron" => {
                let edge = if let Ok(f) = payload.as_float() {
                    f as f32
                } else if let Ok(i) = payload.as_int() {
                    i as f32
                } else {
                    1.0
                };
                bevy::prelude::Mesh::from(bevy::math::primitives::Tetrahedron::default())
                    .scaled_by(Vec3::splat(edge))
            }
            other => {
                return Err(into_rhai_error(format!(
                    "attach_mesh: unknown primitive `{other}`"
                )));
            }
        };
        let handle = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            meshes.add(mesh)
        };
        world
            .resource_mut::<AttachCache>()
            .meshes
            .insert(cache_key, handle.clone());
        Ok(handle)
    }

    fn build_pbr_handle(
        world: &mut bevy_ecs::world::World,
        material: Dynamic,
    ) -> Result<Handle<StandardMaterial>, Box<rhai::EvalAltResult>> {
        let map = try_into_map(material).unwrap_or_default();
        let mut mat = StandardMaterial::default();
        if let Some(color) = map.get("base_color") {
            mat.base_color = color_from_dynamic(color.clone());
        }
        if let Some(v) = float_field(&map, "metallic") {
            mat.metallic = v;
        }
        if let Some(v) = float_field(&map, "roughness") {
            mat.perceptual_roughness = v;
        }
        if let Some(emissive) = map.get("emissive") {
            mat.emissive = color_from_dynamic(emissive.clone()).into();
        }
        let handle = {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            materials.add(mat)
        };
        Ok(handle)
    }

    fn build_sprite(world: &mut bevy_ecs::world::World, spec: Dynamic) -> Sprite {
        let map = try_into_map(spec).unwrap_or_default();
        let color = map
            .get("color")
            .map(|c| color_from_dynamic(c.clone()))
            .unwrap_or(Color::WHITE);
        let custom_size = map
            .get("custom_size")
            .and_then(|v| try_into_array(v.clone()))
            .and_then(|a| {
                let w = a.first().and_then(|v| v.as_float().ok())? as f32;
                let h = a.get(1).and_then(|v| v.as_float().ok())? as f32;
                Some(Vec2::new(w, h))
            });
        let image = map.get("image").and_then(|v| v.clone().into_string().ok());
        let mut sprite = if let Some(path) = image {
            let server = world.resource::<AssetServer>();
            Sprite {
                image: server.load(path),
                color,
                ..Default::default()
            }
        } else {
            Sprite::from_color(color, custom_size.unwrap_or(Vec2::ONE))
        };
        if let Some(size) = custom_size {
            sprite.custom_size = Some(size);
        }
        sprite
    }

    fn build_camera_3d(opts: Dynamic) -> (Camera, Projection, Transform) {
        let map = try_into_map(opts).unwrap_or_default();
        let proj_kind = map
            .get("projection")
            .and_then(|v| v.clone().into_string().ok())
            .unwrap_or_else(|| "perspective".to_owned());
        let projection = if proj_kind == "orthographic" {
            let mut p = OrthographicProjection::default_3d();
            if let Some(s) = float_field(&map, "scale") {
                p.scale = s;
            }
            if let Some(n) = float_field(&map, "near") {
                p.near = n;
            }
            if let Some(f) = float_field(&map, "far") {
                p.far = f;
            }
            if let Some(h) = float_field(&map, "viewport_height") {
                p.scaling_mode = bevy::camera::ScalingMode::FixedVertical { viewport_height: h };
            }
            Projection::Orthographic(p)
        } else {
            let fov = float_field(&map, "fov_degrees").unwrap_or(60.0);
            let near = float_field(&map, "near").unwrap_or(0.1);
            let far = float_field(&map, "far").unwrap_or(1000.0);
            Projection::Perspective(PerspectiveProjection {
                fov: fov.to_radians(),
                near,
                far,
                ..Default::default()
            })
        };

        let target = map
            .get("target")
            .and_then(|v| try_into_array(v.clone()))
            .and_then(|a| {
                let x = a.first().and_then(|v| v.as_float().ok())? as f32;
                let y = a.get(1).and_then(|v| v.as_float().ok())? as f32;
                let z = a.get(2).and_then(|v| v.as_float().ok())? as f32;
                Some(Vec3::new(x, y, z))
            });
        let position = map
            .get("position")
            .and_then(|v| try_into_array(v.clone()))
            .and_then(|a| {
                let x = a.first().and_then(|v| v.as_float().ok())? as f32;
                let y = a.get(1).and_then(|v| v.as_float().ok())? as f32;
                let z = a.get(2).and_then(|v| v.as_float().ok())? as f32;
                Some(Vec3::new(x, y, z))
            });
        let up = map
            .get("up")
            .and_then(|v| try_into_array(v.clone()))
            .and_then(|a| {
                let x = a.first().and_then(|v| v.as_float().ok())? as f32;
                let y = a.get(1).and_then(|v| v.as_float().ok())? as f32;
                let z = a.get(2).and_then(|v| v.as_float().ok())? as f32;
                Some(Vec3::new(x, y, z))
            })
            .unwrap_or(Vec3::Y);
        let transform = if let (Some(eye), Some(tgt)) = (position, target) {
            Transform::from_translation(eye).looking_at(tgt, up)
        } else {
            Transform::default()
        };

        let order = map
            .get("order")
            .and_then(|v| v.as_int().ok())
            .map(|i| isize::from(i16::try_from(i).unwrap_or(0)))
            .unwrap_or(0);
        let active = map
            .get("active")
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(true);
        let clear = map
            .get("clear_color")
            .map(|c| ClearColorConfig::Custom(color_from_dynamic(c.clone())))
            .unwrap_or(ClearColorConfig::Default);
        let camera = Camera {
            order,
            is_active: active,
            clear_color: clear,
            ..Default::default()
        };
        (camera, projection, transform)
    }

    fn build_camera_2d(opts: Dynamic) -> (Camera, Projection, Transform) {
        let map = try_into_map(opts).unwrap_or_default();
        let mut p = OrthographicProjection::default_2d();
        if let Some(s) = float_field(&map, "scale") {
            p.scale = s;
        }
        let projection = Projection::Orthographic(p);
        let transform = Transform::default();
        let order = map
            .get("order")
            .and_then(|v| v.as_int().ok())
            .map(|i| isize::from(i16::try_from(i).unwrap_or(0)))
            .unwrap_or(0);
        let active = map
            .get("active")
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(true);
        let clear = map
            .get("clear_color")
            .map(|c| ClearColorConfig::Custom(color_from_dynamic(c.clone())))
            .unwrap_or(ClearColorConfig::Default);
        let camera = Camera {
            order,
            is_active: active,
            clear_color: clear,
            ..Default::default()
        };
        (camera, projection, transform)
    }

    fn vec3_from(value: Dynamic) -> Option<Vec3> {
        if let Some(arr) = try_into_array(value.clone()) {
            if arr.len() == 3 {
                let x = arr[0]
                    .as_float()
                    .ok()
                    .map(|f| f as f32)
                    .or_else(|| arr[0].as_int().ok().map(|i| i as f32))?;
                let y = arr[1]
                    .as_float()
                    .ok()
                    .map(|f| f as f32)
                    .or_else(|| arr[1].as_int().ok().map(|i| i as f32))?;
                let z = arr[2]
                    .as_float()
                    .ok()
                    .map(|f| f as f32)
                    .or_else(|| arr[2].as_int().ok().map(|i| i as f32))?;
                return Some(Vec3::new(x, y, z));
            }
        }
        if let Some(map) = try_into_map(value) {
            let x = float_field(&map, "x")?;
            let y = float_field(&map, "y")?;
            let z = float_field(&map, "z")?;
            return Some(Vec3::new(x, y, z));
        }
        None
    }
}

/// 注册 `log(msg)` 宿主：把脚本的诊断输出转发到 `tracing::info`，便于在
/// 开了 `bevy_log` 的 App 里直接观察脚本行为。`msg` 可为任意 `Dynamic`，
/// 对非字符串值用 `Debug` 形式格式化。
fn register_logging(engine: &mut Engine) {
    engine.register_fn("log", |msg: Dynamic| {
        if let Ok(s) = msg.clone().into_string() {
            tracing::info!(target: "bevy_vm::script", "{}", s);
        } else {
            tracing::info!(target: "bevy_vm::script", "{:?}", msg);
        }
    });
}

/// 从槽取出 World 的可变引用；槽为空（无活跃 World）时返回 `None`。
///
/// 返回的可变引用刻意来自共享的 [`Rc`]：这正是「作用域裸指针」桥接的核心，
/// 借用安全由 `ScriptSystem::run` 的独占执行不变量保证，而非借用检查器。
#[allow(
    clippy::mut_from_ref,
    reason = "scoped raw pointer bridge; soundness ensured by exclusive single-threaded execution"
)]
fn with_world_mut(slots: &Rc<Slots>) -> Option<&mut World> {
    let ptr = slots.world.get();
    if ptr.is_null() {
        return None;
    }
    // SAFETY: see module doc — slot is non-null only during a single
    // `ScriptSystem::run`, which holds the exclusive `&mut World` and runs
    // single-threaded.
    Some(unsafe { &mut *ptr })
}

/// 从槽取出 EventStore 的可变引用；同样的「作用域裸指针」语义。
#[allow(
    clippy::mut_from_ref,
    reason = "scoped raw pointer bridge; soundness ensured by exclusive single-threaded execution"
)]
fn with_events_mut(slots: &Rc<Slots>) -> Option<&mut EventStore> {
    let ptr = slots.events.get();
    if ptr.is_null() {
        return None;
    }
    // SAFETY: see module doc.
    Some(unsafe { &mut *ptr })
}

/// 取 World 可变引用，槽为空时返回 Rhai 错误。
fn world_mut(slots: &Rc<Slots>) -> Result<&mut World, Box<rhai::EvalAltResult>> {
    with_world_mut(slots).ok_or_else(|| into_rhai_error(NO_ACTIVE_WORLD.to_owned()))
}

/// 取 World 只读引用，槽为空时返回 Rhai 错误。
fn world_ref(slots: &Rc<Slots>) -> Result<&World, Box<rhai::EvalAltResult>> {
    with_world_mut(slots)
        .map(|world| &*world)
        .ok_or_else(|| into_rhai_error(NO_ACTIVE_WORLD.to_owned()))
}

/// 取 EventStore 可变引用，槽为空时返回 Rhai 错误。
fn events_mut(slots: &Rc<Slots>) -> Result<&mut EventStore, Box<rhai::EvalAltResult>> {
    with_events_mut(slots).ok_or_else(|| into_rhai_error(NO_ACTIVE_EVENTS.to_owned()))
}

/// 取 EventStore 只读引用，槽为空时返回 Rhai 错误。
fn events_ref(slots: &Rc<Slots>) -> Result<&EventStore, Box<rhai::EvalAltResult>> {
    with_events_mut(slots)
        .map(|s| &*s)
        .ok_or_else(|| into_rhai_error(NO_ACTIVE_EVENTS.to_owned()))
}

const NO_ACTIVE_RNG: &str = "script host function called with no active VmRng";
const NO_ACTIVE_TIME: &str = "script host function called with no active Time";
const NO_ACTIVE_PAUSE: &str = "script host function called with no active Pause";

/// Get a `&mut VmRng` for the current tick, or a Rhai error if no tick is
/// active.
#[allow(
    clippy::mut_from_ref,
    reason = "scoped raw pointer bridge; soundness ensured by exclusive single-threaded execution"
)]
fn rng_mut(slots: &Rc<Slots>) -> Result<&mut VmRng, Box<rhai::EvalAltResult>> {
    let ptr = slots.rng.get();
    if ptr.is_null() {
        return Err(into_rhai_error(NO_ACTIVE_RNG.to_owned()));
    }
    // SAFETY: see module doc.
    Ok(unsafe { &mut *ptr })
}

/// Get a `&Time<()>` for the current tick.
fn time_ref(slots: &Rc<Slots>) -> Result<&Time<()>, Box<rhai::EvalAltResult>> {
    let ptr = slots.time.get();
    if ptr.is_null() {
        return Err(into_rhai_error(NO_ACTIVE_TIME.to_owned()));
    }
    // SAFETY: see module doc.
    Ok(unsafe { &*ptr })
}

/// Get a `&mut Pause` for the current tick.
#[allow(
    clippy::mut_from_ref,
    reason = "scoped raw pointer bridge; soundness ensured by exclusive single-threaded execution"
)]
fn pause_mut(slots: &Rc<Slots>) -> Result<&mut Pause, Box<rhai::EvalAltResult>> {
    let ptr = slots.pause.get();
    if ptr.is_null() {
        return Err(into_rhai_error(NO_ACTIVE_PAUSE.to_owned()));
    }
    // SAFETY: see module doc.
    Ok(unsafe { &mut *ptr })
}

/// Get a `&Pause` for the current tick.
fn pause_ref(slots: &Rc<Slots>) -> Result<&Pause, Box<rhai::EvalAltResult>> {
    pause_mut(slots).map(|p| &*p)
}

/// Wrap a host-side error string into a Rhai runtime error.
fn into_rhai_error(message: String) -> Box<rhai::EvalAltResult> {
    Box::new(rhai::EvalAltResult::ErrorRuntime(
        message.into(),
        rhai::Position::NONE,
    ))
}

/// Wrap any [`Display`](std::fmt::Display) error into a Rhai runtime error.
fn error_to_rhai<E: std::fmt::Display>(error: E) -> Box<rhai::EvalAltResult> {
    into_rhai_error(error.to_string())
}

/// 把实体编码为脚本侧整数。
fn encode_entity(entity: Entity) -> ScriptEntity {
    entity.to_bits() as ScriptEntity
}

/// 把脚本侧整数解码回实体。
fn decode_entity(value: ScriptEntity) -> Entity {
    Entity::from_bits(value as u64)
}

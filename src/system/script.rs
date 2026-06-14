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
use crate::system::System;
use crate::world_access;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
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

/// 实体在脚本侧的表示：实体位编码后的整数。
type ScriptEntity = i64;

/// 脚本运行期间共享的 `&mut` 槽：World + EventStore。
#[derive(Default)]
struct Slots {
    world: Cell<*mut World>,
    events: Cell<*mut EventStore>,
}

impl Slots {
    fn fill(&self, world: &mut World, events: &mut EventStore) {
        self.world.set(ptr::from_mut(world));
        self.events.set(ptr::from_mut(events));
    }

    fn clear(&self) {
        self.world.set(ptr::null_mut());
        self.events.set(ptr::null_mut());
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
        register_world_functions(&mut engine, &slots, &components, &plugin_name);
        register_event_functions(&mut engine, &slots, &events, &plugin_name);
        register_random(&mut engine, &slots);
        register_time(&mut engine, &slots);
        register_logging(&mut engine);

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
        })
    }
}

impl System for ScriptSystem {
    /// 在给定 World + EventStore 上执行一次脚本。
    ///
    /// run_if 表达式先 eval（共享同一 slots，能调 host 函数）；任一为 false
    /// 直接返回 Ok(()) 跳过 system body。
    ///
    /// 执行期间宿主函数可通过作用域指针读写两者；返回前指针槽被清空。
    fn run(&self, world: &mut World, events: &mut EventStore) -> Result<(), VmError> {
        self.slots.fill(world, events);
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
    if plugin_name != crate::plugin_loader::ROOT_PLUGIN {
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
) {
    register_query(engine, slots, registry, plugin_name);
    register_value_access(engine, slots, registry, plugin_name);
    register_lifecycle(engine, slots);
}

/// 注册 `query(component) -> [entity]` 和 `has_component(entity, name)`。
fn register_query(
    engine: &mut Engine,
    slots: &Rc<Slots>,
    registry: &Rc<ComponentRegistry>,
    plugin_name: &Rc<str>,
) {
    let slots_q = Rc::clone(slots);
    let reg_q = Rc::clone(registry);
    let plug_q = Rc::clone(plugin_name);
    engine.register_fn("query", move |component: &str| -> Vec<Dynamic> {
        let Some(world) = with_world_mut(&slots_q) else {
            return Vec::new();
        };
        let resolved = resolve_component_name(&plug_q, component, |n| reg_q.resolve(n).is_some());
        world_access::query_with_component(world, &reg_q, &resolved)
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

/// 注册实体生命周期：`spawn_entity() -> id`、`despawn(id) -> bool`、`is_alive(id) -> bool`。
fn register_lifecycle(engine: &mut Engine, slots: &Rc<Slots>) {
    let slots_spawn = Rc::clone(slots);
    engine.register_fn(
        "spawn_entity",
        move || -> Result<ScriptEntity, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_spawn)?;
            Ok(encode_entity(world_access::spawn(world)))
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
/// 三者都从 [`crate::random::VmRng`] 资源取——同一 VmWorld 共享一个
/// 决定性 RNG，配置可指定 seed 让脚本输出可重现。
///
/// - `random() -> f64`：均匀采样 `[0, 1)`。
/// - `random_range(min, max) -> f64`：均匀采样 `[min, max)`，`min >= max`
///   退化为 `min`。
/// - `random_int(low, high) -> i64`：均匀采样 `[low, high)`，`low >= high`
///   退化为 `low`。
fn register_random(engine: &mut Engine, slots: &Rc<Slots>) {
    let slots_f = Rc::clone(slots);
    engine.register_fn(
        "random",
        move || -> Result<f64, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_f)?;
            let mut rng = world
                .get_resource_mut::<crate::random::VmRng>()
                .ok_or_else(|| into_rhai_error("VmRng resource missing".to_owned()))?;
            Ok(rng.next_f64())
        },
    );

    let slots_r = Rc::clone(slots);
    engine.register_fn(
        "random_range",
        move |min: f64, max: f64| -> Result<f64, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_r)?;
            let mut rng = world
                .get_resource_mut::<crate::random::VmRng>()
                .ok_or_else(|| into_rhai_error("VmRng resource missing".to_owned()))?;
            Ok(rng.next_f64_range(min, max))
        },
    );

    let slots_i = Rc::clone(slots);
    engine.register_fn(
        "random_int",
        move |low: i64, high: i64| -> Result<i64, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slots_i)?;
            let mut rng = world
                .get_resource_mut::<crate::random::VmRng>()
                .ok_or_else(|| into_rhai_error("VmRng resource missing".to_owned()))?;
            Ok(rng.next_i64_range(low, high))
        },
    );
}

/// 注册 `time()` / `delta()` 宿主——直接读 [`bevy_time::Time`] 资源。
///
/// - `time() -> f64`：自世界启动累计秒数（[`Time::elapsed_secs_f64`]）。
/// - `delta() -> f64`：本帧 dt 秒数（[`Time::delta_secs_f64`]）。
///
/// 时间由 host 端通过 [`crate::VmWorld::advance_time`] 推进；headless
/// 测试若不 advance，则两者均返回 0，符合"决定性世界"承诺。
fn register_time(engine: &mut Engine, slots: &Rc<Slots>) {
    let slots_t = Rc::clone(slots);
    engine.register_fn("time", move || -> Result<f64, Box<rhai::EvalAltResult>> {
        let world = world_ref(&slots_t)?;
        Ok(world
            .get_resource::<bevy_time::Time>()
            .map(bevy_time::Time::elapsed_secs_f64)
            .unwrap_or(0.0))
    });

    let slots_d = Rc::clone(slots);
    engine.register_fn("delta", move || -> Result<f64, Box<rhai::EvalAltResult>> {
        let world = world_ref(&slots_d)?;
        Ok(world
            .get_resource::<bevy_time::Time>()
            .map(bevy_time::Time::delta_secs_f64)
            .unwrap_or(0.0))
    });

    // pause/resume/is_paused：仿 Bevy `Time<Virtual>::pause()`——脚本控制
    // VM 全局暂停。暂停期间所有依赖 delta() 的逻辑自然冻结。
    let slots_p = Rc::clone(slots);
    engine.register_fn("pause", move || -> Result<(), Box<rhai::EvalAltResult>> {
        let world = world_mut(&slots_p)?;
        if let Some(mut state) = world.get_resource_mut::<crate::vm::VmPauseState>() {
            state.paused = true;
        }
        Ok(())
    });

    let slots_r = Rc::clone(slots);
    engine.register_fn("resume", move || -> Result<(), Box<rhai::EvalAltResult>> {
        let world = world_mut(&slots_r)?;
        if let Some(mut state) = world.get_resource_mut::<crate::vm::VmPauseState>() {
            state.paused = false;
        }
        Ok(())
    });

    let slots_ip = Rc::clone(slots);
    engine.register_fn(
        "is_paused",
        move || -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_ref(&slots_ip)?;
            Ok(world
                .get_resource::<crate::vm::VmPauseState>()
                .map(|s| s.paused)
                .unwrap_or(false))
        },
    );
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

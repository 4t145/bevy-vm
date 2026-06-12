//! 脚本 system：把 World 操作暴露给 AI 生成的 Rhai 脚本。
//!
//! [`ScriptSystem`] 是 [`System`] 的一种实现，内部是一个注册好宿主函数的
//! `rhai::Engine` 和编译好的脚本 `AST`（载入时编译一次，每 tick 复跑）。脚本通过
//! 宿主函数 `query` / `get` / `set` / `add` / `get_prop` / `set_prop` / `remove_prop`
//! 读写世界。
//!
//! # 宿主函数如何安全地访问 `&mut World`
//!
//! Rhai 宿主函数闭包要求 `'static`，无法直接捕获 tick 传入的 `&mut World`。这里
//! 采用「作用域裸指针」桥接：执行前把 World 指针写入一个 `Rc<Cell<*mut World>>`
//! 槽，宿主函数运行时从槽取出指针解引用，执行完清空。该 unsafe 的安全性由架构
//! 保证：解释器**独占** World、**单线程同步**执行脚本，脚本运行全程指针有效且无
//! 并发别名。槽在非执行期间为空指针，宿主函数在空槽上返回错误而非解引用。

mod convert;

use crate::component::ComponentRegistry;
use crate::error::VmError;
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

/// 实体在脚本侧的表示：实体位编码后的整数。
type ScriptEntity = i64;

/// 运行一段 Rhai 脚本的 system。
pub struct ScriptSystem {
    engine: Engine,
    ast: AST,
    world_slot: Rc<Cell<*mut World>>,
}

impl ScriptSystem {
    /// 编译脚本并注册全部 World 宿主函数。
    ///
    /// `registry` 是该世界的组件注册表，宿主函数据此把组件名分发到两层访问路径。
    ///
    /// # Errors
    ///
    /// 脚本无法编译为合法 Rhai AST 时返回 [`VmError::ScriptCompile`]。
    pub fn compile(script: &str, registry: Rc<ComponentRegistry>) -> Result<Self, VmError> {
        let mut engine = Engine::new();
        engine.set_max_operations(MAX_OPERATIONS);
        engine.set_max_call_levels(MAX_CALL_LEVELS);

        let world_slot: Rc<Cell<*mut World>> = Rc::new(Cell::new(ptr::null_mut()));
        register_world_functions(&mut engine, &world_slot, &registry);

        let ast = engine
            .compile(script)
            .map_err(|e| VmError::ScriptCompile(e.to_string()))?;

        Ok(Self {
            engine,
            ast,
            world_slot,
        })
    }
}

impl System for ScriptSystem {
    /// 在给定 World 上执行一次脚本。
    ///
    /// 执行期间宿主函数可通过作用域指针读写该 World；返回前指针槽被清空。
    fn run(&self, world: &mut World) -> Result<(), VmError> {
        self.world_slot.set(ptr::from_mut(world));
        let mut scope = Scope::new();
        let result = self
            .engine
            .run_ast_with_scope(&mut scope, &self.ast)
            .map_err(|e| VmError::ScriptRuntime(e.to_string()));
        self.world_slot.set(ptr::null_mut());
        result
    }
}

/// 在空指针槽上访问 World 时的统一错误信息。
const NO_ACTIVE_WORLD: &str = "脚本宿主函数在无活跃 World 时被调用";

/// 把全部 World 操作注册为 Rhai 宿主函数。
fn register_world_functions(
    engine: &mut Engine,
    world_slot: &Rc<Cell<*mut World>>,
    registry: &Rc<ComponentRegistry>,
) {
    register_query(engine, world_slot, registry);
    register_value_access(engine, world_slot, registry);
    register_lifecycle(engine, world_slot);
}

/// 注册 `query(component) -> [entity]`。
fn register_query(
    engine: &mut Engine,
    world_slot: &Rc<Cell<*mut World>>,
    registry: &Rc<ComponentRegistry>,
) {
    let slot = Rc::clone(world_slot);
    let registry = Rc::clone(registry);
    engine.register_fn("query", move |component: &str| -> Vec<Dynamic> {
        let Some(world) = with_world_mut(&slot) else {
            return Vec::new();
        };
        world_access::query_with_component(world, &registry, component)
            .into_iter()
            .map(|entity| Dynamic::from(encode_entity(entity)))
            .collect()
    });
}

/// 注册统一的值访问：`get` / `set` / `add` / `remove`，参数均为 `(entity, component, path)`。
fn register_value_access(
    engine: &mut Engine,
    world_slot: &Rc<Cell<*mut World>>,
    registry: &Rc<ComponentRegistry>,
) {
    let slot = Rc::clone(world_slot);
    let reg = Rc::clone(registry);
    engine.register_fn(
        "get",
        move |entity: ScriptEntity,
              component: &str,
              path: &str|
              -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            let world = world_ref(&slot)?;
            let value = world_access::get(world, &reg, decode_entity(entity), component, path)
                .map_err(into_rhai_error)?;
            Ok(convert::to_dynamic(&value))
        },
    );

    let slot = Rc::clone(world_slot);
    let reg = Rc::clone(registry);
    engine.register_fn(
        "set",
        move |entity: ScriptEntity,
              component: &str,
              path: &str,
              value: Dynamic|
              -> Result<(), Box<rhai::EvalAltResult>> {
            let ron_value = convert::from_dynamic(value).map_err(into_rhai_error)?;
            let world = world_mut(&slot)?;
            world_access::set(
                world,
                &reg,
                decode_entity(entity),
                component,
                path,
                ron_value,
            )
            .map_err(into_rhai_error)
        },
    );

    let slot = Rc::clone(world_slot);
    let reg = Rc::clone(registry);
    engine.register_fn(
        "add",
        move |entity: ScriptEntity,
              component: &str,
              path: &str,
              delta: f64|
              -> Result<f64, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slot)?;
            world_access::add(world, &reg, decode_entity(entity), component, path, delta)
                .map_err(into_rhai_error)
        },
    );

    let slot = Rc::clone(world_slot);
    let reg = Rc::clone(registry);
    engine.register_fn(
        "remove",
        move |entity: ScriptEntity,
              component: &str,
              path: &str|
              -> Result<(), Box<rhai::EvalAltResult>> {
            let world = world_mut(&slot)?;
            world_access::remove(world, &reg, decode_entity(entity), component, path)
                .map_err(into_rhai_error)
        },
    );
}

/// 注册实体生命周期：`spawn_entity() -> id`、`despawn(id) -> bool`、`is_alive(id) -> bool`。
fn register_lifecycle(engine: &mut Engine, world_slot: &Rc<Cell<*mut World>>) {
    let slot = Rc::clone(world_slot);
    engine.register_fn(
        "spawn_entity",
        move || -> Result<ScriptEntity, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slot)?;
            Ok(encode_entity(world_access::spawn(world)))
        },
    );

    let slot = Rc::clone(world_slot);
    engine.register_fn(
        "despawn",
        move |entity: ScriptEntity| -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_mut(&slot)?;
            Ok(world_access::despawn(world, decode_entity(entity)))
        },
    );

    let slot = Rc::clone(world_slot);
    engine.register_fn(
        "is_alive",
        move |entity: ScriptEntity| -> Result<bool, Box<rhai::EvalAltResult>> {
            let world = world_ref(&slot)?;
            Ok(world_access::is_alive(world, decode_entity(entity)))
        },
    );
}

/// 从槽取出 World 的可变引用；槽为空（无活跃 World）时返回 `None`。
///
/// 返回的可变引用刻意来自共享的 [`Rc`]：这正是「作用域裸指针」桥接的核心，
/// 借用安全由 `ScriptRuntime::run` 的独占执行不变量保证，而非借用检查器。
#[allow(
    clippy::mut_from_ref,
    reason = "作用域裸指针桥接，安全性由独占单线程执行不变量保证"
)]
fn with_world_mut(slot: &Rc<Cell<*mut World>>) -> Option<&mut World> {
    let ptr = slot.get();
    if ptr.is_null() {
        return None;
    }
    // SAFETY: 槽非空仅在 `ScriptRuntime::run` 执行期间成立，此时调用方持有对该
    // World 的独占 `&mut`，且脚本单线程同步执行——不存在并发或别名访问。
    Some(unsafe { &mut *ptr })
}

/// 取 World 可变引用，槽为空时返回 Rhai 错误。
fn world_mut(slot: &Rc<Cell<*mut World>>) -> Result<&mut World, Box<rhai::EvalAltResult>> {
    with_world_mut(slot).ok_or_else(|| into_rhai_error(NO_ACTIVE_WORLD.to_owned()))
}

/// 取 World 只读引用，槽为空时返回 Rhai 错误。
fn world_ref(slot: &Rc<Cell<*mut World>>) -> Result<&World, Box<rhai::EvalAltResult>> {
    with_world_mut(slot)
        .map(|world| &*world)
        .ok_or_else(|| into_rhai_error(NO_ACTIVE_WORLD.to_owned()))
}

/// 把宿主侧错误字符串包成 Rhai 运行时错误。
fn into_rhai_error(message: String) -> Box<rhai::EvalAltResult> {
    Box::new(rhai::EvalAltResult::ErrorRuntime(
        message.into(),
        rhai::Position::NONE,
    ))
}

/// 把实体编码为脚本侧整数。
fn encode_entity(entity: Entity) -> ScriptEntity {
    entity.to_bits() as ScriptEntity
}

/// 把脚本侧整数解码回实体。
fn decode_entity(value: ScriptEntity) -> Entity {
    Entity::from_bits(value as u64)
}

//! World access primitives shared by script host functions and config loading.
//!
//! All value access uses **dotted paths** (e.g. `value`, `slots.0.kind`) to
//! locate fields inside a component. Both component layers share the same
//! syntax:
//! - Engine-level typed components are first serialized to [`ron::Value`]
//!   through [`crate::component::TypedComponent`], navigated via [`path`],
//!   then deserialized back into the component on write.
//! - Content-level dynamic components apply [`path`] directly to the stored
//!   [`ron::Value`].
//!
//! Script authors use one path syntax across both layers without needing to
//! know which layer a component lives on.

pub mod path;

use crate::component::typed::TypedComponentError;
use crate::component::{ComponentKind, ComponentRegistry, TypedComponent};
use bevy_ecs::prelude::*;
use path::{PathError, ValuePathExt};
use serde_json::Value;
use thiserror::Error;

/// Errors raised by the public [`world_access`](self) API.
#[derive(Debug, Error)]
pub enum WorldAccessError {
    /// The target entity has been despawned or never existed.
    #[error("entity {entity:?} does not exist or has been despawned")]
    EntityNotFound {
        /// The offending entity id.
        entity: Entity,
    },
    /// Component name is not registered with the [`ComponentRegistry`].
    #[error("component `{component}` is not registered")]
    UnknownComponent {
        /// Component name as provided by config / script.
        component: String,
    },
    /// Entity does not carry the requested component.
    #[error("entity {entity:?} does not carry component `{component}`")]
    ComponentNotPresent {
        /// Entity in question.
        entity: Entity,
        /// Component name.
        component: String,
    },
    /// `remove` was called on a typed component (whose shape is fixed at
    /// compile time and therefore does not support field deletion).
    #[error("typed component `{component}` does not support field removal")]
    TypedRemoveUnsupported {
        /// Component name.
        component: String,
    },
    /// A typed component became unreadable immediately after being inserted —
    /// indicates a serialization issue rather than user error.
    #[error("typed component still unreadable on entity {entity:?} after insertion")]
    TypedReadAfterInsert {
        /// Entity that was just populated.
        entity: Entity,
    },
    /// Path navigation failed.
    #[error(transparent)]
    Path(#[from] PathError),
    /// Strongly typed component bridge failed (serialize/deserialize/lookup).
    #[error(transparent)]
    Typed(#[from] TypedComponentError),
}

/// Query every entity carrying the named component.
///
/// Returns an empty list if the component is not registered, or if no entity
/// in the world currently has it.
#[must_use]
pub fn query_with_component(
    world: &mut World,
    registry: &ComponentRegistry,
    component: &str,
) -> Vec<Entity> {
    let Some(component_id) = component_id(registry, component) else {
        return Vec::new();
    };
    let mut query = QueryBuilder::<Entity>::new(world)
        .with_id(component_id)
        .build();
    query.iter(world).collect()
}

/// Read the value at `path` on `entity`'s `component`.
///
/// # Errors
///
/// Returns [`WorldAccessError`] if the entity is missing, the component is
/// unknown or absent, or the path is invalid.
pub fn get(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
) -> Result<Value, WorldAccessError> {
    ensure_alive(world, entity)?;
    match resolve(registry, component)? {
        ComponentKind::Dynamic(dyn_component) => {
            // Fall back to the declared default when the entity has not
            // instantiated this dynamic component yet — matches "declared
            // therefore exists", letting scripts `get` before any `set`.
            let value = crate::component::dynamic::get(world, entity, dyn_component.id)
                .unwrap_or(&dyn_component.default);
            Ok(value.path_get(path)?.clone())
        }
        ComponentKind::Typed(typed) => {
            // Mirror dynamic-component semantics for typed components: when the
            // entity does not carry the component yet, fall back to the
            // `Default` snapshot so a `set` on a fresh entity can still query
            // its current value first.
            let type_registry = registry.type_registry();
            let snapshot = typed
                .read_value(world, entity, type_registry)?
                .or_else(|| typed.default_value(type_registry).ok())
                .ok_or_else(|| WorldAccessError::ComponentNotPresent {
                    entity,
                    component: component.to_owned(),
                })?;
            Ok(snapshot.path_get(path)?.clone())
        }
    }
}

/// Read the **whole** component value on `entity`.
///
/// Returns the dynamic component's stored [`Value`] (cloned) for that layer,
/// or the typed component's serialized snapshot for the other. `None` when
/// the entity does not carry the component.
///
/// # Errors
///
/// Returns [`WorldAccessError`] when the component is unknown or its
/// serialization fails.
pub fn read_component(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
) -> Result<Option<Value>, WorldAccessError> {
    match resolve(registry, component)? {
        ComponentKind::Dynamic(dyn_component) => {
            Ok(crate::component::dynamic::get(world, entity, dyn_component.id).cloned())
        }
        ComponentKind::Typed(typed) => {
            Ok(typed.read_value(world, entity, registry.type_registry())?)
        }
    }
}

/// List the names of every registered component currently attached to `entity`.
#[must_use]
pub fn components_of(world: &World, registry: &ComponentRegistry, entity: Entity) -> Vec<String> {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return Vec::new();
    };
    registry
        .component_names()
        .filter(|name| component_id(registry, name).is_some_and(|id| entity_ref.contains_id(id)))
        .map(str::to_owned)
        .collect()
}

/// Check whether `entity` actually carries the named component.
///
/// 与 [`get`] 的"声明即存在"语义不同：那条路径对未挂的 dynamic / typed
/// 组件会回落到默认值，方便脚本不分阶段地读字段。本函数走"严格 ECS
/// 查询"——脚本想区分"实体真的挂了 X 吗"时用它（典型场景：hover 路径
/// 想区分 tile 与按钮）。
///
/// 未注册的组件名 / 死实体均返回 `false`。
#[must_use]
pub fn has_component(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    name: &str,
) -> bool {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return false;
    };
    let Some(id) = component_id(registry, name) else {
        return false;
    };
    entity_ref.contains_id(id)
}

/// List every entity in `world`.
#[must_use]
pub fn all_entities(world: &mut World) -> Vec<Entity> {
    world.query::<Entity>().iter(world).collect()
}

/// Write `value` at `path` on `entity`'s `component`.
///
/// Both component layers share the same path semantics:
/// - Dynamic component: missing intermediate map keys are auto-created.
/// - Typed component: the current snapshot (or [`Default`]) is read out as
///   a [`Value`], updated at `path`, and deserialized back into the
///   component — preserving the strongly typed invariant on failure
///   (failed writes leave the component untouched).
///
/// # Errors
///
/// Returns [`WorldAccessError`] if the entity is missing, the component is
/// unknown, the path is invalid, or the new value's type does not fit the
/// target field.
pub fn set(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
    value: Value,
) -> Result<(), WorldAccessError> {
    ensure_alive(world, entity)?;
    match resolve(registry, component)? {
        ComponentKind::Dynamic(dyn_component) => {
            let id = dyn_component.id;
            // Auto-insert the dynamic component's default when missing — lets
            // scripts `set` directly on freshly spawned entities.
            if crate::component::dynamic::get(world, entity, id).is_none() {
                crate::component::dynamic::insert(world, entity, id, dyn_component.default.clone());
            }
            let root = crate::component::dynamic::get_mut(world, entity, id).ok_or_else(|| {
                WorldAccessError::ComponentNotPresent {
                    entity,
                    component: component.to_owned(),
                }
            })?;
            root.path_set(path, value)?;
            Ok(())
        }
        ComponentKind::Typed(typed) => {
            typed_path_set(world, entity, registry.type_registry(), typed, path, value)?;
            apply_required(world, registry, entity, component)
        }
    }
}

/// 对 `entity` 上的 typed 组件 `component`，按其 `requires` 列表自动连带尚未挂上
/// 的依赖组件（用各自的 [`Default`]）。
///
/// 与 Bevy 0.18 `#[require(...)]` 等价：写入主组件后，缺什么就补什么。
/// 已挂的组件不会被覆盖。被连带的目标也是 typed 组件——内容层动态组件不参与
/// 此机制。
///
/// # Errors
///
/// Returns [`WorldAccessError::UnknownComponent`] when a `requires(...)` name
/// has no registered typed component (本应在
/// [`crate::component::ComponentRegistry::validate_requires`] 注册期被拦截，
/// 这里作为兜底）。
pub fn apply_required(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
) -> Result<(), WorldAccessError> {
    let Some(ComponentKind::Typed(typed)) = registry.resolve(component) else {
        return Ok(());
    };
    let required: Vec<String> = typed.requires.clone();
    for required_name in &required {
        let required_typed =
            registry
                .typed(required_name)
                .ok_or_else(|| WorldAccessError::UnknownComponent {
                    component: required_name.clone(),
                })?;
        let Ok(entity_ref) = world.get_entity(entity) else {
            return Err(WorldAccessError::EntityNotFound { entity });
        };
        if entity_ref.contains_id(required_typed.id) {
            continue;
        }
        required_typed.insert_default(world, entity, registry.type_registry())?;
        // 递归连带：required 组件本身也可能 requires 其他。
        apply_required(world, registry, entity, required_name)?;
    }
    Ok(())
}

/// Remove the value at `path` on a dynamic component.
///
/// Typed components do not support removal — their shape is fixed at compile
/// time.
///
/// # Errors
///
/// Returns [`WorldAccessError`] if the entity is missing, the component is
/// unknown / absent / typed, or the path is invalid.
pub fn remove(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
) -> Result<(), WorldAccessError> {
    ensure_alive(world, entity)?;
    let ComponentKind::Dynamic(dyn_component) = resolve(registry, component)? else {
        return Err(WorldAccessError::TypedRemoveUnsupported {
            component: component.to_owned(),
        });
    };
    let id = dyn_component.id;
    let root = crate::component::dynamic::get_mut(world, entity, id).ok_or_else(|| {
        WorldAccessError::ComponentNotPresent {
            entity,
            component: component.to_owned(),
        }
    })?;
    root.path_remove(path)?;
    Ok(())
}

/// Whether `entity` is still alive (not despawned).
#[must_use]
pub fn is_alive(world: &World, entity: Entity) -> bool {
    world.get_entity(entity).is_ok()
}

/// Spawn an empty entity and return its id.
#[must_use]
pub fn spawn(world: &mut World) -> Entity {
    world.spawn_empty().id()
}

/// Despawn `entity`, returning whether it existed beforehand.
pub fn despawn(world: &mut World, entity: Entity) -> bool {
    world.despawn(entity)
}

/// Set `child`'s parent to `parent` (Bevy's [`bevy_ecs::hierarchy::ChildOf`]
/// relation). The hooks Bevy installs on the relationship type maintain the
/// reverse [`Children`](bevy_ecs::hierarchy::Children) collection automatically.
///
/// Returns `false` if either entity is dead — the call is a no-op then.
pub fn set_parent(world: &mut World, child: Entity, parent: Entity) -> bool {
    if !is_alive(world, child) || !is_alive(world, parent) {
        return false;
    }
    if let Ok(mut entity_mut) = world.get_entity_mut(child) {
        entity_mut.insert(bevy_ecs::hierarchy::ChildOf(parent));
        true
    } else {
        false
    }
}

/// Remove `child`'s parent relation. Returns `false` if the entity is dead.
/// Removing a missing relation is a no-op (returns `true`).
pub fn clear_parent(world: &mut World, child: Entity) -> bool {
    let Ok(mut entity_mut) = world.get_entity_mut(child) else {
        return false;
    };
    entity_mut.remove::<bevy_ecs::hierarchy::ChildOf>();
    true
}

/// Return `entity`'s parent if any.
#[must_use]
pub fn parent_of(world: &World, entity: Entity) -> Option<Entity> {
    world
        .get_entity(entity)
        .ok()
        .and_then(|e| e.get::<bevy_ecs::hierarchy::ChildOf>())
        .map(bevy_ecs::hierarchy::ChildOf::parent)
}

/// Return `entity`'s direct children (in declaration order).
#[must_use]
pub fn children_of(world: &World, entity: Entity) -> Vec<Entity> {
    use bevy_ecs::relationship::RelationshipTarget;
    world
        .get_entity(entity)
        .ok()
        .and_then(|e| e.get::<bevy_ecs::hierarchy::Children>())
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

/// Insert the named dynamic component's default instance on `entity`.
///
/// # Errors
///
/// Returns [`WorldAccessError::UnknownComponent`] if `component` is not a
/// registered dynamic component.
pub fn insert_dynamic_default(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
) -> Result<(), WorldAccessError> {
    let dyn_component =
        registry
            .dynamic(component)
            .ok_or_else(|| WorldAccessError::UnknownComponent {
                component: component.to_owned(),
            })?;
    let (id, default) = (dyn_component.id, dyn_component.default.clone());
    crate::component::dynamic::insert(world, entity, id, default);
    Ok(())
}

/// Typed-component path write: read the current [`Value`] snapshot → mutate at
/// `path` → deserialize back into the component.
///
/// Stays in sync with dynamic-component semantics: when the entity does not
/// carry the component yet, insert its [`Default`] first so scripts can
/// `set` on a freshly spawned entity.
fn typed_path_set(
    world: &mut World,
    entity: Entity,
    type_registry: &bevy_reflect::TypeRegistry,
    typed: &TypedComponent,
    path: &str,
    value: Value,
) -> Result<(), WorldAccessError> {
    let snapshot = typed.read_value(world, entity, type_registry)?;
    let mut snapshot = match snapshot {
        Some(value) => value,
        None => {
            typed.insert_default(world, entity, type_registry)?;
            typed
                .read_value(world, entity, type_registry)?
                .ok_or(WorldAccessError::TypedReadAfterInsert { entity })?
        }
    };
    // 空 path + Object value：做顶层字段合并而非整体替换。让脚本写
    // `set(e, "Node", "", #{ width: ... })` 时未提及的字段保留默认值，
    // 而不是 reflect 反序列化时报"missing field"。
    //
    // 例外：reflect 把 enum 序列化为"单键 map"`{"Variant": {...}}`，
    // variant 之间互斥；若 default 与 override 是不同 variant，简单字段合并
    // 会产生**两键 map**（reflect 反序列化随即报"expected map with a single key"）。
    // 因此当两边都是单键 map 时整体替换；其它情况按字段合并。
    match (path.is_empty(), &mut snapshot, value) {
        (true, Value::Object(snap_map), Value::Object(override_map)) => {
            if looks_like_reflect_enum(snap_map) || looks_like_reflect_enum(&override_map) {
                *snap_map = override_map;
            } else {
                for (key, v) in override_map {
                    snap_map.insert(key, v);
                }
            }
        }
        (_, _, value) => {
            snapshot.path_set(path, value)?;
        }
    }
    typed.write_value(world, entity, type_registry, snapshot)?;
    Ok(())
}

/// 判断一个 map 是否是 reflect enum 形态：单键，且 key 以大写字母开头。
///
/// reflect 把 enum variant 序列化为 `{"VariantName": {...}}`——variant 名按
/// Rust 规范是 PascalCase（首字母大写）。普通 struct 字段是 snake_case，所以
/// "单键大写"是个稳妥但非完美的判别。
///
/// 这是 `set(e, C, "", #{...})` 路径里"merge vs replace"决策需要的——enum
/// variant 之间互斥不能字段合并，普通 struct 可以。
fn looks_like_reflect_enum(map: &serde_json::Map<String, Value>) -> bool {
    let mut iter = map.iter();
    let Some((key, _)) = iter.next() else {
        return false;
    };
    if iter.next().is_some() {
        return false;
    }
    key.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Resolve the component name to its layer; returns
/// [`WorldAccessError::UnknownComponent`] when unknown.
fn resolve<'a>(
    registry: &'a ComponentRegistry,
    component: &str,
) -> Result<ComponentKind<'a>, WorldAccessError> {
    registry
        .resolve(component)
        .ok_or_else(|| WorldAccessError::UnknownComponent {
            component: component.to_owned(),
        })
}

/// Lookup the [`ComponentId`] for the named component (works for both layers).
fn component_id(
    registry: &ComponentRegistry,
    component: &str,
) -> Option<bevy_ecs::component::ComponentId> {
    match registry.resolve(component)? {
        ComponentKind::Dynamic(dyn_component) => Some(dyn_component.id),
        ComponentKind::Typed(typed) => Some(typed.id),
    }
}

/// Reject dangling entity ids with a clear error rather than panicking.
fn ensure_alive(world: &World, entity: Entity) -> Result<(), WorldAccessError> {
    if is_alive(world, entity) {
        return Ok(());
    }
    Err(WorldAccessError::EntityNotFound { entity })
}

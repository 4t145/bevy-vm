//! World 访问原语：脚本宿主函数与配置加载共用的底层操作。
//!
//! 统一的值访问以**点号路径**（如 `value`、`slots.0.kind`）定位组件内部字段，
//! 按组件所属层分发：
//! - 引擎层类型化组件（如 `Position`）→ 经 [`bevy_reflect`] 的 `reflect_path`；
//! - 内容层动态组件 → 经 [`crate::component::dynamic`] 取出 [`ron::Value`]，再用
//!   本模块的点号导航增删改。
//!
//! 脚本作者两层用同一套路径语法，无需知道底层实现。

mod path;

use crate::component::{ComponentKind, ComponentRegistry};
use bevy_ecs::prelude::*;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::{GetPath, PartialReflect, Reflect, ReflectRef};
use path::ValuePathExt;
use ron::Value;

/// 查询所有挂有指定组件的实体。
///
/// 组件名未登记，或世界中尚无该组件时，返回空列表。
#[must_use]
pub fn query_with_component(
    world: &mut World,
    registry: &ComponentRegistry,
    component: &str,
) -> Vec<Entity> {
    let Some(component_id) = component_id(world, registry, component) else {
        return Vec::new();
    };
    let mut query = QueryBuilder::<Entity>::new(world)
        .with_id(component_id)
        .build();
    query.iter(world).collect()
}

/// 读取实体上某组件给定点号路径处的值。
///
/// # Errors
///
/// 实体已销毁、不挂该组件、组件未登记、路径不存在或类型不符时返回描述性错误。
pub fn get(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
) -> Result<Value, String> {
    ensure_alive(world, entity)?;
    match resolve(registry, component)? {
        ComponentKind::Dynamic(dyn_component) => {
            let value = crate::component::dynamic::get(world, entity, dyn_component.id)
                .ok_or_else(|| missing_component(entity, component))?;
            value.path_get(path).cloned()
        }
        ComponentKind::Typed => typed_get(world, registry, entity, component, path),
    }
}

/// 读取实体上某组件的**完整值**。
///
/// 动态组件返回其存储 [`Value`] 的克隆；类型化组件把其结构体字段反射成一个
/// 映射 [`Value`]。实体不挂该组件时返回 `None`。
///
/// # Errors
///
/// 组件未登记，或反射读取失败时返回描述性错误。
pub fn read_component(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
) -> Result<Option<Value>, String> {
    match resolve(registry, component)? {
        ComponentKind::Dynamic(dyn_component) => {
            Ok(crate::component::dynamic::get(world, entity, dyn_component.id).cloned())
        }
        ComponentKind::Typed => typed_read_component(world, registry, entity, component),
    }
}

/// 列出某实体当前实际挂载的全部已登记组件名。
#[must_use]
pub fn components_of(world: &World, registry: &ComponentRegistry, entity: Entity) -> Vec<String> {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return Vec::new();
    };
    registry
        .component_names()
        .filter(|name| {
            component_id(world, registry, name).is_some_and(|id| entity_ref.contains_id(id))
        })
        .map(str::to_owned)
        .collect()
}

/// 列出世界中的全部实体。
#[must_use]
pub fn all_entities(world: &mut World) -> Vec<Entity> {
    world.query::<Entity>().iter(world).collect()
}

/// 在实体上某组件给定点号路径处写入值。
///
/// 对动态组件，路径不存在时按需创建中间结构；对类型化组件，按反射路径写入。
///
/// # Errors
///
/// 实体已销毁、不挂该组件、组件未登记、路径非法或类型不符时返回描述性错误。
pub fn set(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
    value: Value,
) -> Result<(), String> {
    ensure_alive(world, entity)?;
    match resolve(registry, component)? {
        ComponentKind::Dynamic(dyn_component) => {
            let id = dyn_component.id;
            // 动态组件缺失时按默认值自动插入，使脚本可对新 spawn 的实体直接 set。
            if crate::component::dynamic::get(world, entity, id).is_none() {
                crate::component::dynamic::insert(world, entity, id, dyn_component.default.clone());
            }
            let root = crate::component::dynamic::get_mut(world, entity, id)
                .ok_or_else(|| missing_component(entity, component))?;
            root.path_set(path, value)
        }
        ComponentKind::Typed => typed_set(world, registry, entity, component, path, &value),
    }
}

/// 对实体上某组件给定路径处的数值累加 `delta`，返回累加后的新值。
///
/// # Errors
///
/// 同 [`get`] 与 [`set`]，外加目标不是数值时返回错误。
pub fn add(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
    delta: f64,
) -> Result<f64, String> {
    let current = get(world, registry, entity, component, path)?;
    let number = value_as_f64(&current).ok_or_else(|| format!("路径 `{path}` 处的值不是数值"))?;
    let next = number + delta;
    set(world, registry, entity, component, path, number_value(next))?;
    Ok(next)
}

/// 删除动态组件给定路径处的值。
///
/// 类型化组件不支持删除（其形状编译期固定）。
///
/// # Errors
///
/// 实体已销毁、不挂该组件、组件非动态、路径非法时返回描述性错误。
pub fn remove(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
) -> Result<(), String> {
    ensure_alive(world, entity)?;
    let ComponentKind::Dynamic(dyn_component) = resolve(registry, component)? else {
        return Err(format!("类型化组件 `{component}` 不支持删除字段"));
    };
    let id = dyn_component.id;
    let root = crate::component::dynamic::get_mut(world, entity, id)
        .ok_or_else(|| missing_component(entity, component))?;
    root.path_remove(path)
}

/// 判断实体是否仍存活（未被销毁）。
#[must_use]
pub fn is_alive(world: &World, entity: Entity) -> bool {
    world.get_entity(entity).is_ok()
}

/// 创建一个空实体，返回其 id。
#[must_use]
pub fn spawn(world: &mut World) -> Entity {
    world.spawn_empty().id()
}

/// 销毁一个实体，返回它此前是否存在。
pub fn despawn(world: &mut World, entity: Entity) -> bool {
    world.despawn(entity)
}

/// 把某动态组件的默认值实例插入实体（按名）。
///
/// # Errors
///
/// 组件名不是已注册的动态组件时返回描述性错误。
pub fn insert_dynamic_default(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
) -> Result<(), String> {
    let dyn_component = registry
        .dynamic(component)
        .ok_or_else(|| format!("`{component}` 不是已注册的动态组件"))?;
    let (id, default) = (dyn_component.id, dyn_component.default.clone());
    crate::component::dynamic::insert(world, entity, id, default);
    Ok(())
}

/// 解析组件名到其所属层；未登记时返回描述性错误。
fn resolve<'a>(
    registry: &'a ComponentRegistry,
    component: &str,
) -> Result<ComponentKind<'a>, String> {
    registry
        .resolve(component)
        .ok_or_else(|| format!("组件 `{component}` 未登记"))
}

/// 取组件名对应的 [`ComponentId`]（两层通用）。
fn component_id(
    world: &World,
    registry: &ComponentRegistry,
    component: &str,
) -> Option<bevy_ecs::component::ComponentId> {
    match registry.resolve(component)? {
        ComponentKind::Dynamic(dyn_component) => Some(dyn_component.id),
        ComponentKind::Typed => {
            let registration = registry
                .type_registry()
                .get_with_short_type_path(component)?;
            world.components().get_id(registration.type_id())
        }
    }
}

/// 类型化组件：经反射 `reflect_path` 读取并转成 [`ron::Value`]。
fn typed_get(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
) -> Result<Value, String> {
    let reflect_component = lookup_reflect_component(registry, component)?;
    let entity_ref = world
        .get_entity(entity)
        .map_err(|_| missing_entity(entity))?;
    let reflected = reflect_component
        .reflect(entity_ref)
        .ok_or_else(|| missing_component(entity, component))?;
    let field = reflect_path_value(reflected, path)?;
    Ok(number_value(field))
}

/// 类型化组件：经反射 `reflect_path_mut` 写入一个 `f64`（窄化为 `f32`）。
fn typed_set(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
    path: &str,
    value: &Value,
) -> Result<(), String> {
    let number = value_as_f64(value).ok_or_else(|| "类型化组件字段只接受数值".to_owned())?;
    let reflect_component = lookup_reflect_component(registry, component)?;
    let mut entity_mut = world
        .get_entity_mut(entity)
        .map_err(|_| missing_entity(entity))?;
    let reflected = reflect_component
        .reflect_mut(&mut entity_mut)
        .ok_or_else(|| missing_component(entity, component))?;
    let field = reflected
        .into_inner()
        .reflect_path_mut(path)
        .map_err(|e| format!("反射路径 `{path}` 无效: {e}"))?;
    let applied = number as f32;
    field
        .try_apply(applied.as_partial_reflect())
        .map_err(|e| format!("字段类型不匹配: {e}"))
}

/// 类型化组件：把整个结构体反射成一个映射 [`Value`]（字段名 -> f32 数值）。
fn typed_read_component(
    world: &World,
    registry: &ComponentRegistry,
    entity: Entity,
    component: &str,
) -> Result<Option<Value>, String> {
    let reflect_component = lookup_reflect_component(registry, component)?;
    let Ok(entity_ref) = world.get_entity(entity) else {
        return Ok(None);
    };
    let Some(reflected) = reflect_component.reflect(entity_ref) else {
        return Ok(None);
    };
    let ReflectRef::Struct(structure) = reflected.reflect_ref() else {
        return Err(format!("组件 `{component}` 不是结构体，无法反射为值"));
    };
    let mut map = ron::Map::new();
    for index in 0..structure.field_len() {
        let name = structure
            .name_at(index)
            .ok_or_else(|| format!("组件 `{component}` 第 {index} 个字段无名字"))?;
        let field = structure
            .field_at(index)
            .ok_or_else(|| format!("组件 `{component}` 第 {index} 个字段不可读"))?;
        let number = field
            .try_downcast_ref::<f32>()
            .ok_or_else(|| format!("组件 `{component}` 字段 `{name}` 不是 f32"))?;
        map.insert(
            Value::String(name.to_owned()),
            number_value(f64::from(*number)),
        );
    }
    Ok(Some(Value::Map(map)))
}

/// 读取反射路径处的 `f32` 值并提升为 `f64`。
fn reflect_path_value(reflected: &dyn Reflect, path: &str) -> Result<f64, String> {
    let field = reflected
        .reflect_path(path)
        .map_err(|e| format!("反射路径 `{path}` 无效: {e}"))?;
    field
        .try_downcast_ref::<f32>()
        .map(|v| f64::from(*v))
        .ok_or_else(|| format!("路径 `{path}` 处的值不是 f32"))
}

/// 在反射注册表中查到组件的 [`ReflectComponent`] 反射数据。
fn lookup_reflect_component<'a>(
    registry: &'a ComponentRegistry,
    component: &str,
) -> Result<&'a ReflectComponent, String> {
    let registration = registry
        .type_registry()
        .get_with_short_type_path(component)
        .ok_or_else(|| format!("组件类型 `{component}` 未登记"))?;
    registration
        .data::<ReflectComponent>()
        .ok_or_else(|| format!("组件 `{component}` 缺少 ReflectComponent 反射数据"))
}

/// 实体存活校验：踩到悬空 id 返回明确错误，而非 panic。
fn ensure_alive(world: &World, entity: Entity) -> Result<(), String> {
    if is_alive(world, entity) {
        return Ok(());
    }
    Err(missing_entity(entity))
}

fn missing_entity(entity: Entity) -> String {
    format!("实体 {entity:?} 不存在或已销毁")
}

fn missing_component(entity: Entity, component: &str) -> String {
    format!("实体 {entity:?} 上不存在组件 `{component}`")
}

/// 把 [`ron::Value`] 解释为 `f64`（整数与浮点都接受）。
fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => Some(number.into_f64()),
        _ => None,
    }
}

/// 用一个 `f64` 构造一个 [`ron::Value`] 数值。
fn number_value(value: f64) -> Value {
    Value::Number(ron::value::Number::new(value))
}

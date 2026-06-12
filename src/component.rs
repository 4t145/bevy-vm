//! 值系统的组件层。
//!
//! 组件分两层（见 [`ComponentRegistry`]）：
//! - **引擎层**（类型化）：`Position`、`Velocity` 等少数引擎内核组件，是 `#[derive(Reflect)]`
//!   的 Rust 类型，只被静态热路径 system 访问，享受全速、无反射开销。
//! - **内容层**（动态，见 [`dynamic`]）：游戏/AI 逻辑的组件，运行时按名注册，值为
//!   [`ron::Value`]，可由 AI 在配置里自声明。

pub mod dynamic;

pub use dynamic::DynComponent;

use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_ecs::world::World;
use bevy_reflect::{GetTypeRegistration, Reflect, TypeRegistry};
use ron::Value;
use std::collections::HashMap;

/// 三维位置。AI 友好的简化平移表示，由同步层翻译到渲染侧 `Transform`。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// 三维速度，由静态移动 system 积分到 [`Position`]。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// 某个组件名归属于哪一层。
pub enum ComponentKind<'a> {
    /// 引擎层类型化组件，通过反射注册表访问。
    Typed,
    /// 内容层动态组件，通过其 [`DynComponent`] 访问。
    Dynamic(&'a DynComponent),
}

/// 组件注册表：配置/脚本层的组件名 -> 具体访问方式的桥梁。
///
/// 持有引擎层的反射注册表与内容层的动态组件表。脚本运行时长期持有它（经
/// [`std::rc::Rc`]），据此把组件名分发到正确的访问路径。
pub struct ComponentRegistry {
    type_registry: TypeRegistry,
    typed_inserters: HashMap<String, fn(&mut World, bevy_ecs::entity::Entity)>,
    dynamic: HashMap<String, DynComponent>,
}

impl ComponentRegistry {
    /// 构建一个登记了全部引擎层类型化组件的注册表。
    ///
    /// 内容层动态组件不在此登记，而是在世界构建期按配置声明逐个注册。
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self {
            type_registry: TypeRegistry::new(),
            typed_inserters: HashMap::new(),
            dynamic: HashMap::new(),
        };
        registry.register_typed::<Position>("Position");
        registry.register_typed::<Velocity>("Velocity");
        registry
    }

    /// 登记一个引擎层类型化组件，使其可被配置/脚本按名引用。
    fn register_typed<T>(&mut self, name: &str)
    where
        T: Component + Reflect + GetTypeRegistration + Default,
    {
        self.type_registry.register::<T>();
        self.typed_inserters
            .insert(name.to_owned(), |world, entity| {
                world.entity_mut(entity).insert(T::default());
            });
    }

    /// 向实体插入一个引擎层类型化组件的默认值。
    ///
    /// 返回 `true` 表示成功，`false` 表示该名称不是已登记的类型化组件。
    #[must_use]
    pub fn insert_typed_default(
        &self,
        world: &mut World,
        entity: bevy_ecs::entity::Entity,
        name: &str,
    ) -> bool {
        let Some(inserter) = self.typed_inserters.get(name) else {
            return false;
        };
        inserter(world, entity);
        true
    }

    /// 注册一个内容层动态组件。
    ///
    /// `name` 为对配置/脚本可见的组件名；`default` 为 spawn 时的初始值模板。
    pub fn register_dynamic(&mut self, world: &mut World, name: &str, default: Value) {
        let component = dynamic::register(world, name, default);
        self.dynamic.insert(name.to_owned(), component);
    }

    /// 解析组件名归属的层；未登记时返回 `None`。
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<ComponentKind<'_>> {
        if let Some(component) = self.dynamic.get(name) {
            return Some(ComponentKind::Dynamic(component));
        }
        self.type_registry
            .get_with_short_type_path(name)
            .map(|_| ComponentKind::Typed)
    }

    /// 返回某动态组件的元信息；非动态组件返回 `None`。
    #[must_use]
    pub fn dynamic(&self, name: &str) -> Option<&DynComponent> {
        self.dynamic.get(name)
    }

    /// 遍历全部已登记组件的名字（引擎层 + 内容层）。
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.typed_inserters
            .keys()
            .chain(self.dynamic.keys())
            .map(String::as_str)
    }

    /// 返回引擎层反射注册表，供类型化组件做路径读写。
    #[must_use]
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }
}

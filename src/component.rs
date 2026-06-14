//! 值系统的组件层。
//!
//! 组件分两层（见 [`ComponentRegistry`]）：
//! - **引擎层**（类型化）：`Position`、`Velocity`、`Rotation`、`Scale` 等核心组件，
//!   是 `#[derive(Component, Serialize, Deserialize)]` 的强类型 Rust 类型，由 ECS
//!   按 archetype 存储；脚本/配置层通过 serde 与它们互操作。
//! - **内容层**（动态，见 [`dynamic`]）：游戏/AI 逻辑的组件，运行时按名注册，值为
//!   [`ron::Value`]，可由 AI 在配置里自声明。
//!
//! 两层共享同一套点号路径（[`crate::world_access::path`]）：对 typed 组件而言，
//! 路径访问通过「整段 serde 序列化为 [`Value`]」桥接到 path 模块。

pub mod camera;
pub mod dynamic;
pub mod mesh;
pub mod picking;
pub mod scene;
pub mod sprite;
pub mod text;
pub mod typed;

#[cfg(test)]
mod tests;

pub use dynamic::DynComponent;
pub use typed::TypedComponent;

use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_ecs::world::World;
use bevy_reflect::std_traits::ReflectDefault;
use bevy_reflect::{FromReflect, GetTypeRegistration, Reflect, TypePath, TypeRegistry};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// 批量注册 typed 组件——名字默认取类型简称（最后一段 `::T`）。
///
/// `register_typed!(self, world, [Foo, bar::Baz])` 等价于：
/// `self.register_typed::<Foo>(world, "Foo");`
/// `self.register_typed::<bar::Baz>(world, "Baz");`
///
/// 需要 `.requires(...)` 链式调用的，仍写完整 `register_typed::<T>(...)` 调用。
macro_rules! register_typed {
    ($self:expr, $world:expr, [$($ty:ty),* $(,)?]) => {{
        $(
            $self.register_typed::<$ty>($world, stringify_last_segment!($ty));
        )*
    }};
}

/// 批量注册嵌套字段类型——名字不可见，无需。
///
/// `register_field_types!(self, [Foo, Bar])` 等价于：
/// `self.register_field_type::<Foo>(); self.register_field_type::<Bar>();`
macro_rules! register_field_types {
    ($self:expr, [$($ty:ty),* $(,)?]) => {{
        $(
            $self.register_field_type::<$ty>();
        )*
    }};
}

/// 取类型路径的最后一段——`bar::Baz` → `"Baz"`、`Foo` → `"Foo"`。
///
/// 仅 builder 期一次性调用，运行时间可忽略。
macro_rules! stringify_last_segment {
    ($ty:ty) => {{
        let full: &'static str = stringify!($ty);
        full.rsplit("::").next().unwrap_or(full)
    }};
}

/// Errors raised when registering components with a [`ComponentRegistry`].
#[derive(Debug, Error)]
pub enum RegistryError {
    /// A dynamic component declaration collides with the name of an already
    /// registered typed component — typically an AI-generated config that
    /// re-declares an engine component with a mismatched field set.
    #[error(
        "component name `{name}` is already registered as a typed component and cannot be re-declared as dynamic"
    )]
    DynamicShadowsTyped {
        /// Conflicting component name.
        name: String,
    },
    /// `requires(...)` referenced a component that is not registered as a
    /// typed component — surfaced at registry validation time so typos are
    /// caught before any entity is spawned.
    #[error(
        "typed component `{component}` declares require on `{required}` which is not a registered typed component"
    )]
    UnknownRequired {
        /// The component declaring the requirement.
        component: String,
        /// The name passed to `requires(...)`.
        required: String,
    },
    /// The required-component graph has a cycle — auto-insert would loop.
    #[error("required-component cycle detected: {chain}")]
    RequiresCycle {
        /// `A -> B -> C -> A`-style chain that closes the cycle.
        chain: String,
    },
}

/// 三维位置。AI 友好的简化平移表示，由同步层翻译到渲染侧 `Transform`。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// 三维速度，由静态移动 system 积分到 [`Position`]。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// 欧拉角（度），由同步层翻译为渲染侧 `Transform.rotation`。
///
/// 选择度数而非弧度是为了 AI/脚本端书写直观；同步层负责换算到弧度并构造四元数。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Rotation {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// 三维缩放，由同步层翻译为渲染侧 `Transform.scale`。
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Scale {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Scale {
    fn default() -> Self {
        Self {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        }
    }
}

/// 某个组件名归属于哪一层。
pub enum ComponentKind<'a> {
    /// 引擎层类型化组件，通过其 [`TypedComponent`] 入口 serde 互操作。
    Typed(&'a TypedComponent),
    /// 内容层动态组件，通过其 [`DynComponent`] 访问。
    Dynamic(&'a DynComponent),
}

/// 组件注册表：配置/脚本层的组件名 -> 具体访问方式的桥梁。
///
/// 持有引擎层的强类型组件入口与内容层的动态组件表，**以及**所有 typed
/// 组件类型的 [`TypeRegistry`]——reflect 序列化路径需要在每次访问时拿到
/// 对应类型的 `TypeRegistration`。
///
/// 脚本运行时长期持有它（经 [`std::rc::Rc`]），据此把组件名分发到正确的
/// 访问路径。
pub struct ComponentRegistry {
    typed: HashMap<String, TypedComponent>,
    dynamic: HashMap<String, DynComponent>,
    /// 所有 typed 组件类型在此注册。reflect 序列化/反序列化需要它来查找
    /// `ReflectComponent` / `ReflectDefault` 等 metadata。
    type_registry: TypeRegistry,
}

impl ComponentRegistry {
    /// 构建一个登记了全部引擎层类型化组件的注册表。
    ///
    /// 内容层动态组件不在此登记，而是在世界构建期按配置声明逐个注册。
    ///
    /// # Panics
    ///
    /// Panics if the built-in `requires` declarations are inconsistent —
    /// these are static and any failure means a programming error in this
    /// crate, not in user input.
    #[must_use]
    pub fn with_builtins(world: &mut World) -> Self {
        let mut registry = Self {
            typed: HashMap::new(),
            dynamic: HashMap::new(),
            type_registry: TypeRegistry::new(),
        };
        // 注册顺序：先注册被依赖的组件，再注册声明 require 的组件。
        // 不带 requires 的批量注册：
        register_typed!(
            registry,
            world,
            [Position, Rotation, Scale, picking::Pickable]
        );
        // 带 requires 的逐一注册（链式声明保留可读性）：
        registry
            .register_typed::<Velocity>(world, "Velocity")
            .requires("Position");
        registry
            .register_typed::<camera::Camera3d>(world, "Camera3d")
            .requires("Position");
        registry
            .register_typed::<camera::Camera2d>(world, "Camera2d")
            .requires("Position");
        registry
            .register_typed::<text::TextLabel>(world, "TextLabel")
            .requires("Position");
        registry
            .register_typed::<sprite::Sprite2d>(world, "Sprite2d")
            .requires("Position");
        registry
            .register_typed::<mesh::Mesh3dRender>(world, "Mesh3dRender")
            .requires("Position");
        registry
            .register_typed::<scene::SceneRender>(world, "SceneRender")
            .requires("Position");

        // 嵌套字段类型——reflect 反序列化需要每个可达类型都在 registry。
        register_field_types!(
            registry,
            [
                camera::CameraProjection,
                camera::OrthoScalingMode,
                crate::resource::image::ImageBuilder,
                crate::resource::mesh::MeshBuilder,
                crate::resource::material::MaterialBuilder,
                crate::resource::material::PbrMaterial,
                bevy_color::Color,
                bevy_color::Srgba,
                bevy_color::LinearRgba,
                bevy_color::Hsla,
                bevy_color::Hsva,
                bevy_color::Hwba,
                bevy_color::Laba,
                bevy_color::Lcha,
                bevy_color::Oklaba,
                bevy_color::Oklcha,
                bevy_color::Xyza,
            ]
        );

        #[cfg(feature = "bevy-bridge")]
        registry.register_ui_types(world);

        registry
            .validate_requires()
            .expect("内置 requires 关系应当自洽");
        registry
    }

    /// 注册 Bevy UI 相关 typed 组件 + 它们引用的所有字段类型。
    ///
    /// 这些类型直接复用 Bevy 0.18 的 `bevy::ui::*`——不再写 VM 自家的镜像，
    /// reflect 序列化路径让脚本以原本字段集操作它们。
    ///
    /// 仅在 `bevy-bridge` feature 下可用——UI 渲染依赖整套 Bevy 主世界，
    /// headless 路径无 UI 概念。
    #[cfg(feature = "bevy-bridge")]
    fn register_ui_types(&mut self, world: &mut World) {
        use bevy::prelude::*;
        use bevy::text::{FontSmoothing, Justify, LineBreak, LineHeight, TextBounds, TextEntity};
        use bevy::ui::widget::{Button, Text, TextNodeFlags};
        use bevy::ui::{
            AlignContent, AlignItems, AlignSelf, BoxSizing, Display, FlexDirection, FlexWrap,
            GridAutoFlow, GridPlacement, GridTrack, JustifyContent, JustifyItems, JustifySelf,
            MaxTrackSizingFunction, MinTrackSizingFunction, Overflow, OverflowAxis,
            OverflowClipBox, OverflowClipMargin, PositionType, RepeatedGridTrack, UiRect, Val,
        };

        // ---- 顶层 typed 组件 ----------------------------------------------
        register_typed!(
            self,
            world,
            [
                Node,
                BackgroundColor,
                BorderColor,
                Outline,
                ZIndex,
                Button,
                Text,
                TextFont,
                TextColor,
                TextLayout,
                TextNodeFlags,
            ]
        );

        // ---- Node 引用的字段类型（值 / enum） ------------------------------
        // reflect 反序列化嵌套字段时要查它们的 registration。
        register_field_types!(
            self,
            [
                Val,
                UiRect,
                Display,
                BoxSizing,
                PositionType,
                Overflow,
                OverflowAxis,
                OverflowClipBox,
                OverflowClipMargin,
                AlignItems,
                JustifyItems,
                AlignSelf,
                JustifySelf,
                AlignContent,
                JustifyContent,
                FlexDirection,
                FlexWrap,
                GridAutoFlow,
                GridTrack,
                RepeatedGridTrack,
                GridPlacement,
                MinTrackSizingFunction,
                MaxTrackSizingFunction,
                // Text* 字段
                Justify,
                LineBreak,
                LineHeight,
                FontSmoothing,
                TextBounds,
                TextEntity,
            ]
        );
    }

    /// 登记一个引擎层类型化组件，使其可被配置/脚本按名引用。
    ///
    /// 返回一个 [`TypedComponentBuilder`]，可以继续 `.requires("Other")`
    /// 声明该组件被设置时缺省连带的其他类型化组件。
    ///
    /// 同时把 `T` 注册到内部 [`TypeRegistry`]——reflect 序列化路径需要。
    fn register_typed<T>(&mut self, world: &mut World, name: &str) -> TypedComponentBuilder<'_>
    where
        T: Component + Reflect + FromReflect + GetTypeRegistration + TypePath,
    {
        self.type_registry.register::<T>();
        let typed = TypedComponent::new::<T>(world);
        self.typed.insert(name.to_owned(), typed);
        TypedComponentBuilder {
            registry: self,
            name: name.to_owned(),
        }
    }

    /// Register an additional type used as a field by typed components but
    /// not itself a component (e.g. enums like `OrthoScalingMode`,
    /// builders like `MeshBuilder`). `reflect` deserialization needs every
    /// concrete type encountered to be in the registry.
    pub fn register_field_type<T>(&mut self)
    where
        T: Reflect + FromReflect + GetTypeRegistration + TypePath,
    {
        self.type_registry.register::<T>();
    }

    /// Borrow the underlying [`TypeRegistry`] — used by [`crate::world_access`]
    /// when invoking the reflect-driven [`TypedComponent`] vtable.
    #[must_use]
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }

    /// 校验所有 typed 组件的 `requires` 声明：目标必须存在、不存在环。
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::UnknownRequired`] when a `requires(...)` name
    /// has no registered typed component, or [`RegistryError::RequiresCycle`]
    /// when the dependency graph contains a cycle.
    pub fn validate_requires(&self) -> Result<(), RegistryError> {
        for (name, typed) in &self.typed {
            for required in &typed.requires {
                if !self.typed.contains_key(required) {
                    return Err(RegistryError::UnknownRequired {
                        component: name.clone(),
                        required: required.clone(),
                    });
                }
            }
        }
        // DFS 检环；white/gray/black 三色标记。
        enum Color {
            Gray,
            Black,
        }
        let mut color: HashMap<&str, Color> = HashMap::new();
        for start in self.typed.keys() {
            if color.contains_key(start.as_str()) {
                continue;
            }
            let mut stack: Vec<(&str, usize)> = vec![(start.as_str(), 0)];
            color.insert(start.as_str(), Color::Gray);
            while let Some(&(node, idx)) = stack.last() {
                let requires = &self.typed[node].requires;
                if idx >= requires.len() {
                    color.insert(node, Color::Black);
                    stack.pop();
                    continue;
                }
                stack.last_mut().expect("栈非空：上一步刚拿到 last").1 += 1;
                let next = requires[idx].as_str();
                match color.get(next) {
                    Some(Color::Gray) => {
                        let mut chain: Vec<&str> = stack.iter().map(|&(name, _)| name).collect();
                        chain.push(next);
                        return Err(RegistryError::RequiresCycle {
                            chain: chain.join(" -> "),
                        });
                    }
                    Some(Color::Black) => continue,
                    None => {
                        color.insert(next, Color::Gray);
                        stack.push((next, 0));
                    }
                }
            }
        }
        Ok(())
    }

    /// 注册一个内容层动态组件。
    ///
    /// `name` 为对配置/脚本可见的组件名；`default` 为 spawn 时的初始值模板。
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::DynamicShadowsTyped`] when the name collides
    /// with a typed component — surfacing AI-generated config mistakes at
    /// load time rather than letting them silently mismatch.
    pub fn register_dynamic(
        &mut self,
        world: &mut World,
        name: &str,
        default: Value,
    ) -> Result<(), RegistryError> {
        if self.typed.contains_key(name) {
            return Err(RegistryError::DynamicShadowsTyped {
                name: name.to_owned(),
            });
        }
        let component = dynamic::register(world, name, default);
        self.dynamic.insert(name.to_owned(), component);
        Ok(())
    }

    /// 解析组件名归属的层；未登记时返回 `None`。
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<ComponentKind<'_>> {
        if let Some(component) = self.dynamic.get(name) {
            return Some(ComponentKind::Dynamic(component));
        }
        self.typed.get(name).map(ComponentKind::Typed)
    }

    /// 返回某动态组件的元信息；非动态组件返回 `None`。
    #[must_use]
    pub fn dynamic(&self, name: &str) -> Option<&DynComponent> {
        self.dynamic.get(name)
    }

    /// 返回某类型化组件的元信息；非类型化组件返回 `None`。
    #[must_use]
    pub fn typed(&self, name: &str) -> Option<&TypedComponent> {
        self.typed.get(name)
    }

    /// 遍历全部已登记组件的名字（引擎层 + 内容层）。
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.typed
            .keys()
            .chain(self.dynamic.keys())
            .map(String::as_str)
    }
}

/// 链式声明 typed 组件的 `requires` 关系。
///
/// 由 [`ComponentRegistry::register_typed`] 返回；drop 时无副作用，
/// 因此调用方可以无视返回值。
pub struct TypedComponentBuilder<'a> {
    registry: &'a mut ComponentRegistry,
    name: String,
}

impl<'a> TypedComponentBuilder<'a> {
    /// 声明该组件被设置时，若实体未挂 `required`，自动以其 `Default` 挂上。
    ///
    /// `required` 必须是已注册的 typed 组件名；非法名会在
    /// [`ComponentRegistry::validate_requires`] 校验阶段被拒绝。
    pub fn requires(self, required: &str) -> Self {
        if let Some(typed) = self.registry.typed.get_mut(&self.name)
            && !typed.requires.iter().any(|name| name == required)
        {
            typed.requires.push(required.to_owned());
        }
        self
    }
}

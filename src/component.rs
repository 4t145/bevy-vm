//! 值系统的组件层。
//!
//! 单 World 架构下，VM 不再有自家的视觉 typed 组件（Mesh3dRender / Sprite2d
//! / Camera3d / TextLabel / SceneRender / Position / Rotation / Scale 等都已
//! 删除）。脚本想挂渲染相关组件时，调专用 host fn（`attach_mesh` / `attach_pbr`
//! / `attach_sprite` / `attach_camera` 等），它们直接在主 World 上 build
//! Bevy `Handle<...>` + insert 原生组件。
//!
//! 仍然存在的两层：
//! - **引擎层 typed 组件**：`Pickable`（标记 picking 启用）+ Bevy UI 原生类型
//!   (`Node`, `BackgroundColor`, ... )。脚本通过 `set(e, "Node", "", #{...})`
//!   走 reflect 反序列化照常使用。
//! - **内容层动态组件**（[`dynamic`]）：游戏逻辑层，按名注册，存为 [`ron::Value`]，
//!   由 AI 在 ron / json 里自由声明。

pub mod dynamic;
pub mod picking;
pub mod typed;

#[cfg(test)]
mod tests;

pub use dynamic::DynComponent;
pub use typed::TypedComponent;

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use bevy_reflect::{FromReflect, GetTypeRegistration, Reflect, TypePath, TypeRegistry};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Batch-register typed components by Rust type name (last `::` segment).
macro_rules! register_typed {
    ($self:expr, $world:expr, [$($ty:ty),* $(,)?]) => {{
        $(
            let _ = $self.register_typed::<$ty>($world, stringify_last_segment!($ty));
        )*
    }};
}

/// Batch-register field types — names are not surface-visible.
macro_rules! register_field_types {
    ($self:expr, [$($ty:ty),* $(,)?]) => {{
        $(
            $self.register_field_type::<$ty>();
        )*
    }};
}

macro_rules! stringify_last_segment {
    ($ty:ty) => {{
        let full: &'static str = stringify!($ty);
        full.rsplit("::").next().unwrap_or(full)
    }};
}

/// Errors raised when registering components with a [`ComponentRegistry`].
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Dynamic name collides with a typed registration.
    #[error(
        "component name `{name}` is already registered as a typed component and cannot be re-declared as dynamic"
    )]
    DynamicShadowsTyped {
        /// Conflicting component name.
        name: String,
    },
    /// `requires(...)` referenced an unknown typed component.
    #[error(
        "typed component `{component}` declares require on `{required}` which is not a registered typed component"
    )]
    UnknownRequired {
        /// The component declaring the requirement.
        component: String,
        /// The name passed to `requires(...)`.
        required: String,
    },
    /// `requires(...)` graph contains a cycle.
    #[error("required-component cycle detected: {chain}")]
    RequiresCycle {
        /// `A -> B -> C -> A`-style chain that closes the cycle.
        chain: String,
    },
}

/// 某个组件名归属于哪一层。
pub enum ComponentKind<'a> {
    /// 引擎层类型化组件，通过其 [`TypedComponent`] 入口 serde 互操作。
    Typed(&'a TypedComponent),
    /// 内容层动态组件，通过其 [`DynComponent`] 访问。
    Dynamic(&'a DynComponent),
}

/// Component registry: maps name → typed/dynamic access for both layers.
pub struct ComponentRegistry {
    pub(crate) typed: HashMap<String, TypedComponent>,
    pub(crate) dynamic: HashMap<String, DynComponent>,
    type_registry: TypeRegistry,
}

impl ComponentRegistry {
    /// Builder used by [`crate::VmInstance`] to construct the per-instance
    /// registry. Pre-loads every Bevy native type the VM may want to expose
    /// to scripts — currently just [`picking::Pickable`] and the UI types
    /// under `bevy::ui::*`.
    ///
    /// Idempotent on a shared world: Bevy's `register_component<T>` returns
    /// the same `ComponentId` on repeat, so multiple `VmInstance`s constructed
    /// against the same world don't conflict.
    #[must_use]
    pub fn with_builtins(world: &mut World) -> Self {
        let mut registry = Self {
            typed: HashMap::new(),
            dynamic: HashMap::new(),
            type_registry: TypeRegistry::new(),
        };
        register_typed!(registry, world, [picking::Pickable]);
        registry.register_field_type::<bevy_color::Color>();
        register_field_types!(
            registry,
            [
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
            .expect("built-in `requires` graph must be self-consistent");
        registry
    }

    /// Register Bevy UI typed components — the VM exposes them by name to
    /// scripts so dynamically constructed UI panels can walk through `set`.
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
                Justify,
                LineBreak,
                LineHeight,
                FontSmoothing,
                TextBounds,
                TextEntity,
            ]
        );
    }

    /// Register a typed component and add it to this registry's name table.
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

    /// Register an additional reflect type used as a field by typed components.
    pub fn register_field_type<T>(&mut self)
    where
        T: Reflect + FromReflect + GetTypeRegistration + TypePath,
    {
        self.type_registry.register::<T>();
    }

    /// Borrow the underlying [`TypeRegistry`].
    #[must_use]
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }

    /// Validate that every `requires(...)` declaration references a real
    /// typed component and the dependency graph is acyclic.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::UnknownRequired`] for dangling targets,
    /// [`RegistryError::RequiresCycle`] for cycles.
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
                stack.last_mut().expect("non-empty").1 = idx + 1;
                let next = requires[idx].as_str();
                match color.get(next) {
                    Some(Color::Gray) => {
                        let chain: Vec<&str> = stack
                            .iter()
                            .map(|(n, _)| *n)
                            .chain(std::iter::once(next))
                            .collect();
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

    /// Register a dynamic (script-declared) component.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::DynamicShadowsTyped`] when `name` already
    /// names a typed component.
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

    /// Resolve a name to its layer.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<ComponentKind<'_>> {
        if let Some(typed) = self.typed.get(name) {
            return Some(ComponentKind::Typed(typed));
        }
        if let Some(dyn_component) = self.dynamic.get(name) {
            return Some(ComponentKind::Dynamic(dyn_component));
        }
        None
    }

    /// Look up the typed component descriptor for `name`.
    #[must_use]
    pub fn typed(&self, name: &str) -> Option<&TypedComponent> {
        self.typed.get(name)
    }

    /// Look up the dynamic component descriptor for `name`.
    #[must_use]
    pub fn dynamic(&self, name: &str) -> Option<&DynComponent> {
        self.dynamic.get(name)
    }

    /// Iterate every registered component name (typed + dynamic).
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.typed
            .keys()
            .chain(self.dynamic.keys())
            .map(String::as_str)
    }
}

/// Builder for the post-`register_typed` continuation: chain `.requires(...)`
/// to declare typed-component requirements (auto-inserted on init).
pub struct TypedComponentBuilder<'a> {
    registry: &'a mut ComponentRegistry,
    name: String,
}

impl TypedComponentBuilder<'_> {
    /// Declare that initialising the component this builder represents also
    /// implicitly inserts `target` if absent.
    pub fn requires(&mut self, target: &str) -> &mut Self {
        if let Some(typed) = self.registry.typed.get_mut(&self.name) {
            typed.requires.push(target.to_owned());
        }
        self
    }
}

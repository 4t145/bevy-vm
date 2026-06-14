//! Typed-component bridge: exposes strongly typed Rust components through a
//! [`Value`] view via [`bevy_reflect`].
//!
//! ECS stores these components as their real Rust types; configuration and
//! script layers want to access them with the same dotted-path + JSON-value
//! protocol used for dynamic components. This module bridges the two sides
//! through a small set of **type-erased function pointers** powered by
//! reflection rather than direct `serde` derives. The benefit: any type
//! deriving `Reflect` can be a typed component, including Bevy's built-in
//! components which mostly derive `Reflect` but not `Serialize`.
//!
//! The vtable mirrors the four operations used by [`crate::world_access`]:
//!
//! - [`TypedComponent::insert_default`] — insert the component's default
//!   instance (resolved via [`bevy_reflect::std_traits::ReflectDefault`]).
//! - [`TypedComponent::insert_from_value`] — deserialize a [`Value`] into a
//!   component instance and insert it.
//! - [`TypedComponent::read_value`] — read the component on an entity and
//!   serialize it back to a [`Value`].
//! - [`TypedComponent::write_value`] — overwrite the existing component
//!   instance on an entity with the deserialization of a whole [`Value`].
//!
//! Path-based get/set is composed by [`crate::world_access`] from these:
//! "read whole [`Value`] snapshot → manipulate via [`crate::world_access::path`]
//!  → write back when needed", giving typed components the same dotted-path
//! semantics as dynamic components.
//!
//! # Enum shape note
//!
//! `bevy_reflect` serializes enums in **externally tagged** form — a `Foo::Bar
//! { x: 1 }` round-trips as `{"Bar": {"x": 1}}`, and a unit variant `Foo::Baz`
//! as the bare string `"Baz"`. This differs from `serde`'s default and from
//! the `#[serde(tag = "kind")]` form previously used here; configuration files
//! must follow the reflect form.

use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::entity::Entity;
use bevy_ecs::reflect::ReflectComponent;
use bevy_ecs::world::World;
use bevy_reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use bevy_reflect::std_traits::ReflectDefault;
use bevy_reflect::{
    FromReflect, GetTypeRegistration, PartialReflect, Reflect, TypePath, TypeRegistry,
};
use serde::de::DeserializeSeed;
use serde_json::Value;
use std::any::TypeId;
use thiserror::Error;

/// Errors raised by [`TypedComponent`] operations.
#[derive(Debug, Error)]
pub enum TypedComponentError {
    /// The target entity has been despawned or never existed.
    #[error("entity {entity:?} does not exist")]
    EntityNotFound {
        /// Entity that triggered the lookup.
        entity: Entity,
    },
    /// `write_value` was called on an entity that does not carry the component.
    #[error("entity {entity:?} does not carry this typed component")]
    ComponentNotPresent {
        /// Entity missing the component.
        entity: Entity,
    },
    /// Serializing the strongly typed component failed (rare, only for unusual
    /// field types).
    #[error("failed to serialize component via reflect: {0}")]
    Serialize(String),
    /// Deserializing a [`Value`] into the strongly typed component failed.
    #[error("failed to deserialize value into target component via reflect: {0}")]
    Deserialize(String),
    /// Component type is not registered with the
    /// [`TypeRegistry`] passed in — a programmer error.
    #[error("component type id {type_id:?} is not registered with TypeRegistry")]
    TypeNotRegistered {
        /// Rust [`TypeId`] of the missing type.
        type_id: TypeId,
    },
    /// `ReflectDefault` registration is missing — the component did not
    /// `#[reflect(Default)]`.
    #[error("component is missing ReflectDefault metadata")]
    NoReflectDefault,
    /// `ReflectComponent` registration is missing — the component did not
    /// `#[reflect(Component)]`.
    #[error("component is missing ReflectComponent metadata")]
    NoReflectComponent,
}

/// Vtable of type-erased operations on a typed component, all backed by
/// `bevy_reflect`. Each closure takes the live [`TypeRegistry`] so it can
/// resolve the component's [`TypeRegistration`](bevy_reflect::TypeRegistration)
/// without storing a stale reference.
type InsertDefaultFn = fn(&mut World, Entity, &TypeRegistry) -> Result<(), TypedComponentError>;
type InsertFromValueFn =
    fn(&mut World, Entity, &TypeRegistry, Value) -> Result<(), TypedComponentError>;
type ReadValueFn = fn(&World, Entity, &TypeRegistry) -> Result<Option<Value>, TypedComponentError>;
type WriteValueFn = fn(&mut World, Entity, &TypeRegistry, Value) -> Result<(), TypedComponentError>;
type DefaultValueFn = fn(&TypeRegistry) -> Result<Value, TypedComponentError>;

/// Metadata of a registered typed component: ECS [`ComponentId`] + a vtable of
/// reflect-driven access functions, plus the Rust [`TypeId`] for fast
/// `TypeRegistry` lookup at the vtable boundary.
pub struct TypedComponent {
    /// ECS component id, used by `query` and id-based access.
    pub id: ComponentId,
    /// Rust type id, used to look up the registration in [`TypeRegistry`].
    pub type_id: TypeId,
    /// Names of other registered components that should be auto-inserted
    /// (with their `Default`) whenever this component is set on an entity
    /// that does not yet carry them. Mirrors Bevy 0.18 `#[require(...)]`
    /// semantics on the VM side.
    pub requires: Vec<String>,
    insert_default: InsertDefaultFn,
    insert_from_value: InsertFromValueFn,
    read_value: ReadValueFn,
    write_value: WriteValueFn,
    default_value: DefaultValueFn,
}

impl TypedComponent {
    /// Build a [`TypedComponent`] for type `T` and register it with `world`.
    ///
    /// `T` must derive `Reflect` and carry the `#[reflect(Component, Default)]`
    /// attributes — those produce the [`ReflectComponent`] /
    /// [`ReflectDefault`] entries that this vtable depends on. The caller is
    /// responsible for `type_registry.register::<T>()` (typically done by
    /// [`crate::component::ComponentRegistry::register_typed`]).
    #[must_use]
    pub fn new<T>(world: &mut World) -> Self
    where
        T: Component + Reflect + FromReflect + GetTypeRegistration + TypePath,
    {
        let id = world.register_component::<T>();
        Self {
            id,
            type_id: TypeId::of::<T>(),
            requires: Vec::new(),
            insert_default: insert_default::<T>,
            insert_from_value: insert_from_value::<T>,
            read_value: read_value::<T>,
            write_value: write_value::<T>,
            default_value: default_value::<T>,
        }
    }

    /// Insert the component's [`Default`] instance on `entity`.
    ///
    /// # Errors
    ///
    /// Returns [`TypedComponentError`] when the type is not registered or
    /// lacks `#[reflect(Default)]` / `#[reflect(Component)]`.
    pub fn insert_default(
        &self,
        world: &mut World,
        entity: Entity,
        registry: &TypeRegistry,
    ) -> Result<(), TypedComponentError> {
        (self.insert_default)(world, entity, registry)
    }

    /// Deserialize `value` into a component instance and insert it on `entity`
    /// (overwriting any existing instance of the same type).
    ///
    /// When `value` is a map, missing fields are filled in from the component's
    /// [`Default`] instance via [`Self::merge_with_default`] — letting config
    /// authors override only the fields they care about.
    ///
    /// # Errors
    ///
    /// Returns [`TypedComponentError`] if the entity is missing, the
    /// component lacks the required reflect metadata, or `value` cannot be
    /// deserialized into the component type.
    pub fn insert_from_value(
        &self,
        world: &mut World,
        entity: Entity,
        registry: &TypeRegistry,
        value: Value,
    ) -> Result<(), TypedComponentError> {
        let merged = self.merge_with_default(registry, value)?;
        (self.insert_from_value)(world, entity, registry, merged)
    }

    /// Read the component on `entity` and serialize it to a [`Value`].
    ///
    /// Returns `Ok(None)` if the entity does not exist or does not carry the
    /// component.
    ///
    /// # Errors
    ///
    /// Returns [`TypedComponentError::Serialize`] when the strongly typed
    /// component fails to serialize.
    pub fn read_value(
        &self,
        world: &World,
        entity: Entity,
        registry: &TypeRegistry,
    ) -> Result<Option<Value>, TypedComponentError> {
        (self.read_value)(world, entity, registry)
    }

    /// Deserialize `value` and overwrite the existing component instance on
    /// `entity`.
    ///
    /// # Errors
    ///
    /// Returns [`TypedComponentError`] if the entity is missing, does not
    /// carry the component, or `value` cannot be deserialized.
    pub fn write_value(
        &self,
        world: &mut World,
        entity: Entity,
        registry: &TypeRegistry,
        value: Value,
    ) -> Result<(), TypedComponentError> {
        (self.write_value)(world, entity, registry, value)
    }

    /// Top-level field-merge `overrides` into the component's default
    /// instance.
    ///
    /// Merging only happens when both values are maps; otherwise `overrides`
    /// is returned unchanged. This lets config authors specify only the fields
    /// they care about (matching the old "spawn-then-set-each-field" semantics).
    fn merge_with_default(
        &self,
        registry: &TypeRegistry,
        overrides: Value,
    ) -> Result<Value, TypedComponentError> {
        let Value::Object(override_map) = overrides else {
            return Ok(overrides);
        };
        let Value::Object(mut merged) = self.default_value(registry)? else {
            return Ok(Value::Object(override_map));
        };
        for (key, value) in override_map {
            merged.insert(key, value);
        }
        Ok(Value::Object(merged))
    }

    /// [`Value`] of the component's default instance.
    ///
    /// Used as the fallback snapshot when reading a component from an entity
    /// that does not carry it — keeping parity with the dynamic-component
    /// "declared therefore exists" semantics.
    ///
    /// # Errors
    ///
    /// Returns [`TypedComponentError`] on serialization failure or when the
    /// component lacks `#[reflect(Default)]`.
    pub fn default_value(&self, registry: &TypeRegistry) -> Result<Value, TypedComponentError> {
        (self.default_value)(registry)
    }
}

// ---- vtable bodies -------------------------------------------------------

fn registration(
    registry: &TypeRegistry,
    type_id: TypeId,
) -> Result<&bevy_reflect::TypeRegistration, TypedComponentError> {
    registry
        .get(type_id)
        .ok_or(TypedComponentError::TypeNotRegistered { type_id })
}

fn reflect_default<T: 'static>(
    registry: &TypeRegistry,
) -> Result<Box<dyn Reflect>, TypedComponentError> {
    let registration = registration(registry, TypeId::of::<T>())?;
    let default = registration
        .data::<ReflectDefault>()
        .ok_or(TypedComponentError::NoReflectDefault)?;
    Ok(default.default())
}

fn reflect_component_for<T: 'static>(
    registry: &TypeRegistry,
) -> Result<&ReflectComponent, TypedComponentError> {
    let registration = registration(registry, TypeId::of::<T>())?;
    registration
        .data::<ReflectComponent>()
        .ok_or(TypedComponentError::NoReflectComponent)
}

fn insert_default<T>(
    world: &mut World,
    entity: Entity,
    registry: &TypeRegistry,
) -> Result<(), TypedComponentError>
where
    T: Component + Reflect + FromReflect + GetTypeRegistration + TypePath,
{
    let default = reflect_default::<T>(registry)?;
    let reflect_component = reflect_component_for::<T>(registry)?;
    let mut entity_mut = world
        .get_entity_mut(entity)
        .map_err(|_| TypedComponentError::EntityNotFound { entity })?;
    reflect_component.insert(&mut entity_mut, default.as_partial_reflect(), registry);
    Ok(())
}

fn deserialize_value<T: 'static>(
    registry: &TypeRegistry,
    value: Value,
) -> Result<Box<dyn PartialReflect>, TypedComponentError> {
    let registration = registration(registry, TypeId::of::<T>())?;
    let deserializer = TypedReflectDeserializer::new(registration, registry);
    deserializer
        .deserialize(value)
        .map_err(|e| TypedComponentError::Deserialize(e.to_string()))
}

fn insert_from_value<T>(
    world: &mut World,
    entity: Entity,
    registry: &TypeRegistry,
    value: Value,
) -> Result<(), TypedComponentError>
where
    T: Component + Reflect + FromReflect + GetTypeRegistration + TypePath,
{
    let boxed = deserialize_value::<T>(registry, value)?;
    let reflect_component = reflect_component_for::<T>(registry)?;
    let mut entity_mut = world
        .get_entity_mut(entity)
        .map_err(|_| TypedComponentError::EntityNotFound { entity })?;
    reflect_component.insert(&mut entity_mut, boxed.as_ref(), registry);
    Ok(())
}

fn read_value<T>(
    world: &World,
    entity: Entity,
    registry: &TypeRegistry,
) -> Result<Option<Value>, TypedComponentError>
where
    T: Component + Reflect,
{
    let Ok(entity_ref) = world.get_entity(entity) else {
        return Ok(None);
    };
    let Some(component) = entity_ref.get::<T>() else {
        return Ok(None);
    };
    serialize_reflect(component, registry).map(Some)
}

fn default_value<T>(registry: &TypeRegistry) -> Result<Value, TypedComponentError>
where
    T: 'static + Reflect,
{
    let default = reflect_default::<T>(registry)?;
    serialize_reflect(default.as_ref(), registry)
}

fn write_value<T>(
    world: &mut World,
    entity: Entity,
    registry: &TypeRegistry,
    value: Value,
) -> Result<(), TypedComponentError>
where
    T: Component + Reflect + FromReflect + GetTypeRegistration + TypePath,
{
    let mut entity_mut = world
        .get_entity_mut(entity)
        .map_err(|_| TypedComponentError::EntityNotFound { entity })?;
    if entity_mut.get::<T>().is_none() {
        return Err(TypedComponentError::ComponentNotPresent { entity });
    }
    let boxed = deserialize_value::<T>(registry, value)?;
    let reflect_component = reflect_component_for::<T>(registry)?;
    reflect_component.insert(&mut entity_mut, boxed.as_ref(), registry);
    Ok(())
}

/// Serialize a `dyn Reflect` value into a [`Value`] without the type-path
/// wrapper that [`bevy_reflect::serde::ReflectSerializer`] adds.
fn serialize_reflect<R: Reflect + ?Sized>(
    value: &R,
    registry: &TypeRegistry,
) -> Result<Value, TypedComponentError> {
    let serializer = TypedReflectSerializer::new(value.as_partial_reflect(), registry);
    serde_json::to_value(serializer).map_err(|e| TypedComponentError::Serialize(e.to_string()))
}

//! R1 spike：评估把 typed 组件桥从 serde 派生切到 `bevy_reflect` 序列化的代价。
//!
//! 我们不动现有 `TypedComponent`——只在测试里直接用 `bevy_reflect` 的
//! `ReflectSerializer` / `ReflectDeserializer` 跑一遍 round-trip，验证：
//!
//! 1. 普通 struct（`Position`、`Sprite2d` 这种）是否能 round-trip 干净
//! 2. **enum 形态差异**——`OrthoScalingMode` 等当前用 `#[serde(tag = "kind")]`
//!    internally tagged，reflect 用自己的固定形态；输出的 JSON 可能和现有
//!    配置 RON 不兼容
//! 3. `Default` 的获取路径——能否从 `TypeRegistry` 直接拿到默认值
//! 4. `Option<T>`、`[f32; 4]` 这类常用包装的处理是否一致
//!
//! 这些发现会决定 R1 是否值得全面切，以及切的时候会产生多少配置迁移。

use bevy_ecs::prelude::*;
use bevy_reflect::serde::{ReflectSerializer, TypedReflectDeserializer, TypedReflectSerializer};
use bevy_reflect::std_traits::ReflectDefault;
use bevy_reflect::{FromReflect, GetTypeRegistration, PartialReflect, Reflect, TypeRegistry};
use serde::de::DeserializeSeed;
use serde_json::Value;

// ---- 候选组件：reflect 派生版本（不影响 production 代码） ---------------

#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default)]
struct ReflectPosition {
    x: f32,
    y: f32,
    z: f32,
}

/// 现状：`#[serde(tag = "kind", rename_all = "snake_case")]` —— internally tagged。
/// 这里改成 reflect 派生，看默认形态是什么。
#[derive(Reflect, Debug, Clone, PartialEq, Default)]
#[reflect(Default)]
enum ReflectScalingMode {
    #[default]
    WindowSize,
    Fixed {
        width: f32,
        height: f32,
    },
    FixedVertical {
        viewport_height: f32,
    },
}

#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default)]
struct ReflectCamera2d {
    scaling_mode: ReflectScalingMode,
    scale: f32,
}

#[derive(Reflect, Debug, Clone, PartialEq, Default)]
#[reflect(Default)]
enum ReflectImageBuilder {
    #[default]
    None,
    AssetPath {
        path: String,
    },
}

#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default)]
struct ReflectSprite2d {
    image: Option<ReflectImageBuilder>,
    color: [f32; 4],
    flip_x: bool,
    flip_y: bool,
    custom_size: Option<[f32; 2]>,
}

// ---- 帮助函数 ------------------------------------------------------------

fn registry_with<T: GetTypeRegistration>() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    registry.register::<T>();
    registry
}

/// 用 `TypedReflectSerializer` —— **不带类型 wrapper**。
fn ser_typed<T: Reflect>(value: &T, registry: &TypeRegistry) -> Value {
    let serializer = TypedReflectSerializer::new(value as &dyn PartialReflect, registry);
    serde_json::to_value(serializer).expect("typed reflect serialize")
}

/// 用 `ReflectSerializer` —— **带类型 wrapper**。
fn ser_wrapped<T: Reflect>(value: &T, registry: &TypeRegistry) -> Value {
    let serializer = ReflectSerializer::new(value as &dyn PartialReflect, registry);
    serde_json::to_value(serializer).expect("wrapped reflect serialize")
}

fn de_typed<T: FromReflect + GetTypeRegistration>(json: Value, registry: &TypeRegistry) -> T {
    let registration = registry
        .get(std::any::TypeId::of::<T>())
        .expect("type registered");
    let deserializer = TypedReflectDeserializer::new(registration, registry);
    let boxed: Box<dyn PartialReflect> = deserializer
        .deserialize(json)
        .expect("typed reflect deserialize");
    T::from_reflect(boxed.as_ref()).expect("from_reflect")
}

// ---- 测试 ---------------------------------------------------------------

#[test]
fn struct_roundtrip_position() {
    let registry = registry_with::<ReflectPosition>();
    let original = ReflectPosition {
        x: 1.0,
        y: 2.5,
        z: -3.0,
    };
    let json = ser_typed(&original, &registry);
    eprintln!("Position typed JSON: {json}");

    let recovered: ReflectPosition = de_typed(json, &registry);
    assert_eq!(recovered, original, "round-trip identity");
}

/// 验证 Bevy reflect 对 unit/struct enum variant 的输出形态——
/// 这是和我们 `#[serde(tag = "kind")]` 现状最大的潜在差异。
#[test]
fn enum_variant_shape_window_size() {
    let registry = registry_with::<ReflectScalingMode>();
    let json = ser_typed(&ReflectScalingMode::WindowSize, &registry);
    eprintln!("WindowSize typed JSON: {json}");

    let json2 = ser_typed(
        &ReflectScalingMode::Fixed {
            width: 1280.0,
            height: 720.0,
        },
        &registry,
    );
    eprintln!("Fixed{{...}} typed JSON: {json2}");

    let json3 = ser_typed(
        &ReflectScalingMode::FixedVertical {
            viewport_height: 10.0,
        },
        &registry,
    );
    eprintln!("FixedVertical{{...}} typed JSON: {json3}");

    // round-trip 各 variant
    let r1: ReflectScalingMode = de_typed(json.clone(), &registry);
    assert!(matches!(r1, ReflectScalingMode::WindowSize));

    let r2: ReflectScalingMode = de_typed(json2.clone(), &registry);
    assert!(
        matches!(r2, ReflectScalingMode::Fixed { width, height } if width == 1280.0 && height == 720.0)
    );

    let r3: ReflectScalingMode = de_typed(json3.clone(), &registry);
    assert!(
        matches!(r3, ReflectScalingMode::FixedVertical { viewport_height } if viewport_height == 10.0)
    );
}

/// 现状的 internally-tagged JSON（`{"kind": "fixed", "width": ..., "height": ...}`）
/// 能否被 reflect 反序列化？预期：不能。验证我们对"配置兼容性"的判断。
#[test]
fn current_internally_tagged_json_does_not_match_reflect_shape() {
    let registry = registry_with::<ReflectScalingMode>();

    // 这是当前 `OrthoScalingMode` 在配置 RON / JSON 里写的形态。
    let current_form = serde_json::json!({
        "kind": "fixed",
        "width": 1280.0,
        "height": 720.0
    });

    let registration = registry
        .get(std::any::TypeId::of::<ReflectScalingMode>())
        .expect("registered");
    let deserializer = TypedReflectDeserializer::new(registration, &registry);
    let result = deserializer.deserialize(current_form);

    assert!(
        result.is_err(),
        "internally-tagged shape should NOT be accepted by reflect deserializer"
    );
    eprintln!(
        "internally-tagged 失败错误：{}",
        result.expect_err("checked above")
    );
}

#[test]
fn nested_struct_with_enum_field() {
    let mut registry = TypeRegistry::new();
    registry.register::<ReflectCamera2d>();
    registry.register::<ReflectScalingMode>();

    let original = ReflectCamera2d {
        scaling_mode: ReflectScalingMode::Fixed {
            width: 800.0,
            height: 600.0,
        },
        scale: 2.0,
    };
    let json = ser_typed(&original, &registry);
    eprintln!("Camera2d typed JSON: {json}");

    let recovered: ReflectCamera2d = de_typed(json, &registry);
    assert_eq!(recovered, original);
}

#[test]
fn option_array_roundtrip() {
    let mut registry = TypeRegistry::new();
    registry.register::<ReflectSprite2d>();
    registry.register::<ReflectImageBuilder>();

    let with_image = ReflectSprite2d {
        image: Some(ReflectImageBuilder::AssetPath {
            path: "tex.png".into(),
        }),
        color: [1.0, 0.5, 0.25, 1.0],
        flip_x: true,
        flip_y: false,
        custom_size: Some([64.0, 32.0]),
    };
    let json = ser_typed(&with_image, &registry);
    eprintln!("Sprite2d (with image) typed JSON: {json}");

    let recovered: ReflectSprite2d = de_typed(json.clone(), &registry);
    assert_eq!(recovered, with_image);

    let without_image = ReflectSprite2d {
        image: None,
        color: [1.0; 4],
        flip_x: false,
        flip_y: false,
        custom_size: None,
    };
    let json2 = ser_typed(&without_image, &registry);
    eprintln!("Sprite2d (no image) typed JSON: {json2}");
    let recovered2: ReflectSprite2d = de_typed(json2, &registry);
    assert_eq!(recovered2, without_image);
}

/// `ReflectSerializer` 带类型 wrapper 的形态——若我们要自动按"组件名"路由，
/// 这种 wrapper 的 key 是完全限定 type path，会很长。看实际字符串。
#[test]
fn wrapped_serializer_shows_type_path_form() {
    let registry = registry_with::<ReflectPosition>();
    let json = ser_wrapped(
        &ReflectPosition {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        &registry,
    );
    eprintln!("Wrapped Position JSON: {json}");
    // 例如 {"reflect_spike::ReflectPosition": { "x": 1, "y": 2, "z": 3 }}
}

/// 通过 `ReflectComponent` 把 `Box<dyn PartialReflect>` 装回 ECS 实体，
/// 再读回来——这是 set/get 路径。验证完整端到端流程。
#[test]
fn insert_via_reflect_component_then_query() {
    let mut world = World::new();
    let registry = registry_with::<ReflectPosition>();

    let entity = world.spawn_empty().id();

    // 1. 反序列化得到 Box<dyn PartialReflect>
    let json = serde_json::json!({ "x": 7.0, "y": 8.0, "z": 9.0 });
    let registration = registry
        .get(std::any::TypeId::of::<ReflectPosition>())
        .expect("registered");
    let deserializer = TypedReflectDeserializer::new(registration, &registry);
    let boxed = deserializer.deserialize(json).expect("deserialize");

    // 2. 通过 ReflectComponent 把它装进 entity
    let reflect_component = registration
        .data::<bevy_ecs::reflect::ReflectComponent>()
        .expect("ReflectComponent metadata present");
    let mut entity_mut = world.entity_mut(entity);
    reflect_component.insert(&mut entity_mut, boxed.as_ref(), &registry);

    // 3. 直接按强类型读回，确认值正确
    let position = world
        .entity(entity)
        .get::<ReflectPosition>()
        .expect("Position present");
    assert_eq!(
        *position,
        ReflectPosition {
            x: 7.0,
            y: 8.0,
            z: 9.0
        }
    );
}

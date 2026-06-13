//! 渲染同步层：把沙箱 [`VmWorld`] 的实体投影到 Bevy 主 World 的渲染实体。
//!
//! 这是「平行宇宙」架构的桥：沙箱是独立的逻辑 World（只有数据），Bevy 主 World
//! 持有渲染管线与资产。两者实体 id 不通用，故维护一张 [`EntityMap`] 映射，并每帧
//! 增量同步——新增者 spawn 渲染实体、存活者更新 `Transform`、消失者 despawn。
//!
//! 沙箱实体靠两个组件被渲染：
//! - `Position{x,y,z}`、可选 `rotation{x,y,z}`（欧拉角度）/ `scale{x,y,z}`——变换；
//! - `Renderable`（内容层动态组件）——渲染意图，由 `kind` 字段分派：
//!   - `cube` / `sphere`：程序图元 + PBR 材质；
//!   - `scene`：glTF 场景（`asset` 指向 .glb/.gltf，走 `SceneRoot`）；
//!   - `mesh`：自定义网格资产 + PBR 材质。
//!
//! 资产引用是透传给 [`AssetServer`] 的字符串，支持 `source://path#label` 形式——
//! 来源（文件系统 / 网络 / 预加载）由 Bevy 的 asset source 注册机制承载，本层不做
//! 任何路径假设；相同资产串经 [`AssetCache`] 去重，只加载一次。
//!
//! 仅在 `render` feature 下编译，避免日常 test/clippy 拉入完整 bevy。

use crate::VmWorld;
use bevy::prelude::*;
use bevy_ecs::entity::Entity as VmEntity;
use bevy_ecs::system::SystemParam;
use std::collections::{HashMap, HashSet};

/// 沙箱实体声明渲染意图的组件名。
const RENDERABLE: &str = "Renderable";
/// 沙箱实体的位置组件名。
const POSITION: &str = "Position";
/// 沙箱实体的旋转组件名（欧拉角度，度）。
const ROTATION: &str = "rotation";
/// 沙箱实体的缩放组件名。
const SCALE: &str = "scale";

/// 程序图元默认边长/直径。
const DEFAULT_SIZE: f32 = 1.0;
/// 默认基色（无指定时的中性灰）。
const DEFAULT_COLOR: [f32; 3] = [0.8, 0.8, 0.8];
/// 默认粗糙度。
const DEFAULT_ROUGHNESS: f32 = 0.8;
/// 默认金属度。
const DEFAULT_METALLIC: f32 = 0.0;

/// 把 [`VmWorld`] 当作 NonSend 资源驱动并渲染的插件。
///
/// 它每帧 tick 沙箱、再把结果同步到渲染实体。`VmWorld` 非 `Send`（内部 `Rc` +
/// Rhai 引擎），故只能作 NonSend 资源、其 system 钉在主线程——这对单世界零代价，
/// 因为沙箱本就串行 tick。
pub struct VmViewerPlugin;

impl Plugin for VmViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EntityMap>()
            .init_resource::<AssetCache>()
            .add_systems(Update, (tick_vm, sync_render).chain());
    }
}

/// 沙箱实体 → 渲染实体的持久映射。
#[derive(Resource, Default)]
struct EntityMap {
    forward: HashMap<VmEntity, Entity>,
}

/// 资产句柄缓存：相同资产串只加载一次后复用。
///
/// 也是「预加载」的天然落点——预加载即提前把句柄填进这些表。
#[derive(Resource, Default)]
struct AssetCache {
    scenes: HashMap<String, Handle<Scene>>,
    meshes: HashMap<String, Handle<Mesh>>,
    images: HashMap<String, Handle<Image>>,
}

impl AssetCache {
    /// 取一个 glTF 场景句柄，缺失则经 `AssetServer` 加载并缓存。
    fn scene(&mut self, server: &AssetServer, asset: &str) -> Handle<Scene> {
        self.scenes
            .entry(asset.to_owned())
            .or_insert_with(|| server.load(asset.to_owned()))
            .clone()
    }

    /// 取一个网格资产句柄，缺失则加载并缓存。
    fn mesh(&mut self, server: &AssetServer, asset: &str) -> Handle<Mesh> {
        self.meshes
            .entry(asset.to_owned())
            .or_insert_with(|| server.load(asset.to_owned()))
            .clone()
    }

    /// 取一个贴图句柄，缺失则加载并缓存。
    fn image(&mut self, server: &AssetServer, asset: &str) -> Handle<Image> {
        self.images
            .entry(asset.to_owned())
            .or_insert_with(|| server.load(asset.to_owned()))
            .clone()
    }
}

/// 每帧推进沙箱一个 tick。
fn tick_vm(mut vm: NonSendMut<VmWorld>) {
    if let Err(error) = vm.tick() {
        warn!("沙箱 tick 失败: {error}");
    }
}

/// 把沙箱中带 `Renderable` 的实体增量同步到渲染实体。
fn sync_render(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<EntityMap>,
    mut assets: AssetBuilder,
    mut commands: Commands,
    mut transforms: Query<&mut Transform>,
) {
    let live = vm.query(RENDERABLE);
    let live_set: HashSet<VmEntity> = live.iter().copied().collect();

    for vm_entity in live {
        let transform = read_transform(&vm, vm_entity);
        match map.forward.get(&vm_entity) {
            Some(&render_entity) => {
                if let Ok(mut existing) = transforms.get_mut(render_entity) {
                    *existing = transform;
                }
            }
            None => {
                let appearance = Appearance::read(&vm, vm_entity);
                let render_entity =
                    spawn_render_entity(&mut commands, &mut assets, &appearance, transform);
                map.forward.insert(vm_entity, render_entity);
            }
        }
    }

    despawn_missing(&mut map, &live_set, &mut commands);
}

/// 构建渲染实体所需的资产相关 system 参数集合。
#[derive(SystemParam)]
struct AssetBuilder<'w> {
    cache: ResMut<'w, AssetCache>,
    server: Res<'w, AssetServer>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

/// 按外观种类 spawn 一个渲染实体，返回其 id。
fn spawn_render_entity(
    commands: &mut Commands,
    assets: &mut AssetBuilder,
    appearance: &Appearance,
    transform: Transform,
) -> Entity {
    match appearance {
        Appearance::Primitive {
            shape,
            size,
            material,
        } => {
            let mesh = assets.meshes.add(shape.mesh(*size));
            let material = assets
                .materials
                .add(material.build(&mut assets.cache, &assets.server));
            commands
                .spawn((Mesh3d(mesh), MeshMaterial3d(material), transform))
                .id()
        }
        Appearance::Mesh { asset, material } => {
            let mesh = assets.cache.mesh(&assets.server, asset);
            let material = assets
                .materials
                .add(material.build(&mut assets.cache, &assets.server));
            commands
                .spawn((Mesh3d(mesh), MeshMaterial3d(material), transform))
                .id()
        }
        Appearance::Scene { asset } => {
            let scene = assets.cache.scene(&assets.server, asset);
            commands.spawn((SceneRoot(scene), transform)).id()
        }
    }
}

/// 销毁沙箱中已消失实体对应的渲染实体。
fn despawn_missing(map: &mut EntityMap, live: &HashSet<VmEntity>, commands: &mut Commands) {
    map.forward.retain(|vm_entity, render_entity| {
        let alive = live.contains(vm_entity);
        if !alive {
            commands.entity(*render_entity).despawn();
        }
        alive
    });
}

/// 读取沙箱实体的位置/旋转/缩放，翻译为渲染 [`Transform`]。
fn read_transform(vm: &VmWorld, entity: VmEntity) -> Transform {
    let translation = read_vec3(vm, entity, POSITION, 0.0);
    let euler = read_vec3(vm, entity, ROTATION, 0.0);
    let scale = read_vec3(vm, entity, SCALE, 1.0);
    Transform {
        translation,
        rotation: Quat::from_euler(
            EulerRot::XYZ,
            euler.x.to_radians(),
            euler.y.to_radians(),
            euler.z.to_radians(),
        ),
        scale,
    }
}

/// 从某组件读取 `{x,y,z}` 三轴；组件或字段缺失时各轴回退 `fallback`。
fn read_vec3(vm: &VmWorld, entity: VmEntity, component: &str, fallback: f32) -> Vec3 {
    let axis = |path: &str| {
        vm.get(entity, component, path)
            .ok()
            .and_then(|v| value_as_f32(&v))
            .unwrap_or(fallback)
    };
    Vec3::new(axis("x"), axis("y"), axis("z"))
}

/// 渲染意图：从沙箱 `Renderable` 组件按 `kind` 读出的外观。
enum Appearance {
    /// 程序图元（cube/sphere）+ PBR 材质。
    Primitive {
        shape: Shape,
        size: f32,
        material: MaterialSpec,
    },
    /// 自定义网格资产 + PBR 材质。
    Mesh {
        asset: String,
        material: MaterialSpec,
    },
    /// glTF 场景（含自带材质，不叠加我们的 PBR 材质）。
    Scene { asset: String },
}

/// 支持的内置图元形状。
enum Shape {
    Cube,
    Sphere,
}

impl Shape {
    /// 构造该形状给定尺寸的网格。
    fn mesh(&self, size: f32) -> Mesh {
        match self {
            Shape::Cube => Cuboid::new(size, size, size).into(),
            Shape::Sphere => Sphere::new(size * 0.5).into(),
        }
    }
}

impl Appearance {
    /// 从沙箱实体的 `Renderable` 组件读取外观，按 `kind` 分派。
    fn read(vm: &VmWorld, entity: VmEntity) -> Self {
        let kind = vm
            .get(entity, RENDERABLE, "kind")
            .ok()
            .and_then(string_value)
            .unwrap_or_else(|| "cube".to_owned());

        match kind.as_str() {
            "scene" => Self::Scene {
                asset: read_asset(vm, entity),
            },
            "mesh" => Self::Mesh {
                asset: read_asset(vm, entity),
                material: MaterialSpec::read(vm, entity),
            },
            "sphere" => Self::Primitive {
                shape: Shape::Sphere,
                size: read_size(vm, entity),
                material: MaterialSpec::read(vm, entity),
            },
            _ => Self::Primitive {
                shape: Shape::Cube,
                size: read_size(vm, entity),
                material: MaterialSpec::read(vm, entity),
            },
        }
    }
}

/// PBR 材质参数，从 `Renderable.material` 子结构读取。
struct MaterialSpec {
    base_color: [f32; 3],
    base_color_texture: Option<String>,
    normal_map_texture: Option<String>,
    metallic: f32,
    roughness: f32,
    emissive: [f32; 3],
    alpha: f32,
}

impl MaterialSpec {
    /// 从实体的 `Renderable.material` 读取，字段缺失走默认。
    ///
    /// 兼容旧形态：若顶层有 `color` 而无 `material`，仍读顶层 `color` 作基色。
    fn read(vm: &VmWorld, entity: VmEntity) -> Self {
        let at = |field: &str, path: &str| format!("material.{field}{path}");
        let scalar = |full: &str| {
            vm.get(entity, RENDERABLE, full)
                .ok()
                .and_then(|v| value_as_f32(&v))
        };
        let base_color = read_rgb(vm, entity, &at("base_color", ""))
            .or_else(|| read_rgb(vm, entity, "color"))
            .unwrap_or(DEFAULT_COLOR);
        Self {
            base_color,
            base_color_texture: vm
                .get(entity, RENDERABLE, &at("base_color_texture", ""))
                .ok()
                .and_then(string_value),
            normal_map_texture: vm
                .get(entity, RENDERABLE, &at("normal_map_texture", ""))
                .ok()
                .and_then(string_value),
            metallic: scalar(&at("metallic", "")).unwrap_or(DEFAULT_METALLIC),
            roughness: scalar(&at("roughness", "")).unwrap_or(DEFAULT_ROUGHNESS),
            emissive: read_rgb(vm, entity, &at("emissive", "")).unwrap_or([0.0, 0.0, 0.0]),
            alpha: scalar(&at("alpha", "")).unwrap_or(1.0),
        }
    }

    /// 构造 Bevy [`StandardMaterial`]，按需加载贴图句柄。
    fn build(&self, cache: &mut AssetCache, server: &AssetServer) -> StandardMaterial {
        let [r, g, b] = self.base_color;
        let [er, eg, eb] = self.emissive;
        let mut material = StandardMaterial {
            base_color: Color::srgba(r, g, b, self.alpha),
            metallic: self.metallic,
            perceptual_roughness: self.roughness,
            emissive: LinearRgba::rgb(er, eg, eb),
            base_color_texture: self
                .base_color_texture
                .as_deref()
                .map(|asset| cache.image(server, asset)),
            normal_map_texture: self
                .normal_map_texture
                .as_deref()
                .map(|asset| cache.image(server, asset)),
            ..default()
        };
        if self.alpha < 1.0 {
            material.alpha_mode = AlphaMode::Blend;
        }
        material
    }
}

/// 读取 `Renderable.asset` 资产引用字符串（缺失返回空串，加载会失败但不崩）。
fn read_asset(vm: &VmWorld, entity: VmEntity) -> String {
    vm.get(entity, RENDERABLE, "asset")
        .ok()
        .and_then(string_value)
        .unwrap_or_default()
}

/// 读取 `Renderable.size`，缺失走默认。
fn read_size(vm: &VmWorld, entity: VmEntity) -> f32 {
    vm.get(entity, RENDERABLE, "size")
        .ok()
        .and_then(|v| value_as_f32(&v))
        .unwrap_or(DEFAULT_SIZE)
}

/// 从给定路径读取一个三元数值序列作 RGB。
fn read_rgb(vm: &VmWorld, entity: VmEntity, path: &str) -> Option<[f32; 3]> {
    let ron::Value::Seq(items) = vm.get(entity, RENDERABLE, path).ok()? else {
        return None;
    };
    let [r, g, b] = items.as_slice() else {
        return None;
    };
    Some([value_as_f32(r)?, value_as_f32(g)?, value_as_f32(b)?])
}

/// 把 [`ron::Value`] 解释为 `f32`。
fn value_as_f32(value: &ron::Value) -> Option<f32> {
    match value {
        ron::Value::Number(number) => Some(number.into_f64() as f32),
        _ => None,
    }
}

/// 取出 [`ron::Value`] 中的字符串。
fn string_value(value: ron::Value) -> Option<String> {
    match value {
        ron::Value::String(s) => Some(s),
        _ => None,
    }
}

/// 便于 viewer 把已构建的 [`VmWorld`] 注入 App 作 NonSend 资源。
///
/// 同时挂上 [`VmViewerPlugin`]。调用方仍需自行加 `DefaultPlugins`、相机与光照。
pub fn insert_vm_world(app: &mut App, vm: VmWorld) {
    app.insert_non_send_resource(vm).add_plugins(VmViewerPlugin);
}

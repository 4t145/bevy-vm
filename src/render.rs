//! 渲染同步层：把沙箱 [`VmWorld`] 的实体投影到 Bevy 主 World 的渲染实体。
//!
//! 这是「平行宇宙」架构的桥：沙箱是独立的逻辑 World（只有数据），Bevy 主 World
//! 持有渲染管线与资产。两者实体 id 不通用，故维护一张 [`EntityMap`] 映射，并每帧
//! 增量同步——新增者 spawn 渲染实体、存活者更新 `Transform`、消失者 despawn。
//!
//! 沙箱实体靠两个组件被渲染：
//! - `Position{x,y,z}`（引擎层类型化）——平移，翻译到渲染 `Transform`；
//! - `Renderable`（内容层动态组件）——渲染意图，声明形状/尺寸/颜色。
//!
//! 仅在 `render` feature 下编译，避免日常 test/clippy 拉入完整 bevy。

use crate::VmWorld;
use bevy::prelude::*;
use bevy_ecs::entity::Entity as VmEntity;
use std::collections::HashMap;

/// 沙箱实体声明渲染意图的组件名。
const RENDERABLE: &str = "Renderable";
/// 沙箱实体的位置组件名。
const POSITION: &str = "Position";

/// 默认图元边长/半径。
const DEFAULT_SIZE: f32 = 1.0;
/// 默认颜色（无指定时的中性灰）。
const DEFAULT_COLOR: [f32; 3] = [0.8, 0.8, 0.8];

/// 把 [`VmWorld`] 当作 NonSend 资源驱动并渲染的插件。
///
/// 它每帧 tick 沙箱、再把结果同步到渲染实体。`VmWorld` 非 `Send`（内部 `Rc` +
/// Rhai 引擎），故只能作 NonSend 资源、其 system 钉在主线程——这对单世界零代价，
/// 因为沙箱本就串行 tick。
pub struct VmViewerPlugin;

impl Plugin for VmViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EntityMap>()
            .add_systems(Update, (tick_vm, sync_render).chain());
    }
}

/// 沙箱实体 → 渲染实体的持久映射。
#[derive(Resource, Default)]
struct EntityMap {
    forward: HashMap<VmEntity, Entity>,
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
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform>,
) {
    let live = vm.query(RENDERABLE);
    let live_set: std::collections::HashSet<VmEntity> = live.iter().copied().collect();

    for vm_entity in live {
        let position = read_position(&vm, vm_entity);
        match map.forward.get(&vm_entity) {
            Some(&render_entity) => {
                if let Ok(mut transform) = transforms.get_mut(render_entity) {
                    transform.translation = position;
                }
            }
            None => {
                let appearance = Appearance::read(&vm, vm_entity);
                let render_entity = commands
                    .spawn((
                        Mesh3d(meshes.add(appearance.mesh())),
                        MeshMaterial3d(materials.add(appearance.material())),
                        Transform::from_translation(position),
                    ))
                    .id();
                map.forward.insert(vm_entity, render_entity);
            }
        }
    }

    despawn_missing(&mut map, &live_set, &mut commands);
}

/// 销毁沙箱中已消失实体对应的渲染实体。
fn despawn_missing(
    map: &mut EntityMap,
    live: &std::collections::HashSet<VmEntity>,
    commands: &mut Commands,
) {
    map.forward.retain(|vm_entity, render_entity| {
        let alive = live.contains(vm_entity);
        if !alive {
            commands.entity(*render_entity).despawn();
        }
        alive
    });
}

/// 读取沙箱实体的位置，翻译为渲染坐标；缺失字段回退 0。
fn read_position(vm: &VmWorld, entity: VmEntity) -> Vec3 {
    let axis = |path: &str| {
        vm.get(entity, POSITION, path)
            .ok()
            .and_then(|v| value_as_f32(&v))
            .unwrap_or(0.0)
    };
    Vec3::new(axis("x"), axis("y"), axis("z"))
}

/// 渲染意图：从沙箱 `Renderable` 组件读出的形状与颜色。
struct Appearance {
    shape: Shape,
    size: f32,
    color: [f32; 3],
}

/// 支持的内置图元形状。
enum Shape {
    Cube,
    Sphere,
}

impl Appearance {
    /// 从沙箱实体的 `Renderable` 组件读取外观，字段缺失时回退默认。
    fn read(vm: &VmWorld, entity: VmEntity) -> Self {
        let shape = match vm.get(entity, RENDERABLE, "shape").ok().as_ref() {
            Some(ron::Value::String(s)) if s == "sphere" => Shape::Sphere,
            _ => Shape::Cube,
        };
        let size = vm
            .get(entity, RENDERABLE, "size")
            .ok()
            .and_then(|v| value_as_f32(&v))
            .unwrap_or(DEFAULT_SIZE);
        let color = read_color(vm, entity).unwrap_or(DEFAULT_COLOR);
        Self { shape, size, color }
    }

    /// 构造该外观对应的网格。
    fn mesh(&self) -> Mesh {
        match self.shape {
            Shape::Cube => Cuboid::new(self.size, self.size, self.size).into(),
            Shape::Sphere => Sphere::new(self.size * 0.5).into(),
        }
    }

    /// 构造该外观对应的材质。
    fn material(&self) -> StandardMaterial {
        let [r, g, b] = self.color;
        StandardMaterial::from(Color::srgb(r, g, b))
    }
}

/// 从 `Renderable.color`（一个三元数值序列）读取 RGB。
fn read_color(vm: &VmWorld, entity: VmEntity) -> Option<[f32; 3]> {
    let ron::Value::Seq(items) = vm.get(entity, RENDERABLE, "color").ok()? else {
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

/// 便于 viewer 把已构建的 [`VmWorld`] 注入 App 作 NonSend 资源。
///
/// 同时挂上 [`VmViewerPlugin`]。调用方仍需自行加 `DefaultPlugins`、相机与光照。
pub fn insert_vm_world(app: &mut App, vm: VmWorld) {
    app.insert_non_send_resource(vm).add_plugins(VmViewerPlugin);
}

//! 渲染同步层：把沙箱 [`VmWorld`] 的实体投影到 Bevy 主 World 的渲染实体。
//!
//! # 设计原则
//!
//! 不再有"大而全的 `Renderable` enum"——VM 端拆分成独立 typed 组件：
//! - [`crate::component::sprite::Sprite2d`] —— Bevy `Sprite` 的镜像。
//! - [`crate::component::mesh::Mesh3dRender`] —— Bevy `(Mesh3d, MeshMaterial3d)` 的合成。
//! - [`crate::component::scene::SceneRender`] —— Bevy `SceneRoot` 的镜像。
//! - [`crate::component::camera::Camera3d/Camera2d`]、[`crate::component::text::TextLabel`]——已有。
//!
//! 资源字段（mesh / material / image）持 [`crate::resource::ResourceBuilder`]——
//! 同步层第一次见到时调用 `build()` 拿 Bevy `Handle<R>`，按 `cache_key()` 在
//! [`ResourceCache`] 缓存复用。
//!
//! 每种组件一个独立 sync system，结构整齐：
//! - 第一次见到（VM 实体在 EntityMap 没记录）→ spawn 渲染实体 + cache builders。
//! - 已存在 → 透明地"按字段"更新（`Sprite.color = ...`、`StandardMaterial.base_color = ...`）；
//!   builder 改了（`cache_key` 变了）→ 重新 resolve handle 并替换。
//! - VM 实体消失 → despawn 镜像。
//!
//! 仅在 `bevy-bridge` feature 下编译，避免 headless 拉入完整 bevy。

use crate::VmWorld;
use crate::component::camera::{
    Camera2d as VmCamera2d, Camera3d as VmCamera3d, CameraProjection, OrthoScalingMode,
};
use crate::component::mesh::Mesh3dRender;
use crate::component::picking::Pickable as VmPickable;
use crate::component::scene::SceneRender;
use crate::component::sprite::Sprite2d as VmSprite2d;
use crate::component::text::TextLabel as VmTextLabel;
use crate::component::{Position, Rotation, Scale};
use crate::resource::material::MaterialBuilder;
use crate::resource::mesh::MeshBuilder;
use crate::resource::{BuildContext, CacheKey, ResourceBuilder, ResourceCache};
use bevy::prelude::*;
use bevy_ecs::entity::Entity as VmEntity;
use bevy_ecs::system::SystemParam;
use std::collections::{HashMap, HashSet};

/// 把 [`VmWorld`] 当作 NonSend 资源驱动并渲染的插件。
///
/// 它每帧 tick 沙箱、再把结果同步到渲染实体。`VmWorld` 非 `Send`（内部 `Rc` +
/// Rhai 引擎），故只能作 NonSend 资源、其 system 钉在主线程——这对单世界零代价，
/// 因为沙箱本就串行 tick。
pub struct VmViewerPlugin;

/// SystemSet covering the VM's per-frame tick + render sync.
///
/// Pump systems wired by [`VmEventAppExt::add_vm_event_in`] /
/// [`VmEventAppExt::add_vm_event_out`] use `.before(VmTickSet)` /
/// `.after(VmTickSet)` for ordering.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct VmTickSet;

impl Plugin for VmViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteEntityMap>()
            .init_resource::<MeshEntityMap>()
            .init_resource::<SceneEntityMap>()
            .init_resource::<CameraMap3d>()
            .init_resource::<CameraMap2d>()
            .init_resource::<TextMap>()
            .init_resource::<UiEntityMap>()
            .init_resource::<ResourceCache>()
            .add_systems(
                Update,
                (
                    tick_vm,
                    sync_sprites,
                    sync_meshes,
                    sync_scenes,
                    sync_cameras_3d,
                    sync_cameras_2d,
                    sync_text_labels,
                    sync_ui,
                    sync_hierarchy,
                )
                    .chain()
                    .in_set(VmTickSet),
            );
    }
}

/// Sprite 渲染实体的沙箱 → 渲染映射。每种渲染组件独立一张表——共享会让
/// 一个 system 的 despawn_missing 误删另一种组件的镜像（live_set 是按
/// 组件类型 collect 的）。
#[derive(Resource, Default)]
struct SpriteEntityMap {
    forward: HashMap<VmEntity, Entity>,
}

/// Mesh3d 渲染实体的映射，独立于 sprite / scene。
#[derive(Resource, Default)]
struct MeshEntityMap {
    forward: HashMap<VmEntity, Entity>,
}

/// Scene 渲染实体的映射，独立于 sprite / mesh。
#[derive(Resource, Default)]
struct SceneEntityMap {
    forward: HashMap<VmEntity, Entity>,
}

/// 渲染实体 → 沙箱实体的反向引用。挂在被 picking 选中的渲染实体上，让
/// observer 拿到 `Trigger<Pointer<...>>` 的 target 后能反查 VM 端的实体 id。
///
/// 公开暴露给 [`crate::plugin::picking`] 使用。
#[derive(Component, Debug, Clone, Copy)]
pub struct VmEntityRef(pub VmEntity);

/// 缓存最近一次 spawn / 同步时使用的 builder cache key——同步系统据此判断
/// builder 是否变化（变 ⇒ 需重新 resolve handle 并替换 mesh / material）。
#[derive(Component)]
struct LastMeshKey(CacheKey<Mesh>);

/// 同上，针对材质。
#[derive(Component)]
struct LastMaterialKey(CacheKey<StandardMaterial>);

/// 同上，针对 sprite 图像（可能为 None ⇒ 纯色 sprite）。
#[derive(Component)]
struct LastImageKey(Option<CacheKey<bevy::image::Image>>);

/// 3D 相机的沙箱实体 → 渲染相机实体映射。
#[derive(Resource, Default)]
struct CameraMap3d {
    forward: HashMap<VmEntity, Entity>,
}

/// 2D 相机的沙箱实体 → 渲染相机实体映射。
#[derive(Resource, Default)]
struct CameraMap2d {
    forward: HashMap<VmEntity, Entity>,
}

/// 文本标签实体的沙箱 → 渲染映射。
#[derive(Resource, Default)]
struct TextMap {
    forward: HashMap<VmEntity, Entity>,
}

/// UI 实体的沙箱 → 渲染映射。VM 端挂 `bevy::ui::Node` 的实体对应
/// 主 World 一个 UI 镜像实体。
#[derive(Resource, Default)]
struct UiEntityMap {
    forward: HashMap<VmEntity, Entity>,
}

/// 每帧推进沙箱一个 tick。
///
/// 忠实转发主世界 [`bevy::time::Time`] 的 `delta` 给 VM——viewer 路径的
/// example 作者无需手动管理时间。VM 内部脚本通过 host 函数 `time()` /
/// `delta()` 读到的就是主世界节拍。
fn tick_vm(mut vm: NonSendMut<VmWorld>, time: Res<Time>) {
    vm.advance_time(time.delta());
    if let Err(error) = vm.tick() {
        warn!("沙箱 tick 失败: {error}");
    }
}

// ============================================================================
// Sprite 同步
// ============================================================================

#[derive(SystemParam)]
struct SpriteSyncParams<'w, 's> {
    server: Res<'w, AssetServer>,
    cache: ResMut<'w, ResourceCache>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<bevy::image::Image>>,
    sprites: Query<'w, 's, (&'static mut Sprite, &'static mut LastImageKey)>,
}

/// 同步 [`Sprite2d`] 组件到 Bevy 端的 `Sprite` 实体。
fn sync_sprites(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<SpriteEntityMap>,
    mut commands: Commands,
    mut transforms: Query<&mut Transform>,
    mut params: SpriteSyncParams,
) {
    let live = collect_sprites(vm.world_mut());
    let live_set: HashSet<VmEntity> = live.iter().map(|(e, _, _, _)| *e).collect();

    for (vm_entity, sprite_spec, transform, pickable) in live {
        let new_image_key = sprite_spec
            .image
            .as_ref()
            .map(super::resource::ResourceBuilder::cache_key);

        let existing = map.forward.get(&vm_entity).copied();
        if let Some(render_entity) = existing {
            if let Ok(mut tf) = transforms.get_mut(render_entity) {
                *tf = transform;
            }
            if let Ok((mut sprite, mut last_key)) = params.sprites.get_mut(render_entity) {
                // image 变化 ⇒ 重新 resolve。
                if last_key.0 != new_image_key {
                    sprite.image = resolve_sprite_image(
                        sprite_spec.image.as_ref(),
                        &mut BuildContext {
                            server: &params.server,
                            meshes: &mut params.meshes,
                            materials: &mut params.materials,
                            images: &mut params.images,
                            cache: &mut params.cache,
                        },
                    );
                    last_key.0 = new_image_key;
                }
                sprite.color = sprite_spec.color;
                sprite.flip_x = sprite_spec.flip_x;
                sprite.flip_y = sprite_spec.flip_y;
                sprite.custom_size = sprite_spec.custom_size.map(|[w, h]| Vec2::new(w, h));
            }
        } else {
            let render_entity = spawn_sprite(
                &mut commands,
                &sprite_spec,
                transform,
                vm_entity,
                pickable,
                new_image_key,
                &mut BuildContext {
                    server: &params.server,
                    meshes: &mut params.meshes,
                    materials: &mut params.materials,
                    images: &mut params.images,
                    cache: &mut params.cache,
                },
            );
            map.forward.insert(vm_entity, render_entity);
        }
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

fn collect_sprites(
    world: &mut bevy_ecs::world::World,
) -> Vec<(VmEntity, VmSprite2d, Transform, bool)> {
    let mut query = world.query::<(
        VmEntity,
        &VmSprite2d,
        Option<&Position>,
        Option<&Rotation>,
        Option<&Scale>,
        Option<&VmPickable>,
    )>();
    query
        .iter(world)
        .map(|(entity, sprite, position, rotation, scale, pickable)| {
            (
                entity,
                sprite.clone(),
                build_transform(position, rotation, scale),
                pickable.is_some(),
            )
        })
        .collect()
}

fn spawn_sprite(
    commands: &mut Commands,
    spec: &VmSprite2d,
    transform: Transform,
    vm_entity: VmEntity,
    pickable: bool,
    image_key: Option<CacheKey<bevy::image::Image>>,
    ctx: &mut BuildContext<'_>,
) -> Entity {
    // 没有显式 image 时走 Sprite::from_color——它的 image 是 Handle::default()，
    // Bevy 的 sprite 渲染器把 default handle 当成"白底纯色"处理（与之前
    // Renderable::Sprite 行为一致）。
    let mut sprite = match spec.image.as_ref() {
        Some(builder) => {
            let image = resolve_sprite_image(Some(builder), ctx);
            Sprite {
                image,
                color: spec.color,
                ..default()
            }
        }
        None => {
            let size = spec
                .custom_size
                .map(|[w, h]| Vec2::new(w, h))
                .unwrap_or(Vec2::ONE);
            Sprite::from_color(spec.color, size)
        }
    };
    sprite.flip_x = spec.flip_x;
    sprite.flip_y = spec.flip_y;
    if let Some([w, h]) = spec.custom_size {
        sprite.custom_size = Some(Vec2::new(w, h));
    }
    let mut entity = commands.spawn((
        sprite,
        transform,
        LastImageKey(image_key),
        VmEntityRef(vm_entity),
    ));
    if pickable {
        entity.insert(bevy::picking::Pickable::default());
    }
    entity.id()
}

fn resolve_sprite_image(
    builder: Option<&crate::resource::image::ImageBuilder>,
    ctx: &mut BuildContext<'_>,
) -> Handle<bevy::image::Image> {
    let Some(builder) = builder else {
        return Handle::default();
    };
    let key = builder.cache_key();
    if let Some(handle) = ctx.cache.get(key) {
        return handle;
    }
    let handle = builder.build(ctx);
    ctx.cache.insert(key, handle.clone());
    handle
}

// ============================================================================
// Mesh 同步
// ============================================================================

#[derive(SystemParam)]
struct MeshSyncParams<'w, 's> {
    server: Res<'w, AssetServer>,
    cache: ResMut<'w, ResourceCache>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<bevy::image::Image>>,
    existing: Query<
        'w,
        's,
        (
            &'static mut Mesh3d,
            &'static mut MeshMaterial3d<StandardMaterial>,
            &'static mut LastMeshKey,
            &'static mut LastMaterialKey,
        ),
    >,
}

/// 同步 [`Mesh3dRender`] 组件到 Bevy 端的 `(Mesh3d, MeshMaterial3d)` 实体。
fn sync_meshes(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<MeshEntityMap>,
    mut commands: Commands,
    mut transforms: Query<&mut Transform>,
    mut params: MeshSyncParams,
) {
    let live = collect_meshes(vm.world_mut());
    let live_set: HashSet<VmEntity> = live.iter().map(|(e, _, _, _)| *e).collect();

    for (vm_entity, render_spec, transform, pickable) in live {
        let new_mesh_key = render_spec.mesh.cache_key();
        let new_material_key = render_spec.material.cache_key();

        let existing = map.forward.get(&vm_entity).copied();
        if let Some(render_entity) = existing {
            if let Ok(mut tf) = transforms.get_mut(render_entity) {
                *tf = transform;
            }
            if let Ok((mut mesh3d, mut material3d, mut last_mesh_key, mut last_mat_key)) =
                params.existing.get_mut(render_entity)
            {
                let mut ctx = BuildContext {
                    server: &params.server,
                    meshes: &mut params.meshes,
                    materials: &mut params.materials,
                    images: &mut params.images,
                    cache: &mut params.cache,
                };
                if last_mesh_key.0 != new_mesh_key {
                    mesh3d.0 = resolve_mesh(&render_spec.mesh, &mut ctx);
                    last_mesh_key.0 = new_mesh_key;
                }
                if last_mat_key.0 != new_material_key {
                    material3d.0 = resolve_material(&render_spec.material, &mut ctx);
                    last_mat_key.0 = new_material_key;
                }
            }
        } else {
            let mut ctx = BuildContext {
                server: &params.server,
                meshes: &mut params.meshes,
                materials: &mut params.materials,
                images: &mut params.images,
                cache: &mut params.cache,
            };
            let mesh = resolve_mesh(&render_spec.mesh, &mut ctx);
            let material = resolve_material(&render_spec.material, &mut ctx);
            let mut entity = commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                transform,
                LastMeshKey(new_mesh_key),
                LastMaterialKey(new_material_key),
                // 所有 VM 镜像都挂 VmEntityRef——viewer 切换 world 时按它统一
                // 回收。pickable 仅在脚本明确声明时附加。
                VmEntityRef(vm_entity),
            ));
            if pickable {
                entity.insert(bevy::picking::Pickable::default());
            }
            map.forward.insert(vm_entity, entity.id());
        }
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

fn collect_meshes(
    world: &mut bevy_ecs::world::World,
) -> Vec<(VmEntity, Mesh3dRender, Transform, bool)> {
    let mut query = world.query::<(
        VmEntity,
        &Mesh3dRender,
        Option<&Position>,
        Option<&Rotation>,
        Option<&Scale>,
        Option<&VmPickable>,
    )>();
    query
        .iter(world)
        .map(|(entity, render, position, rotation, scale, pickable)| {
            (
                entity,
                render.clone(),
                build_transform(position, rotation, scale),
                pickable.is_some(),
            )
        })
        .collect()
}

fn resolve_mesh(builder: &MeshBuilder, ctx: &mut BuildContext<'_>) -> Handle<Mesh> {
    let key = builder.cache_key();
    if let Some(handle) = ctx.cache.get(key) {
        return handle;
    }
    let handle = builder.build(ctx);
    ctx.cache.insert(key, handle.clone());
    handle
}

fn resolve_material(
    builder: &MaterialBuilder,
    ctx: &mut BuildContext<'_>,
) -> Handle<StandardMaterial> {
    let key = builder.cache_key();
    if let Some(handle) = ctx.cache.get(key) {
        return handle;
    }
    let handle = builder.build(ctx);
    ctx.cache.insert(key, handle.clone());
    handle
}

// ============================================================================
// Scene 同步
// ============================================================================

/// 同步 [`SceneRender`] 组件到 Bevy 端 `SceneRoot` 实体。
fn sync_scenes(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<SceneEntityMap>,
    mut commands: Commands,
    mut transforms: Query<&mut Transform>,
    server: Res<AssetServer>,
) {
    let live = collect_scenes(vm.world_mut());
    let live_set: HashSet<VmEntity> = live.iter().map(|(e, _, _, _)| *e).collect();

    for (vm_entity, scene_spec, transform, pickable) in live {
        let existing = map.forward.get(&vm_entity).copied();
        if let Some(render_entity) = existing {
            if let Ok(mut tf) = transforms.get_mut(render_entity) {
                *tf = transform;
            }
            // SceneRoot 的资产路径变化目前不支持原地切换——退化为 despawn 重建。
            // 真实游戏里同一实体切场景资产很罕见，先不处理。
        } else {
            let scene_handle: Handle<Scene> = server.load(scene_spec.asset.clone());
            let mut entity =
                commands.spawn((SceneRoot(scene_handle), transform, VmEntityRef(vm_entity)));
            if pickable {
                entity.insert(bevy::picking::Pickable::default());
            }
            map.forward.insert(vm_entity, entity.id());
        }
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

fn collect_scenes(
    world: &mut bevy_ecs::world::World,
) -> Vec<(VmEntity, SceneRender, Transform, bool)> {
    let mut query = world.query::<(
        VmEntity,
        &SceneRender,
        Option<&Position>,
        Option<&Rotation>,
        Option<&Scale>,
        Option<&VmPickable>,
    )>();
    query
        .iter(world)
        .map(|(entity, scene, position, rotation, scale, pickable)| {
            (
                entity,
                scene.clone(),
                build_transform(position, rotation, scale),
                pickable.is_some(),
            )
        })
        .collect()
}

// ============================================================================
// 共用 helpers
// ============================================================================

/// 把沙箱位置/旋转/缩放翻译为渲染 [`Transform`]。
fn build_transform(
    position: Option<&Position>,
    rotation: Option<&Rotation>,
    scale: Option<&Scale>,
) -> Transform {
    let translation = position
        .map(|p| Vec3::new(p.x, p.y, p.z))
        .unwrap_or(Vec3::ZERO);
    // Euler 顺序：YXZ（先 yaw、再 pitch、再 roll）。
    // FPS 游戏里相机视角的标准顺序——脚本端 Rotation.y 是 yaw，
    // .x 是 pitch（已绕变换后的 X），.z 是 roll。
    // 单轴使用时 XYZ / YXZ 等价，所以现有 worlds 仅写 .y 的不受影响。
    let rotation = rotation
        .map(|r| {
            Quat::from_euler(
                EulerRot::YXZ,
                r.y.to_radians(),
                r.x.to_radians(),
                r.z.to_radians(),
            )
        })
        .unwrap_or(Quat::IDENTITY);
    let scale = scale.map(|s| Vec3::new(s.x, s.y, s.z)).unwrap_or(Vec3::ONE);
    Transform {
        translation,
        rotation,
        scale,
    }
}

/// 三种渲染同步系统共用的"VM 实体已消失 → despawn 镜像"清理。
///
/// 用 [`EntityCommands::try_despawn`] 而非 `despawn`——VM 端的 ChildOf
/// 关系（如 UI 树）触发 `linked_spawn` 级联 despawn 时，子节点的镜像可能
/// 已经被父节点 despawn 时连带清掉；此后本帧 sync 还会再去 despawn 一次，
/// 普通 `despawn` 在那种情况下会发"interacting with a despawned entity"
/// 警告——本质上是双重 despawn 的友好降级。
fn despawn_missing(
    map: &mut HashMap<VmEntity, Entity>,
    live: &HashSet<VmEntity>,
    commands: &mut Commands,
) {
    map.retain(|vm_entity, render_entity| {
        let alive = live.contains(vm_entity);
        if !alive {
            commands.entity(*render_entity).try_despawn();
        }
        alive
    });
}

// ============================================================================
// 相机同步（保留原实现，仅微调）
// ============================================================================

/// 把沙箱中带 [`VmCamera3d`] 的实体增量同步到 Bevy 端 `Camera3d` 实体。
fn sync_cameras_3d(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<CameraMap3d>,
    mut commands: Commands,
    mut existing: Query<(&mut Transform, &mut Camera, &mut Projection)>,
) {
    let live = collect_cameras_3d(vm.world_mut());
    let live_set: HashSet<VmEntity> = live.iter().map(|(e, _, _)| *e).collect();

    for (vm_entity, camera, position) in live {
        let transform = build_camera_3d_transform(&camera, position);
        let projection = build_projection(&camera.projection);
        match map.forward.get(&vm_entity).copied() {
            Some(render_entity) => {
                if let Ok((mut existing_tf, mut existing_cam, mut existing_proj)) =
                    existing.get_mut(render_entity)
                {
                    *existing_tf = transform;
                    update_camera_common(&mut existing_cam, &camera);
                    *existing_proj = projection;
                }
            }
            None => {
                let render_entity = spawn_camera_3d(&mut commands, &camera, transform, projection);
                // 标记相机镜像归 VM 拥有——viewer 切换 world 时按 VmEntityRef
                // 一并 despawn，避免旧相机残留与新相机同 order 冲突。
                commands
                    .entity(render_entity)
                    .insert(VmEntityRef(vm_entity));
                map.forward.insert(vm_entity, render_entity);
            }
        }
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

/// 把沙箱中带 [`VmCamera2d`] 的实体增量同步到 Bevy 端 `Camera2d` 实体。
fn sync_cameras_2d(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<CameraMap2d>,
    mut commands: Commands,
    mut existing: Query<(&mut Transform, &mut Camera, &mut Projection)>,
) {
    let live = collect_cameras_2d(vm.world_mut());
    let live_set: HashSet<VmEntity> = live.iter().map(|(e, _, _)| *e).collect();

    for (vm_entity, camera, position) in live {
        let transform = build_camera_2d_transform(&camera, position);
        let projection = build_2d_projection(&camera);
        match map.forward.get(&vm_entity).copied() {
            Some(render_entity) => {
                if let Ok((mut existing_tf, mut existing_cam, mut existing_proj)) =
                    existing.get_mut(render_entity)
                {
                    *existing_tf = transform;
                    update_camera_2d_common(&mut existing_cam, &camera);
                    *existing_proj = projection;
                }
            }
            None => {
                let render_entity = spawn_camera_2d(&mut commands, &camera, transform, projection);
                commands
                    .entity(render_entity)
                    .insert(VmEntityRef(vm_entity));
                map.forward.insert(vm_entity, render_entity);
            }
        }
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

fn collect_cameras_3d(
    world: &mut bevy_ecs::world::World,
) -> Vec<(VmEntity, VmCamera3d, Option<Position>)> {
    let mut query = world.query::<(VmEntity, &VmCamera3d, Option<&Position>)>();
    query
        .iter(world)
        .map(|(entity, camera, position)| (entity, camera.clone(), position.copied()))
        .collect()
}

fn collect_cameras_2d(
    world: &mut bevy_ecs::world::World,
) -> Vec<(VmEntity, VmCamera2d, Option<Position>)> {
    let mut query = world.query::<(VmEntity, &VmCamera2d, Option<&Position>)>();
    query
        .iter(world)
        .map(|(entity, camera, position)| (entity, camera.clone(), position.copied()))
        .collect()
}

fn build_camera_3d_transform(camera: &VmCamera3d, position: Option<Position>) -> Transform {
    let eye = position
        .map(|p| Vec3::new(p.x, p.y, p.z))
        .unwrap_or(Vec3::ZERO);
    let target = Vec3::from_array(camera.target);
    let up = Vec3::from_array(camera.up);
    Transform::from_translation(eye).looking_at(target, up)
}

fn build_camera_2d_transform(camera: &VmCamera2d, position: Option<Position>) -> Transform {
    let (x, y) = position.map_or((0.0, 0.0), |p| (p.x, p.y));
    Transform::from_translation(Vec3::new(x, y, camera.z))
}

fn build_projection(projection: &CameraProjection) -> Projection {
    match projection {
        CameraProjection::Perspective {
            fov_degrees,
            near,
            far,
        } => Projection::Perspective(PerspectiveProjection {
            fov: fov_degrees.to_radians(),
            near: *near,
            far: *far,
            ..default()
        }),
        CameraProjection::Orthographic {
            scaling_mode,
            scale,
            near,
            far,
        } => {
            let mut projection = OrthographicProjection::default_3d();
            projection.scaling_mode = to_bevy_scaling_mode(scaling_mode);
            projection.scale = *scale;
            projection.near = *near;
            projection.far = *far;
            Projection::Orthographic(projection)
        }
    }
}

fn build_2d_projection(camera: &VmCamera2d) -> Projection {
    let mut projection = OrthographicProjection::default_2d();
    projection.scaling_mode = to_bevy_scaling_mode(&camera.scaling_mode);
    projection.scale = camera.scale;
    projection.near = camera.near;
    projection.far = camera.far;
    Projection::Orthographic(projection)
}

fn to_bevy_scaling_mode(mode: &OrthoScalingMode) -> bevy::camera::ScalingMode {
    use bevy::camera::ScalingMode;
    match *mode {
        OrthoScalingMode::WindowSize => ScalingMode::WindowSize,
        OrthoScalingMode::Fixed { width, height } => ScalingMode::Fixed { width, height },
        OrthoScalingMode::AutoMin {
            min_width,
            min_height,
        } => ScalingMode::AutoMin {
            min_width,
            min_height,
        },
        OrthoScalingMode::AutoMax {
            max_width,
            max_height,
        } => ScalingMode::AutoMax {
            max_width,
            max_height,
        },
        OrthoScalingMode::FixedVertical { viewport_height } => {
            ScalingMode::FixedVertical { viewport_height }
        }
        OrthoScalingMode::FixedHorizontal { viewport_width } => {
            ScalingMode::FixedHorizontal { viewport_width }
        }
    }
}

fn spawn_camera_3d(
    commands: &mut Commands,
    camera: &VmCamera3d,
    transform: Transform,
    projection: Projection,
) -> Entity {
    commands
        .spawn((
            bevy::prelude::Camera3d::default(),
            Camera {
                order: isize::from(i16::try_from(camera.order).unwrap_or(0)),
                is_active: camera.active,
                clear_color: clear_color_config(camera.clear_color),
                ..default()
            },
            projection,
            transform,
        ))
        .id()
}

fn spawn_camera_2d(
    commands: &mut Commands,
    camera: &VmCamera2d,
    transform: Transform,
    projection: Projection,
) -> Entity {
    commands
        .spawn((
            bevy::prelude::Camera2d,
            Camera {
                order: isize::from(i16::try_from(camera.order).unwrap_or(0)),
                is_active: camera.active,
                clear_color: clear_color_config(camera.clear_color),
                ..default()
            },
            projection,
            transform,
        ))
        .id()
}

fn update_camera_common(target: &mut Camera, source: &VmCamera3d) {
    target.order = isize::from(i16::try_from(source.order).unwrap_or(0));
    target.is_active = source.active;
    target.clear_color = clear_color_config(source.clear_color);
}

fn update_camera_2d_common(target: &mut Camera, source: &VmCamera2d) {
    target.order = isize::from(i16::try_from(source.order).unwrap_or(0));
    target.is_active = source.active;
    target.clear_color = clear_color_config(source.clear_color);
}

fn clear_color_config(custom: Option<Color>) -> ClearColorConfig {
    match custom {
        Some(color) => ClearColorConfig::Custom(color),
        None => ClearColorConfig::Default,
    }
}

// ============================================================================
// 文本同步
// ============================================================================

fn sync_text_labels(
    mut vm: NonSendMut<VmWorld>,
    mut map: ResMut<TextMap>,
    mut commands: Commands,
    mut existing: Query<(&mut Transform, &mut Text2d, &mut TextFont, &mut TextColor)>,
) {
    let live = collect_text_labels(vm.world_mut());
    let live_set: HashSet<VmEntity> = live.iter().map(|(entity, _, _)| *entity).collect();

    for (vm_entity, label, transform) in live {
        match map.forward.get(&vm_entity).copied() {
            Some(render_entity) => {
                if let Ok((mut existing_tf, mut text, mut font, mut color)) =
                    existing.get_mut(render_entity)
                {
                    *existing_tf = transform;
                    if text.0 != label.content {
                        text.0.clone_from(&label.content);
                    }
                    if (font.font_size - label.font_size).abs() > f32::EPSILON {
                        font.font_size = label.font_size;
                    }
                    *color = TextColor(label.color);
                }
            }
            None => {
                let render_entity = commands
                    .spawn((
                        Text2d::new(label.content.clone()),
                        TextFont {
                            font_size: label.font_size,
                            ..default()
                        },
                        TextColor(label.color),
                        transform,
                        VmEntityRef(vm_entity),
                    ))
                    .id();
                map.forward.insert(vm_entity, render_entity);
            }
        }
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

fn collect_text_labels(
    world: &mut bevy_ecs::world::World,
) -> Vec<(VmEntity, VmTextLabel, Transform)> {
    let mut query = world.query::<(
        VmEntity,
        &VmTextLabel,
        Option<&Position>,
        Option<&Rotation>,
        Option<&Scale>,
    )>();
    query
        .iter(world)
        .map(|(entity, label, position, rotation, scale)| {
            (
                entity,
                label.clone(),
                build_transform(position, rotation, scale),
            )
        })
        .collect()
}

// ============================================================================
// UI 同步
// ============================================================================

/// 把 VM 端挂 [`bevy::ui::Node`] 的实体投影到主 World 的 UI 镜像实体。
///
/// 第一版策略：暴力 clone+insert，每帧整体覆盖所有 UI 组件。优化空间见
/// `docs/proposal/ui_sync_incremental.md`。
///
/// 同步的组件清单（VM 端 → 主 World）：
/// - `Node`、`BackgroundColor`、`BorderColor`、`Outline`、`ZIndex`
/// - `Text`、`TextFont`、`TextColor`、`TextLayout`
/// - `Button`（marker，存在性同步）
///
/// 父子关系由 [`sync_hierarchy`] 接管——这里不处理。VM 端 UI 实体上的
/// `ChildOf` 会通过合并的 EntityMap 翻译到主 World。
fn sync_ui(mut vm: NonSendMut<VmWorld>, mut map: ResMut<UiEntityMap>, mut commands: Commands) {
    let world = vm.world_mut();
    let live = collect_ui_entities(world);
    let live_set: HashSet<VmEntity> = live.iter().map(|(e, _, _)| *e).collect();

    for (vm_entity, snapshot, pickable) in live {
        let (render_entity, is_new) = match map.forward.get(&vm_entity).copied() {
            Some(existing) => (existing, false),
            None => (commands.spawn_empty().id(), true),
        };
        apply_ui_snapshot(&mut commands, render_entity, snapshot);
        if is_new {
            // 默认所有 UI 镜像挂"透明" Pickable —— 不阻挡更深层的 hit。
            // Bevy UI picking 对没挂 Pickable 的节点默认 should_block_lower=true，
            // 会让占满屏幕的 root Node 拦截掉 sprite 棋盘的点击。
            //
            // VM 端显式挂了 Pickable 的实体，覆盖为完整的"接收 click 且阻挡
            // 更深层"语义；反查引用 VmEntityRef 在所有 UI 镜像上都挂——viewer
            // 切换 world 时按它统一回收。
            let mut e = commands.entity(render_entity);
            e.insert((
                bevy::picking::Pickable {
                    should_block_lower: false,
                    is_hoverable: false,
                },
                VmEntityRef(vm_entity),
            ));
            if pickable {
                e.insert(bevy::picking::Pickable {
                    should_block_lower: true,
                    is_hoverable: true,
                });
            }
        }
        map.forward.insert(vm_entity, render_entity);
    }

    despawn_missing(&mut map.forward, &live_set, &mut commands);
}

/// 一帧 UI 同步要复制的全部组件——`None` 表示该组件 VM 端未挂，主 World
/// 镜像应清掉对应组件。
struct UiSnapshot {
    node: bevy::ui::Node,
    background: Option<BackgroundColor>,
    border: Option<BorderColor>,
    outline: Option<Outline>,
    z_index: Option<ZIndex>,
    text: Option<bevy::ui::widget::Text>,
    text_font: Option<TextFont>,
    text_color: Option<TextColor>,
    text_layout: Option<TextLayout>,
    button: bool,
}

fn collect_ui_entities(world: &mut bevy_ecs::world::World) -> Vec<(VmEntity, UiSnapshot, bool)> {
    use bevy::ui::widget::Text;
    let mut query = world.query::<(
        VmEntity,
        &bevy::ui::Node,
        Option<&BackgroundColor>,
        Option<&BorderColor>,
        Option<&Outline>,
        Option<&ZIndex>,
        Option<&Text>,
        Option<&TextFont>,
        Option<&TextColor>,
        Option<&TextLayout>,
        Option<&bevy::ui::widget::Button>,
        Option<&VmPickable>,
    )>();
    query
        .iter(world)
        .map(
            |(entity, node, bg, bd, outline, zi, text, tf, tc, tl, button, pickable)| {
                (
                    entity,
                    UiSnapshot {
                        node: node.clone(),
                        background: bg.cloned(),
                        border: bd.cloned(),
                        outline: outline.cloned(),
                        z_index: zi.copied(),
                        text: text.cloned(),
                        text_font: tf.cloned(),
                        text_color: tc.copied(),
                        text_layout: tl.copied(),
                        button: button.is_some(),
                    },
                    pickable.is_some(),
                )
            },
        )
        .collect()
}

fn apply_ui_snapshot(commands: &mut Commands, render_entity: Entity, s: UiSnapshot) {
    let mut e = commands.entity(render_entity);
    e.insert(s.node);
    insert_or_remove(&mut e, s.background);
    insert_or_remove(&mut e, s.border);
    insert_or_remove(&mut e, s.outline);
    insert_or_remove(&mut e, s.z_index);
    insert_or_remove(&mut e, s.text);
    insert_or_remove(&mut e, s.text_font);
    insert_or_remove(&mut e, s.text_color);
    insert_or_remove(&mut e, s.text_layout);
    if s.button {
        e.insert(bevy::ui::widget::Button);
    } else {
        e.remove::<bevy::ui::widget::Button>();
    }
}

/// `Some(c)` → `insert(c)`；`None` → `remove::<C>()`。让 sync 层"VM 端没挂
/// 的组件，主 World 镜像也别挂"，避免上一帧残留状态。
fn insert_or_remove<C: bevy_ecs::component::Component>(
    entity: &mut bevy_ecs::system::EntityCommands<'_>,
    component: Option<C>,
) {
    match component {
        Some(c) => {
            entity.insert(c);
        }
        None => {
            entity.remove::<C>();
        }
    }
}

// ============================================================================
// 父子关系同步
// ============================================================================

/// 把 VM World 里的 [`bevy_ecs::hierarchy::ChildOf`] 关系投影到主 World 的
/// 渲染镜像实体。
///
/// 必须跑在所有渲染 sync 之后——它依赖各 EntityMap 已经为本帧的活实体
/// 建好 VmEntity → render Entity 的映射。
///
/// 处理流程：
/// 1. 把 5 个 EntityMap 合并成一张总表 `VmEntity → render Entity`。
///    （注意：相机的镜像不参与父子关系——挂相机做子节点很罕见，先忽略。）
/// 2. 对每个有渲染镜像的 VM 实体：
///    - 若 VM 端有 `ChildOf(vm_parent)` 且 `vm_parent` 也在总表里 →
///      给镜像挂上 render-side 的 `ChildOf(render_parent)`（若已挂同样的
///      parent，则 noop——`insert` 是覆盖语义但 hooks 不会重复触发）。
///    - 若 VM 端无 `ChildOf` → 镜像也清掉（防止上一帧的关系残留）。
///
/// 有意未处理的情形：父在 VM 端存在但**当前帧**未渲染（无 Sprite/Mesh/Scene/Text），
/// 此时映射缺失，子的镜像会保持顶层。挂了渲染的子节点的父几乎一定也带渲染，
/// 不强行处理这种边角。
#[derive(SystemParam)]
struct HierarchyMaps<'w> {
    sprite: Res<'w, SpriteEntityMap>,
    mesh: Res<'w, MeshEntityMap>,
    scene: Res<'w, SceneEntityMap>,
    text: Res<'w, TextMap>,
    ui: Res<'w, UiEntityMap>,
}

fn sync_hierarchy(
    mut vm: NonSendMut<VmWorld>,
    maps: HierarchyMaps,
    existing: Query<Option<&ChildOf>>,
    mut commands: Commands,
) {
    // 合并所有渲染实体映射。若同一 VM 实体在多张表（不太可能，但稳健起见后到优先）
    let mut entity_map: HashMap<VmEntity, Entity> = HashMap::new();
    for src in [
        &maps.sprite.forward,
        &maps.mesh.forward,
        &maps.scene.forward,
        &maps.text.forward,
        &maps.ui.forward,
    ] {
        for (&vm_entity, &render_entity) in src {
            entity_map.insert(vm_entity, render_entity);
        }
    }

    let world = vm.world_mut();

    // 第一步：按 VM 父实体 Children vec 顺序为每个 child 计算 desired parent。
    // 直接用 entity_map 的迭代顺序（HashMap 不稳定）会让主 World 的 Children
    // vec 顺序每帧不同——HUD 元素就乱跳。
    //
    // 收集 (vm_child, vm_parent) 配对，并按 vm_parent 内的 Children 顺序排序：
    // 每父用 ChildOf 反向已经够——但我们要的是稳定的 child 内序，所以读父的
    // Children vec。
    use bevy_ecs::hierarchy::{ChildOf as VmChildOf, Children as VmChildren};
    let mut ordered: Vec<(Entity, Option<Entity>)> = Vec::new();
    for (&vm_entity, &render_entity) in &entity_map {
        let parent = world
            .get_entity(vm_entity)
            .ok()
            .and_then(|e| e.get::<VmChildOf>())
            .map(VmChildOf::parent)
            .and_then(|vm_parent| entity_map.get(&vm_parent).copied());
        // 顶层节点（无父）放进 ordered 直接用即可。
        if parent.is_none() {
            ordered.push((render_entity, None));
        }
    }
    // 对每个 VM 父实体，按其 Children vec 顺序追加到 ordered。这样同一父
    // 下的兄弟在 ordered 里相邻、且顺序与脚本 set_parent 调用顺序一致。
    for (&vm_parent, &render_parent) in &entity_map {
        let Some(children) = world
            .get_entity(vm_parent)
            .ok()
            .and_then(|e| e.get::<VmChildren>())
        else {
            continue;
        };
        use bevy_ecs::relationship::RelationshipTarget;
        for vm_child in children.iter() {
            let Some(&render_child) = entity_map.get(&vm_child) else {
                continue;
            };
            ordered.push((render_child, Some(render_parent)));
        }
    }

    // 第二步：按 ordered 顺序，每个 child 比对当前 ChildOf——不同才 insert。
    // ECS hook 在 insert ChildOf 时把 child 追加到父 Children 末尾，所以
    // 按 ordered 顺序 insert 即可让主 World 的 Children 顺序与 VM 一致。
    for (render_entity, desired_parent) in ordered {
        let current_parent = existing
            .get(render_entity)
            .ok()
            .flatten()
            .map(ChildOf::parent);
        if desired_parent == current_parent {
            continue;
        }
        match desired_parent {
            Some(render_parent) => {
                // 父变了——先 remove 让 hook 把 child 从旧父 Children 中清除，
                // 再 insert 让它追加到新父 Children 末尾，保持新父内顺序与
                // ordered 一致。仅在父真的不同时这样做。
                commands
                    .entity(render_entity)
                    .remove::<ChildOf>()
                    .insert(ChildOf(render_parent));
            }
            None => {
                commands.entity(render_entity).remove::<ChildOf>();
            }
        }
    }
}

// ============================================================================
// 公开入口 + 事件桥
// ============================================================================

/// 便于 viewer 把已构建的 [`VmWorld`] 注入 App 作 NonSend 资源。
///
/// 同时挂上 [`VmViewerPlugin`]。调用方仍需自行加 `DefaultPlugins`、相机与光照。
pub fn insert_vm_world(app: &mut App, vm: VmWorld) {
    app.insert_non_send_resource(vm).add_plugins(VmViewerPlugin);
}

/// 重置 [`VmViewerPlugin`] 内部的所有渲染映射资源。
///
/// 切换 [`VmWorld`] 时调用：旧 VM 的 entity id → Bevy entity id 映射现在指向
/// 已 despawn 的 Bevy 实体，必须清空，否则下一帧 sync 会尝试 `commands.entity()`
/// 已被释放的实体。调用方要自己负责把 `VmEntityRef` 标记的镜像实体 despawn。
pub fn reset_viewer_state(world: &mut World) {
    if let Some(mut cache) = world.get_resource_mut::<ResourceCache>() {
        *cache = ResourceCache::default();
    }
    if let Some(mut map) = world.get_resource_mut::<SpriteEntityMap>() {
        *map = SpriteEntityMap::default();
    }
    if let Some(mut map) = world.get_resource_mut::<MeshEntityMap>() {
        *map = MeshEntityMap::default();
    }
    if let Some(mut map) = world.get_resource_mut::<SceneEntityMap>() {
        *map = SceneEntityMap::default();
    }
    if let Some(mut map) = world.get_resource_mut::<CameraMap3d>() {
        *map = CameraMap3d::default();
    }
    if let Some(mut map) = world.get_resource_mut::<CameraMap2d>() {
        *map = CameraMap2d::default();
    }
    if let Some(mut map) = world.get_resource_mut::<TextMap>() {
        *map = TextMap::default();
    }
    if let Some(mut map) = world.get_resource_mut::<UiEntityMap>() {
        *map = UiEntityMap::default();
    }
}

/// Bevy event ↔ VM event bridge — one direction at a time.
///
/// Both sides own their own event storage:
/// - Bevy: the standard [`Events<T>`] resource, with `MessageReader<T>` /
///   `MessageWriter<T>` access.
/// - VM: the [`crate::event::EventStore`] driven by scripts and
///   [`VmWorld::send_event`] / [`VmWorld::drain_events`].
///
/// **Direction is opt-in per channel** to avoid feedback loops: pumping the
/// same Bevy event type both ways re-publishes everything pump_out emits
/// back into the VM, which floods the queue (1 real input → unbounded
/// growth across frames).
pub trait VmEventAppExt {
    /// Forward Bevy `T` events into the VM event channel `name`.
    fn add_vm_event_in<T>(&mut self, name: &'static str) -> &mut Self
    where
        T: Message + Clone + Send + Sync + 'static;

    /// Forward VM events on channel `name` out as Bevy `T` events.
    fn add_vm_event_out<T>(&mut self, name: &'static str) -> &mut Self
    where
        T: Message + Send + Sync + 'static;
}

impl VmEventAppExt for App {
    fn add_vm_event_in<T>(&mut self, name: &'static str) -> &mut Self
    where
        T: Message + Clone + Send + Sync + 'static,
    {
        let in_pump = move |mut reader: MessageReader<T>, mut vm: NonSendMut<VmWorld>| {
            let mut count = 0usize;
            for message in reader.read() {
                count += 1;
                if let Err(error) = vm.send_event::<T>(name, message.clone()) {
                    warn!("pump_in for `{name}` failed: {error}");
                }
            }
            if count > 0 {
                debug!(target: "bevy_vm::pump", "pump_in `{name}`: {count} event(s)");
            }
        };
        self.add_message::<T>()
            .add_systems(Update, in_pump.before(VmTickSet));
        self
    }

    fn add_vm_event_out<T>(&mut self, name: &'static str) -> &mut Self
    where
        T: Message + Send + Sync + 'static,
    {
        let out_pump = move |mut writer: MessageWriter<T>, mut vm: NonSendMut<VmWorld>| match vm
            .drain_events::<T>(name)
        {
            Ok(events) => {
                if !events.is_empty() {
                    debug!(target: "bevy_vm::pump", "pump_out `{name}`: {} event(s)", events.len());
                }
                for event in events {
                    writer.write(event);
                }
            }
            Err(error) => {
                warn!("pump_out for `{name}` failed: {error}");
            }
        };
        self.add_message::<T>()
            .add_systems(Update, out_pump.after(VmTickSet));
        self
    }
}

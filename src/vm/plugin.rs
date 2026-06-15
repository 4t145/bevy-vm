//! VM ↔ Bevy `App` 接入层。
//!
//! 单 World 架构后，本模块只剩两个职责：
//! 1. **驱动 [`VmInstance`] 每帧 tick**——通过 [`VmAppPlugin`] 装的
//!    exclusive system [`tick_all_vms`]。多个 VM 都活在 [`VmRegistry`]
//!    里，按声明顺序逐个 tick。
//! 2. **事件桥**：[`VmEventAppExt`] 把 Bevy 端 typed `Events<T>` 与 VM 的
//!    [`crate::event::EventStore`] 互相 pump。
//!
//! 不再有"sync 层"——VM 脚本直接挂 Bevy 原生 `Mesh3d`/`Camera`/`Sprite` 等
//! 组件到主 World 实体，渲染管线照常吃。

use crate::{VmInstance, VmRegistry};
use bevy::prelude::*;

/// 驱动所有 [`VmInstance`] 的 plugin。挂上后每帧自动推进 VM。
pub struct VmAppPlugin;

/// SystemSet covering the per-frame VM tick. Pump systems wired through
/// [`VmEventAppExt`] use `.before(VmTickSet)` / `.after(VmTickSet)` to
/// align with it.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct VmTickSet;

impl Plugin for VmAppPlugin {
    fn build(&self, app: &mut App) {
        if app.world().get_non_send_resource::<VmRegistry>().is_none() {
            app.insert_non_send_resource(VmRegistry::new());
        }
        app.add_systems(Update, tick_all_vms.in_set(VmTickSet));
    }
}

/// Insert a built [`VmInstance`] into the app's [`VmRegistry`] (creating
/// it if absent) and ensure [`VmAppPlugin`] is wired.
pub fn insert_vm_instance(app: &mut App, vm: VmInstance) -> crate::VmId {
    if app.world().get_non_send_resource::<VmRegistry>().is_none() {
        app.insert_non_send_resource(VmRegistry::new());
        app.add_plugins(VmAppPlugin);
    }
    let mut registry = app.world_mut().non_send_resource_mut::<VmRegistry>();
    registry.insert(vm)
}

/// Exclusive system: tick every active VM in declaration order. We pull
/// each `VmInstance` out of the registry, tick it (granting it `&mut World`
/// — registry stays out of the way during that), then put it back.
fn tick_all_vms(world: &mut World) {
    let dt = world
        .get_resource::<Time>()
        .map(|t| t.delta())
        .unwrap_or_default();
    let ids: Vec<_> = match world.get_non_send_resource::<VmRegistry>() {
        Some(reg) => reg.ids(),
        None => return,
    };
    for id in ids {
        let mut vm = match world
            .get_non_send_resource_mut::<VmRegistry>()
            .and_then(|mut r| r.remove(id))
        {
            Some(vm) => vm,
            None => continue,
        };
        vm.advance_time(dt);
        if let Err(error) = vm.tick(world) {
            warn!("VM {:?} tick failed: {error}", vm.id());
        }
        if let Some(mut reg) = world.get_non_send_resource_mut::<VmRegistry>() {
            reg.insert(vm);
        }
    }
}

/// Despawn every entity carrying [`crate::VmTag`] of `vm_id` — used by
/// the host (e.g. viewer) to wipe a VM's footprint when swapping worlds.
pub fn despawn_tagged_entities(world: &mut World, vm_id: crate::VmId) {
    let mut q = world.query::<(Entity, &crate::VmTag)>();
    let entities: Vec<Entity> = q
        .iter(world)
        .filter(|(_, tag)| tag.vm == vm_id)
        .map(|(e, _)| e)
        .collect();
    for entity in entities {
        if let Ok(em) = world.get_entity_mut(entity) {
            em.despawn();
        }
    }
}

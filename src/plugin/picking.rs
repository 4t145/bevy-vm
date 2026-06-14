//! Picking 桥：Bevy 的 picking observer 事件 → VM 端 typed event 通道。
//!
//! 单 World 后，picking 的 `Pointer<E>.entity` 直接就是 VM 端 entity——脚本
//! 可直接用 entity::to_bits() 反查。VmTag 用作 owner 验证：picking observer
//! 把事件分发到对应 [`crate::VmRegistry`] 中的 instance（按 entity 上的
//! VmTag 找 vm_id）。

use crate::VmInstanceBuilder;
use crate::error::VmError;
use crate::plugin::VmPlugin;
use crate::vm::id::VmTag;
use bevy::app::App;
use bevy::picking::events::{Click, Out, Over, Pointer};
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Channel name for [`Pointer<Click>`].
pub const PICK_CLICK: &str = "PickClick";
/// Channel name for [`Pointer<Over>`].
pub const PICK_OVER: &str = "PickOver";
/// Channel name for [`Pointer<Out>`].
pub const PICK_OUT: &str = "PickOut";

/// Click event payload exposed to scripts.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PickClickEvent {
    /// Entity id (`Entity::to_bits()`).
    pub entity: u64,
    /// Pointer button. `"Primary"`, `"Secondary"`, or `"Middle"`.
    pub button: String,
}

/// Hover-enter event payload.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PickOverEvent {
    /// Entity id.
    pub entity: u64,
}

/// Hover-leave event payload.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PickOutEvent {
    /// Entity id.
    pub entity: u64,
}

/// 把 Bevy 的 picking 事件桥接到 VM 端的 picking 通道。
pub struct PickingPlugin;

impl VmPlugin for PickingPlugin {
    fn build_vm(&self, builder: VmInstanceBuilder) -> Result<VmInstanceBuilder, VmError> {
        builder
            .with_event::<PickClickEvent>(PICK_CLICK)?
            .with_event::<PickOverEvent>(PICK_OVER)?
            .with_event::<PickOutEvent>(PICK_OUT)
    }

    fn build_app(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin);
        app.add_observer(on_pick_click);
        app.add_observer(on_pick_over);
        app.add_observer(on_pick_out);
    }
}

fn dispatch_to_vm<F>(
    entity: Entity,
    tags: &Query<&VmTag>,
    registry: &mut crate::VmRegistry,
    channel: &str,
    payload: F,
) where
    F: FnOnce() -> PickPayload,
{
    let Ok(tag) = tags.get(entity) else {
        return;
    };
    let Some(vm) = registry.get_mut(tag.vm) else {
        return;
    };
    let result = match payload() {
        PickPayload::Click(p) => vm.send_event::<PickClickEvent>(channel, p),
        PickPayload::Over(p) => vm.send_event::<PickOverEvent>(channel, p),
        PickPayload::Out(p) => vm.send_event::<PickOutEvent>(channel, p),
    };
    if let Err(error) = result {
        warn!(target: "bevy_vm::picking", "failed to forward `{channel}` to VM: {error}");
    }
}

enum PickPayload {
    Click(PickClickEvent),
    Over(PickOverEvent),
    Out(PickOutEvent),
}

fn on_pick_click(
    trigger: On<Pointer<Click>>,
    tags: Query<&VmTag>,
    mut registry: NonSendMut<crate::VmRegistry>,
) {
    let entity = trigger.event().entity;
    let button = pointer_button_name(trigger.event().event.button).to_owned();
    dispatch_to_vm(entity, &tags, &mut registry, PICK_CLICK, || {
        PickPayload::Click(PickClickEvent {
            entity: entity.to_bits(),
            button,
        })
    });
}

fn on_pick_over(
    trigger: On<Pointer<Over>>,
    tags: Query<&VmTag>,
    mut registry: NonSendMut<crate::VmRegistry>,
) {
    let entity = trigger.event().entity;
    dispatch_to_vm(entity, &tags, &mut registry, PICK_OVER, || {
        PickPayload::Over(PickOverEvent {
            entity: entity.to_bits(),
        })
    });
}

fn on_pick_out(
    trigger: On<Pointer<Out>>,
    tags: Query<&VmTag>,
    mut registry: NonSendMut<crate::VmRegistry>,
) {
    let entity = trigger.event().entity;
    dispatch_to_vm(entity, &tags, &mut registry, PICK_OUT, || {
        PickPayload::Out(PickOutEvent {
            entity: entity.to_bits(),
        })
    });
}

fn pointer_button_name(button: PointerButton) -> &'static str {
    match button {
        PointerButton::Primary => "Primary",
        PointerButton::Secondary => "Secondary",
        PointerButton::Middle => "Middle",
    }
}

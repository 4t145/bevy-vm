//! Picking 桥：Bevy 原生 `Pointer<E>` observer → Bevy `Messages<T>` →
//! VM 端 cursor 读。
//!
//! 拆桥后所有事件都走同一条路：Bevy `Messages<T>` 资源 + per-VM `EventCursor`。
//! Picking 因为 `Pointer<E>` 是 observer-only（不进 `Messages<T>`），需要一
//! 个 thin observer 把它**镜像**进我们自己定义的 `PickClickMessage` 等普通
//! Bevy `Message<T>`，然后脚本端用 `events("PickClick")` 走通用通道读出。
//!
//! 这层 mirror **不是**第二份 buffer——`Messages<T>` 本来就是 Bevy 自家事件
//! 系统的双缓冲，每帧自动 update（`Messages::update`）。我们只是把 observer
//! 的 trigger 重发为 message——思路与"用户脚本 emit 一个 Bevy Message"一致。

use crate::VmInstanceBuilder;
use crate::error::VmError;
use crate::plugin::VmPlugin;
use bevy::app::App;
use bevy::ecs::message::MessageWriter;
use bevy::picking::Pickable;
use bevy::picking::events::{Click, Out, Over, Pointer};
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Channel name for picking click events.
pub const PICK_CLICK: &str = "PickClick";
/// Channel name for picking hover-enter events.
pub const PICK_OVER: &str = "PickOver";
/// Channel name for picking hover-leave events.
pub const PICK_OUT: &str = "PickOut";

/// Click event payload exposed to scripts (and any Bevy system reading
/// `Messages<PickClickMessage>`).
#[derive(Message, Serialize, Deserialize, Debug, Clone)]
pub struct PickClickMessage {
    /// Entity id (`Entity::to_bits()`).
    pub entity: u64,
    /// Pointer button. `"Primary"`, `"Secondary"`, or `"Middle"`.
    pub button: String,
}

/// Hover-enter event payload.
#[derive(Message, Serialize, Deserialize, Debug, Clone)]
pub struct PickOverMessage {
    /// Entity id.
    pub entity: u64,
}

/// Hover-leave event payload.
#[derive(Message, Serialize, Deserialize, Debug, Clone)]
pub struct PickOutMessage {
    /// Entity id.
    pub entity: u64,
}

/// 把 Bevy 原生 picking observer 镜像到 VM 端可读的 `Messages<...>` 通道。
pub struct PickingPlugin;

impl VmPlugin for PickingPlugin {
    fn build_vm(&self, builder: VmInstanceBuilder) -> Result<VmInstanceBuilder, VmError> {
        builder
            .with_event::<PickClickMessage>(PICK_CLICK)?
            .with_event::<PickOverMessage>(PICK_OVER)?
            .with_event::<PickOutMessage>(PICK_OUT)
    }

    fn build_app(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin);
        app.add_observer(on_pick_click);
        app.add_observer(on_pick_over);
        app.add_observer(on_pick_out);
    }
}

/// 当前冒泡层级是不是脚本"认领"过的 picking target？
///
/// `attach_pickable(e)` 挂的是 `Pickable::default()`（hoverable=true，block=true），
/// 而 `pickable_ignore` 挂的是 `Pickable::IGNORE`（hoverable=false）。我们只把
/// 事件镜像到 hoverable 的层；其它层让 Bevy 的 propagation 继续冒泡，避免
/// 叶子文字 section 抢父按钮的事件。
fn is_dispatch_target(entity: Entity, pickables: &Query<&Pickable>) -> bool {
    pickables
        .get(entity)
        .map(|p| p.is_hoverable)
        .unwrap_or(false)
}

fn on_pick_click(
    trigger: On<Pointer<Click>>,
    pickables: Query<&Pickable>,
    mut writer: MessageWriter<PickClickMessage>,
) {
    let entity = trigger.event_target();
    if !is_dispatch_target(entity, &pickables) {
        return;
    }
    writer.write(PickClickMessage {
        entity: entity.to_bits(),
        button: pointer_button_name(trigger.event().event.button).to_owned(),
    });
}

fn on_pick_over(
    trigger: On<Pointer<Over>>,
    pickables: Query<&Pickable>,
    mut writer: MessageWriter<PickOverMessage>,
) {
    let entity = trigger.event_target();
    if !is_dispatch_target(entity, &pickables) {
        return;
    }
    writer.write(PickOverMessage {
        entity: entity.to_bits(),
    });
}

fn on_pick_out(
    trigger: On<Pointer<Out>>,
    pickables: Query<&Pickable>,
    mut writer: MessageWriter<PickOutMessage>,
) {
    let entity = trigger.event_target();
    if !is_dispatch_target(entity, &pickables) {
        return;
    }
    writer.write(PickOutMessage {
        entity: entity.to_bits(),
    });
}

fn pointer_button_name(button: PointerButton) -> &'static str {
    match button {
        PointerButton::Primary => "Primary",
        PointerButton::Secondary => "Secondary",
        PointerButton::Middle => "Middle",
    }
}

//! Picking 桥：Bevy 的 picking observer 事件 → VM 端 typed event 通道。
//!
//! Bevy 0.18 的 picking 是 **observer** 触发的——`commands.trigger(Pointer<Click>)`
//! 走 `Trigger`/`On` 同步调度，不进 `Events<T>` 队列，因此通用的 pump_in
//! ([`crate::render::VmEventAppExt::add_vm_event_in`]) 拿不到。
//!
//! 这里走 **observer 直通 [`EventStore`]** 的形态：
//! 1. `build_vm` 把 [`PICK_CLICK`] / [`PICK_OVER`] / [`PICK_OUT`] 三条 typed
//!    event 通道注册到 VM。
//! 2. `build_app` 装 Bevy 的 picking 后端（[`MeshPickingPlugin`] +
//!    [`SpritePickingPlugin`]），并为三种 picking 事件各装一个 observer，
//!    observer 内同步把事件写进 VM 的 [`EventStore`]。
//!
//! Observer 的 timing 不是普通 schedule 阶段——它在 picking 后端 `commands.trigger`
//! 那一刻立刻执行。但 VM 的事件双缓冲在 tick 末才 swap，所以脚本仍然下一
//! tick 才看到本帧 picking 事件——和 input pump 完全一致的"一帧延迟"语义。
//!
//! # 反向引用
//!
//! Picking 的 `Pointer<E>.entity` 是 **渲染端** 的实体 id。脚本不能用——
//! 它要的是 VM 端的 id。渲染同步层在 spawn 可 picking 的渲染实体时挂上一份
//! [`crate::render::VmEntityRef`]，observer 通过它反查 VM 实体并写入事件
//! payload 的 `entity` 字段（u64 表示 [`Entity::to_bits`]）。

use crate::VmWorldBuilder;
use crate::error::VmError;
use crate::plugin::VmPlugin;
use crate::render::VmEntityRef;
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
///
/// `entity` is [`Entity::to_bits`] of the **VM-side** entity (script-friendly
/// integer id). `button` is the serialized [`PointerButton`] variant name
/// (`"Primary"` / `"Secondary"` / `"Middle"`).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PickClickEvent {
    /// VM-side entity id (`Entity::to_bits()`).
    pub entity: u64,
    /// Pointer button. `"Primary"`, `"Secondary"`, or `"Middle"`.
    pub button: String,
}

/// Hover-enter event payload. `entity` field same as [`PickClickEvent`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PickOverEvent {
    /// VM-side entity id.
    pub entity: u64,
}

/// Hover-leave event payload. `entity` field same as [`PickClickEvent`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PickOutEvent {
    /// VM-side entity id.
    pub entity: u64,
}

/// 把 Bevy 的 picking 事件桥接到 VM 端的 [`PICK_CLICK`] / [`PICK_OVER`] /
/// [`PICK_OUT`] 通道。
///
/// 与 [`crate::plugin::input::InputPlugin`] 类似的 `&plugin` 两侧使用模式：
///
/// ```ignore
/// let plugin = bevy_vm::plugin::picking::PickingPlugin;
/// let vm = bevy_vm::VmWorldBuilder::new()
///     .add_plugin(&plugin)?
///     .load("world.ron")?;
/// app.add_vm_plugin(&plugin);
/// ```
pub struct PickingPlugin;

impl VmPlugin for PickingPlugin {
    fn build_vm(&self, builder: VmWorldBuilder) -> Result<VmWorldBuilder, VmError> {
        builder
            .with_event::<PickClickEvent>(PICK_CLICK)?
            .with_event::<PickOverEvent>(PICK_OVER)?
            .with_event::<PickOutEvent>(PICK_OUT)
    }

    fn build_app(&self, app: &mut App) {
        // Bevy 0.18 的 `DefaultPlugins` 已自动装 `DefaultPickingPlugins`
        // (PointerInputPlugin + PickingPlugin + InteractionPlugin) 与 sprite
        // backend；这里仅补一个 `MeshPickingPlugin` 让 3D 网格也能 pick，
        // 然后注册三个 observer 把 pointer 事件转发进 VM 的 EventStore。
        app.add_plugins(MeshPickingPlugin);
        app.add_observer(on_pick_click);
        app.add_observer(on_pick_over);
        app.add_observer(on_pick_out);
    }
}

/// Observer for [`Pointer<Click>`]: forward to VM's `PickClick` channel.
fn on_pick_click(
    trigger: On<Pointer<Click>>,
    refs: Query<&VmEntityRef>,
    mut vm: NonSendMut<crate::VmWorld>,
) {
    let render_entity = trigger.event().entity;
    let Ok(vm_ref) = refs.get(render_entity) else {
        return;
    };
    let payload = PickClickEvent {
        entity: vm_ref.0.to_bits(),
        button: pointer_button_name(trigger.event().event.button).to_owned(),
    };
    if let Err(error) = vm.send_event::<PickClickEvent>(PICK_CLICK, payload) {
        report_send_error(PICK_CLICK, &error);
    }
}

/// Observer for [`Pointer<Over>`]: forward to VM's `PickOver` channel.
fn on_pick_over(
    trigger: On<Pointer<Over>>,
    refs: Query<&VmEntityRef>,
    mut vm: NonSendMut<crate::VmWorld>,
) {
    let render_entity = trigger.event().entity;
    let Ok(vm_ref) = refs.get(render_entity) else {
        return;
    };
    let payload = PickOverEvent {
        entity: vm_ref.0.to_bits(),
    };
    if let Err(error) = vm.send_event::<PickOverEvent>(PICK_OVER, payload) {
        report_send_error(PICK_OVER, &error);
    }
}

/// Observer for [`Pointer<Out>`]: forward to VM's `PickOut` channel.
fn on_pick_out(
    trigger: On<Pointer<Out>>,
    refs: Query<&VmEntityRef>,
    mut vm: NonSendMut<crate::VmWorld>,
) {
    let render_entity = trigger.event().entity;
    let Ok(vm_ref) = refs.get(render_entity) else {
        return;
    };
    let payload = PickOutEvent {
        entity: vm_ref.0.to_bits(),
    };
    if let Err(error) = vm.send_event::<PickOutEvent>(PICK_OUT, payload) {
        report_send_error(PICK_OUT, &error);
    }
}

/// Bevy `PointerButton` → script-facing string name. Kept aligned with
/// the Bevy variant names so script authors can copy-paste from Bevy docs.
fn pointer_button_name(button: PointerButton) -> &'static str {
    match button {
        PointerButton::Primary => "Primary",
        PointerButton::Secondary => "Secondary",
        PointerButton::Middle => "Middle",
    }
}

/// Diagnostic for picking-channel send failures (unknown event name, type
/// mismatch, etc.). Logs and swallows — observer-level errors should not
/// kill the frame.
fn report_send_error(channel: &str, error: &VmError) {
    warn!(target: "bevy_vm::picking", "failed to forward `{channel}` to VM: {error}");
}

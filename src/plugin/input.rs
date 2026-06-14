//! Mouse + keyboard event bridge as a [`VmPlugin`].
//!
//! Bevy's `bevy_input` crate already derives `Serialize`/`Deserialize` on
//! every input event under its `serialize` feature (we enable it through
//! `bevy/serialize` in `Cargo.toml`), so the bridge is a thin pass-through:
//! `build_vm` registers each channel as a typed VM event under a stable
//! name, and `build_app` installs the matching pump systems via
//! [`crate::vm::VmEventAppExt::add_vm_event`].
//!
//! # Channel names
//!
//! Both Bevy-side host code and Rhai script authors refer to a channel by
//! the same string. Use the constants below — strings are easy to typo and
//! a misspelt channel name fails silently (no events ever appear).
//!
//! In Rhai:
//!
//! ```rhai
//! for ev in events("MouseButton") {
//!     // ev.button, ev.state, ev.window — exactly Bevy's struct fields
//! }
//! ```
//!
//! # What this plugin registers
//!
//! Mouse: [`MouseButtonInput`], [`MouseMotion`], [`MouseWheel`].
//!
//! Keyboard: [`KeyboardInput`], [`KeyboardFocusLost`].
//!
//! These are the events Bevy itself produces from `winit`. Higher-level
//! resources like `ButtonInput<KeyCode>` are not events and stay on the
//! Bevy side; if a script needs current-frame button state, expose it as
//! a typed event of your own and forward it from a Bevy system.

use crate::VmInstanceBuilder;
use crate::error::VmError;
use crate::plugin::VmPlugin;
use crate::vm::VmEventAppExt;
use bevy::app::App;
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseWheel};
use bevy::window::CursorMoved;

/// Channel name for [`MouseButtonInput`].
pub const MOUSE_BUTTON: &str = "MouseButton";
/// Channel name for [`MouseMotion`] — raw device-level deltas. May not fire
/// on every platform; prefer [`CURSOR_MOVED`] for cursor-following UI.
pub const MOUSE_MOTION: &str = "MouseMotion";
/// Channel name for [`MouseWheel`].
pub const MOUSE_WHEEL: &str = "MouseWheel";
/// Channel name for [`CursorMoved`] — cursor position changes inside windows,
/// with optional `delta` since last event. Fires reliably across platforms.
pub const CURSOR_MOVED: &str = "CursorMoved";
/// Channel name for [`KeyboardInput`].
pub const KEYBOARD_INPUT: &str = "KeyboardInput";
/// Channel name for [`KeyboardFocusLost`].
pub const KEYBOARD_FOCUS_LOST: &str = "KeyboardFocusLost";

/// Bridges Bevy's mouse + keyboard events to the VM event store.
///
/// Add the **same instance** on both sides:
///
/// ```ignore
/// use bevy_vm::{VmInstanceBuilder, plugin::{AppVmPluginExt, BuilderVmPluginExt, input::InputPlugin}};
/// # use bevy::app::App;
/// let plugin = InputPlugin;
/// let vm = VmInstanceBuilder::new().add_plugin(&plugin)?.load("world.ron")?;
/// let mut app = App::new();
/// app.add_vm_plugin(&plugin);
/// # Ok::<(), bevy_vm::VmError>(())
/// ```
///
/// Only need a subset? Don't use this plugin: register the events you want
/// directly with [`VmInstanceBuilder::with_event`] and
/// [`crate::vm::VmEventAppExt::add_vm_event`].
pub struct InputPlugin;

impl VmPlugin for InputPlugin {
    fn build_vm(&self, builder: VmInstanceBuilder) -> Result<VmInstanceBuilder, VmError> {
        builder
            .with_event::<MouseButtonInput>(MOUSE_BUTTON)?
            .with_event::<MouseMotion>(MOUSE_MOTION)?
            .with_event::<MouseWheel>(MOUSE_WHEEL)?
            .with_event::<CursorMoved>(CURSOR_MOVED)?
            .with_event::<KeyboardInput>(KEYBOARD_INPUT)?
            .with_event::<KeyboardFocusLost>(KEYBOARD_FOCUS_LOST)
    }

    fn build_app(&self, app: &mut App) {
        app.add_vm_event_in::<MouseButtonInput>(MOUSE_BUTTON)
            .add_vm_event_in::<MouseMotion>(MOUSE_MOTION)
            .add_vm_event_in::<MouseWheel>(MOUSE_WHEEL)
            .add_vm_event_in::<CursorMoved>(CURSOR_MOVED)
            .add_vm_event_in::<KeyboardInput>(KEYBOARD_INPUT)
            .add_vm_event_in::<KeyboardFocusLost>(KEYBOARD_FOCUS_LOST);
    }
}
